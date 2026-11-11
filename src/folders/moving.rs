//! Moving a folder or backing it up: writers held off, the copy verified, the original
//! retired, never deleted. Interrupted moves are finished on the next scan.

use super::*;

impl Registry {
    /// Move a folder to another directory, keeping every UUID. Writers are held
    /// off for the whole move, and on another drive the new copy is verified
    /// before the original is retired. Across drives the original is moved to
    /// `archive/` beside itself and is returned, for the person to delete once
    /// they are satisfied; Geniuz never deletes it.
    pub fn relocate(&self, query: &str, dest: &Path) -> Result<Relocated, String> {
        self.with_registry_lock(|| self.relocate_unlocked(query, dest))
    }

    pub(super) fn relocate_unlocked(&self, query: &str, dest: &Path) -> Result<Relocated, String> {
        let folder = self.find(query)?;
        let dir = folder
            .db_path
            .parent()
            .ok_or_else(|| "That folder has no parent directory.".to_string())?
            .to_path_buf();
        if canonical(&dir) == canonical(&self.home) {
            return Err("Your first folder stays where it is, because other folders live beside it.".into());
        }
        refuse_symlink(&dir)?;
        std::fs::create_dir_all(dest)
            .map_err(|e| format!("Couldn't create {}: {}", dest.display(), e))?;
        let dest = canonical(dest);
        if dir.parent().map(canonical).as_deref() == Some(dest.as_path()) {
            return Err(format!("\"{}\" is already in {}.", folder.name, dest.display()));
        }
        let known = dest == canonical(&self.home) || self.added().iter().any(|d| canonical(d) == dest);
        if !known && !candidates(&dest).is_empty() {
            return Err(format!(
                "{} already holds memory stores. Add it first with `geniuz folder add`.",
                dest.display()
            ));
        }
        let target = dest.join(&folder.uuid);
        if target.exists() {
            return Err(format!("{} already exists.", target.display()));
        }

        // Put the destination on the list first. If that fails, nothing has moved.
        // If a later step fails, an extra directory on the list does no harm.
        let was_added_directly = self.added().iter().any(|d| canonical(d) == canonical(&dir));
        self.remember_added_unlocked(&dest)?;

        let lock = quiesce(&folder.db_path)?;
        let left_behind = match std::fs::rename(&dir, &target) {
            Ok(()) => {
                drop(lock);
                None
            }
            Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
                Some(copy_then_retire(&dir, &dest, &folder, lock)?)
            }
            Err(_) => {
                // Windows won't rename a directory holding an open file. Nothing
                // has been lost, because the -wal file moves with the directory.
                drop(lock);
                std::fs::rename(&dir, &target)
                    .map_err(|e| format!("Couldn't move {}: {}", dir.display(), e))?;
                None
            }
        };

        if was_added_directly {
            let _ = self.remove_unlocked(&dir);
        }
        // The directory it moved out of stays on the list only while it still
        // holds a folder. Otherwise the list keeps an empty entry, which reads
        // as "unavailable" the day someone deletes that directory.
        if let Some(old_parent) = dir.parent() {
            let listed = self.added().iter().any(|d| canonical(d) == canonical(old_parent));
            if listed && candidates(old_parent).is_empty() {
                let _ = self.remove_unlocked(old_parent);
            }
        }
        Ok(Relocated { folder: self.find(&folder.uuid)?, left_behind })
    }
}

/// Write a complete backup of a live database to one file: flush the `-wal`
/// file and hold off writers first, since a plain copy can miss what is in it.
/// The backup is a file to keep, not a folder to use: it carries the same UUIDs,
/// so `add` refuses it while the original is present.
pub fn backup_to(db: &Path, target: &Path) -> Result<u64, String> {
    let lock = quiesce(db)?;
    let bytes = std::fs::copy(db, target).map_err(|e| format!("Copy failed: {}", e))?;
    drop(lock);
    Ok(bytes)
}

/// Flush the `-wal` file into the database and hold off writers. The returned
/// connection keeps them off until it is dropped.
pub(super) fn quiesce(db: &Path) -> Result<Connection, String> {
    let wal = PathBuf::from(format!("{}-wal", db.display()));
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        let conn = open_read_write(db)?;
        conn.busy_timeout(std::time::Duration::from_secs(2))
            .map_err(|e| format!("Couldn't open {}: {}", db.display(), e))?;
        let busy: i64 = conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
            .unwrap_or(1);
        if busy != 0 || conn.execute_batch("BEGIN IMMEDIATE").is_err() {
            continue;
        }
        // A writer may have slipped in between the flush and the lock.
        if std::fs::metadata(&wal).map(|m| m.len() == 0).unwrap_or(true) {
            return Ok(conn);
        }
    }
    Err("Something is using this folder right now. Close it there and try again.".into())
}

/// Whether a store has been fenced off because it moved to another drive.
pub(super) fn fenced(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'trigger' AND name = ?1",
        [format!("{}_memories_insert", FENCE_TRIGGER)],
        |_| Ok(()),
    )
    .is_ok()
}

/// Triggers that refuse every change to a store's contents, in the file itself.
/// Only tables the file has: one last opened by an older Geniuz has no image
/// tables yet, and a trigger on a missing table would stop the whole move.
///
/// Fails if the file can't be asked which tables it has: guessing "none" would
/// close off only part of it, and the retired original could still take writes.
pub(super) fn fence_sql(conn: &Connection) -> Result<String, String> {
    let mut sql = String::new();
    for table in ["memories", "memory_embeddings", "embedding_meta", "_meta", "images", "memory_images"] {
        let exists: i64 = conn
            .query_row("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [table], |r| r.get(0))
            .map_err(|e| format!("Couldn't read the folder's tables: {}", e))?;
        if exists == 0 {
            continue;
        }
        for op in ["insert", "update", "delete"] {
            sql.push_str(&format!(
                "CREATE TRIGGER {f}_{t}_{o} BEFORE {o} ON {t} BEGIN SELECT RAISE(ABORT, 'This folder has moved to another drive'); END;\n",
                f = FENCE_TRIGGER,
                t = table,
                o = op
            ));
        }
    }
    Ok(sql)
}

/// Move across drives. Every step can be interrupted without losing a memory or
/// leaving two live copies:
///
///  1. Make sure the original has somewhere to retire to.
///  2. Copy into a hidden directory, force it to disk, and verify it.
///  3. In the transaction already holding writers off, fence the original with
///     triggers. From here it refuses every change, durably, including from a
///     connection opened before the move or a writer waiting on the lock.
///  4. Record in the copy that it is final.
///  5. Retire the original, then put the copy in place.
///
/// If this stops partway, `finish_interrupted_moves` completes it or undoes it.
/// It leaves this alone while `MOVE_OWNER` is locked, which it is until this returns.
pub(super) fn copy_then_retire(dir: &Path, dest: &Path, folder: &Folder, lock: Connection) -> Result<PathBuf, String> {
    let expected: i64 = lock
        .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
        .map_err(|e| format!("Couldn't read {}: {}", dir.display(), e))?;

    let parent = dir.parent().ok_or_else(|| "That folder has no parent directory.".to_string())?;
    let archive = parent.join(ARCHIVE_DIR);
    std::fs::create_dir_all(&archive)
        .map_err(|e| format!("Couldn't create {}: {}", archive.display(), e))?;

    let staging = dest.join(format!("{}{}", MOVING_PREFIX, folder.uuid));
    if staging.exists() {
        return Err(format!("An earlier move left {} behind. Check it, then move it aside.", staging.display()));
    }
    std::fs::create_dir_all(&staging)
        .map_err(|e| format!("Couldn't create {}: {}", staging.display(), e))?;
    let owner = std::fs::File::create(staging.join(MOVE_OWNER))
        .map_err(|e| format!("Couldn't write in {}: {}", staging.display(), e))?;
    owner
        .lock()
        .map_err(|e| format!("Couldn't lock {}: {}", staging.display(), e))?;
    let entries = std::fs::read_dir(dir).map_err(|e| format!("Couldn't read {}: {}", dir.display(), e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with("-wal") || name.ends_with("-shm") {
            continue;
        }
        if !path.is_file() {
            return Err(format!("{} holds something other than files, so it wasn't moved.", dir.display()));
        }
        let copied = staging.join(entry.file_name());
        std::fs::copy(&path, &copied).map_err(|e| format!("Couldn't copy {}: {}", path.display(), e))?;
        // Opened for writing: Windows refuses to flush a file opened only to read
        // ("Access is denied"), which stopped every move across drives there.
        std::fs::OpenOptions::new().write(true).open(&copied)
            .and_then(|f| f.sync_all())
            .map_err(|e| format!("Couldn't save {}: {}", copied.display(), e))?;
    }

    verify_copy(&staging.join(DB_FILE), folder, expected)
        .map_err(|e| format!("{} The original is untouched; the partial copy is at {}.", e, staging.display()))?;

    let fence = fence_sql(&lock).map_err(|e| format!("Couldn't close off the original: {}. Nothing moved.", e))?;
    lock.execute_batch(&format!("{}COMMIT;", fence))
        .map_err(|e| format!("Couldn't close off the original: {}. Nothing moved.", e))?;
    drop(lock);
    let committed = staging.join(MOVE_COMMITTED);
    std::fs::File::create(&committed)
        .and_then(|f| f.sync_all())
        .map_err(|e| format!("Couldn't record the move in {}: {}. Geniuz will finish it next time it runs.", staging.display(), e))?;

    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let retired = archive.join(format!("{}{}{}", folder.uuid, MOVED_MARK, stamp));
    std::fs::rename(dir, &retired)
        .map_err(|e| format!("Couldn't retire {}: {}. Geniuz will finish this next time it runs.", dir.display(), e))?;
    drop(owner);
    publish(&staging, &dest.join(&folder.uuid))
        .map_err(|e| format!("{} Geniuz will finish this next time it runs.", e))?;
    Ok(retired)
}

/// Put a finished copy in place, without the two files that tracked the move.
pub(super) fn publish(staging: &Path, target: &Path) -> Result<(), String> {
    std::fs::rename(staging, target)
        .map_err(|e| format!("Couldn't finish the move to {}: {}.", target.display(), e))?;
    for name in [MOVE_OWNER, MOVE_COMMITTED] {
        let _ = std::fs::remove_file(target.join(name));
    }
    Ok(())
}

/// Finish or undo a move across drives that was interrupted. Returns whether
/// anything changed. A move still running is left alone. A copy whose original
/// still accepts writes may be out of date, so it is set aside. A copy can only
/// be published with proof that it is final: its original is fenced, or the copy
/// records that it was. If neither can be seen, it waits, because a sound file
/// isn't proof: the original may hold newer memories on a drive that's unplugged.
pub(super) fn finish_interrupted_moves(root: &Path, live: &[Folder], paused: &[Folder], pending: &mut Vec<PathBuf>) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else { return false };
    let mut changed = false;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(uuid) = name.strip_prefix(MOVING_PREFIX) else { continue };
        let staging = entry.path();

        // A lock that can be taken means the process moving this has stopped.
        let owner = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(staging.join(MOVE_OWNER));
        let Ok(owner) = owner else { continue };
        if owner.try_lock().is_err() {
            continue;
        }

        let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        if live.iter().any(|f| f.uuid.eq_ignore_ascii_case(uuid)) {
            let archive = root.join(ARCHIVE_DIR);
            let _ = std::fs::create_dir_all(&archive);
            let aside = archive.join(format!("{}{}{}", uuid, ABANDONED_MARK, stamp));
            changed |= std::fs::rename(&staging, aside).is_ok();
            continue;
        }
        let original = paused.iter().find(|f| f.uuid.eq_ignore_ascii_case(uuid));
        if original.is_none() && !staging.join(MOVE_COMMITTED).exists() {
            pending.push(staging);
            continue;
        }

        // Only something that really is this folder, complete and readable.
        let db = staging.join(DB_FILE);
        let is_it = read_identity(&db).is_some_and(|f| f.uuid.eq_ignore_ascii_case(uuid));
        let sound = open_read_only(&db)
            .and_then(|c| c.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)).map_err(|e| e.to_string()))
            .is_ok_and(|r| r == "ok");
        if !is_it || !sound {
            pending.push(staging);
            continue;
        }
        if let Some(original) = original {
            let Some(dir) = original.db_path.parent() else { continue };
            let Some(parent) = dir.parent() else { continue };
            let archive = parent.join(ARCHIVE_DIR);
            let _ = std::fs::create_dir_all(&archive);
            let retired = archive.join(format!("{}{}{}", uuid, MOVED_MARK, stamp));
            if std::fs::rename(dir, retired).is_err() {
                continue;
            }
        }
        drop(owner);
        changed |= publish(&staging, &root.join(uuid)).is_ok();
    }
    changed
}

pub(super) fn refuse_symlink(dir: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(dir) {
        Ok(m) if m.file_type().is_symlink() => Err(format!(
            "{} is a link to a folder elsewhere. Move the folder itself, or remove the link.",
            dir.display()
        )),
        _ => Ok(()),
    }
}

pub(super) fn verify_copy(db: &Path, folder: &Folder, expected: i64) -> Result<(), String> {
    let conn = open_read_only(db)?;
    let check: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| format!("Couldn't check the copy: {}.", e))?;
    if check != "ok" {
        return Err(format!("The copy failed its integrity check: {}.", check));
    }
    if get_meta(&conn, KEY_UUID).as_deref() != Some(folder.uuid.as_str()) {
        return Err("The copy has a different identity.".into());
    }
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
        .map_err(|e| format!("Couldn't check the copy: {}.", e))?;
    if count != expected {
        return Err(format!("The copy holds {} memories, not {}.", count, expected));
    }
    Ok(())
}

//! Importing a folder from an exported file: the other half of `export`.
//!
//! An export is one file holding a whole folder, with the folder's ID and every
//! memory's ID inside it. Importing puts a copy of that file where Geniuz reads, as
//! the same folder: same ID, same memories, same threads, so addresses written
//! anywhere still open. Nothing is merged. A folder that is already here, in use or
//! archived, is refused, because two copies of one folder have no original.
//!
//! The file imported from is read as plain bytes and never opened as a database, so
//! it is not changed, and a file on read-only media imports the same way. Every check
//! is made on the copy, in a hidden directory Geniuz doesn't read, and the copy takes
//! its place in one step only when all of them have passed.

use super::*;

const IMPORTING_PREFIX: &str = ".importing-";
const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";
const IMPORTED_NAME: &str = "Imported folder";

/// What an import brought in.
#[derive(Debug)]
pub struct Imported {
    pub folder: Folder,
    pub memories: i64,
    /// The name the folder carried in the file, when `--as` gave it another.
    pub was_called: Option<String>,
}

impl Registry {
    /// Bring an exported folder in as a folder here, under the name it carries or
    /// the one given.
    pub fn import(&self, file: &Path, name: Option<&str>) -> Result<Imported, String> {
        self.with_registry_lock(|| self.import_unlocked(file, name))
    }

    fn import_unlocked(&self, file: &Path, name: Option<&str>) -> Result<Imported, String> {
        let wanted = name.map(validate_name).transpose()?;
        let source = self.check_source(file)?;

        // A hidden directory in the data directory: not read as a folder, and on the
        // same drive as its final place, so taking that place is one step.
        self.clear_dead_imports();
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let staging = self.home.join(format!("{}{}-{}", IMPORTING_PREFIX, std::process::id(), n));
        make_private_dir(&staging)?;
        let done = self.import_staged(&source, &staging, wanted.as_deref());
        if done.is_err() {
            // Only the copy this call made is removed. The file imported from is never touched.
            let _ = std::fs::remove_dir_all(&staging);
        }
        done
    }

    /// What can be refused before anything is copied.
    fn check_source(&self, file: &Path) -> Result<PathBuf, String> {
        let source = std::fs::canonicalize(file)
            .map(plain_path)
            .map_err(|e| format!("Couldn't open {}: {}", file.display(), e))?;
        if source.is_dir() {
            return Err(format!(
                "{} is a directory. Import takes the one file an export wrote. To use folders where they are, see `geniuz folder add`.",
                file.display()
            ));
        }
        // A pipe or a device would be waited on forever, with the registry lock held.
        if !std::fs::metadata(&source).map(|m| m.is_file()).unwrap_or(false) {
            return Err(not_a_folder(file));
        }
        let scan = self.scan();
        let in_use = scan.folders.iter().map(|f| &f.db_path).chain(scan.blocked.iter().map(|b| &b.db_path));
        if in_use.into_iter().any(|p| canonical(p) == source) {
            return Err(format!("{} is already one of your folders.", file.display()));
        }
        let mut head = [0u8; 16];
        let is_sqlite = std::fs::File::open(&source)
            .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut head))
            .map(|_| &head == SQLITE_MAGIC)
            .unwrap_or(false);
        if !is_sqlite {
            return Err(not_a_folder(file));
        }
        // A plain copy of a folder that was in use can have its newest memories in a
        // file beside it. Importing the main file alone would leave them out without
        // a word, so it is refused with the way to make a complete one.
        // The same goes for a `-journal` file: a change that was under way and never finished.
        for suffix in ["-wal", "-journal"] {
            // Built from the name itself, not its text: a name that isn't valid text
            // would otherwise give a path that doesn't exist, and the check would pass.
            let mut beside = source.as_os_str().to_owned();
            beside.push(suffix);
            let beside = PathBuf::from(beside);
            if std::fs::metadata(&beside).map(|m| m.len() > 0).unwrap_or(false) {
                return Err(format!(
                    "{} has unsaved changes in a file beside it ({}), so it looks like a folder in use, not an export. \
                     Make a complete copy where it lives with `geniuz folder export`, and import that.",
                    file.display(),
                    beside.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
        Ok(source)
    }

    fn import_staged(&self, source: &Path, staging: &Path, wanted: Option<&str>) -> Result<Imported, String> {
        let copy = staging.join(DB_FILE);
        std::fs::copy(source, &copy).map_err(|e| format!("Couldn't copy {}: {}", source.display(), e))?;
        make_private_file(&copy)?;
        sync(&copy)?;

        // A file is something people are sent, and a database file carries more than
        // rows: it can hold triggers and views of its writer's choosing, which run later,
        // inside Geniuz, whenever the folder is written to. The copy is opened without
        // trusting any of that, and none of it is run before it is gone (below).
        let conn = open_untrusted(&copy)?;
        for table in ["memories", "_meta"] {
            let kind: Option<String> = conn
                .query_row("SELECT type FROM sqlite_master WHERE name = ?1", [table], |r| r.get(0))
                .optional()
                .map_err(|_| not_a_folder(source))?;
            match kind.as_deref() {
                Some("table") => {}
                // No memories table at all is a store from before 4.0, said further down.
                None if table == "memories" => {}
                _ => return Err(not_a_folder(source)),
            }
        }
        let health: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|e| format!("{} couldn't be checked and wasn't imported ({}).", source.display(), e))?;
        if health != "ok" {
            return Err(format!("{} is damaged and wasn't imported ({}).", source.display(), first_line(&health)));
        }
        let ours = get_meta(&conn, "product").map(|p| crate::db::OWN_PRODUCTS.contains(&p.as_str())).unwrap_or(false);
        if !ours && get_meta(&conn, KEY_UUID).is_none() {
            return Err(not_a_folder(source));
        }
        if memory_table(&conn).is_err() {
            return Err(format!(
                "{} is from a Geniuz before 4.0. Back it up and run {} on it, then import it.",
                source.display(),
                crate::db::MIGRATION_SCRIPT
            ));
        }
        if fenced(&conn) {
            return Err(format!(
                "{} is the closed original of a folder that moved to another drive. Export the folder from where it lives now, and import that.",
                source.display()
            ));
        }
        only_our_schema(&conn).map_err(|e| format!("{} wasn't imported: {}", source.display(), e))?;
        plain_columns(&conn).map_err(|e| format!("{} wasn't imported: {}", source.display(), e))?;
        let memories: i64 = conn
            .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
            .map_err(|e| format!("Couldn't read {}: {}", source.display(), e))?;
        drop(conn);
        // "None of its memories is already here" is checked by ID further down, and
        // an ID that can't be read as text would not be compared at all. Every memory
        // must have one that can.
        if memory_ids(&copy).len() as i64 != memories {
            return Err(format!(
                "{} wasn't imported: some of its memories have IDs Geniuz can't read, so it can't tell whether they are already here.",
                source.display()
            ));
        }

        // A store from 4.x has no identity yet. It gets the one every copy of it
        // would get, named after the file.
        stamp(&copy, &name_from_file(source))?;
        let found = read_identity(&copy).ok_or_else(|| not_a_folder(source))?;
        // The ID as Geniuz writes it, in either case. Any other spelling of the same
        // ID (braces, no hyphens) would pass for a different folder everywhere IDs are
        // compared as text.
        let uuid = uuid::Uuid::parse_str(&found.uuid)
            .ok()
            .map(|u| u.to_string().to_uppercase())
            .filter(|u| u.eq_ignore_ascii_case(&found.uuid))
            .ok_or_else(|| format!("{} has a folder ID Geniuz can't read (\"{}\").", source.display(), found.uuid))?;
        let short: String = uuid.chars().take(SHORT_ID).collect();

        // The same folder, anywhere here.
        let scan = self.scan();
        let same = |u: &str| u.eq_ignore_ascii_case(&uuid);
        // "Not already here" can only be said of places that can be looked at. A folder on
        // a drive that is unplugged right now would come back as a second copy of this one.
        if let Some(why) = &scan.first_folder_problem {
            return Err(format!("Not imported. {}", why));
        }
        if !scan.unavailable.is_empty() || !scan.pending_moves.is_empty() {
            let places: Vec<String> = scan.unavailable.iter().chain(scan.pending_moves.iter()).map(|p| p.display().to_string()).collect();
            return Err(format!(
                "Not imported. Geniuz can't look in {} right now, so it can't tell whether this folder is already there. \
                 Reconnect it and run `geniuz folder list`, or take it off the list with `geniuz folder remove`, then import.",
                places.join(" or ")
            ));
        }
        if let Some(at) = self.mid_move(&uuid) {
            return Err(format!(
                "This folder is already here, part-way through a move ({}). Run `geniuz folder list` with both drives connected to finish it. Nothing was changed.",
                at.display()
            ));
        }
        if let Some(f) = scan.folders.iter().find(|f| same(&f.uuid)) {
            return Err(format!(
                "This folder is already here: {} ({}). Importing never merges or replaces; nothing was changed.",
                f.name,
                f.short_id()
            ));
        }
        if let Some(b) = scan.blocked.iter().find(|b| same(&b.uuid)) {
            return Err(format!("This folder is already here, at {}. Nothing was changed.", b.db_path.display()));
        }
        let archived = self.archived();
        if let Some(f) = archived.iter().find(|f| same(&f.uuid)) {
            return Err(format!(
                "This folder is already here, archived: {} ({}). Bring it back with `geniuz folder restore {}`. Nothing was changed.",
                f.name,
                f.short_id(),
                f.short_id()
            ));
        }
        // Its memories, under any folder ID: a copy that was given another identity.
        let mut held: HashMap<String, (&str, &str)> = HashMap::new();
        let known = scan.folders.iter().chain(archived.iter()).map(|f| (f.name.as_str(), f.uuid.as_str(), &f.db_path));
        let blocked = scan.blocked.iter().map(|b| (b.name.as_str(), b.uuid.as_str(), &b.db_path));
        for (name, id, db) in known.chain(blocked) {
            for memory in memory_ids(db) {
                held.insert(memory, (name, id));
            }
        }
        if let Some((name, id)) = memory_ids(&copy).iter().find_map(|m| held.get(m)) {
            return Err(format!(
                "Not imported. This file holds memories that are already in {} ({}). Copies aren't supported.",
                name,
                id.chars().take(SHORT_ID).collect::<String>().to_uppercase()
            ));
        }

        // Its name. Two folders with one name can't be told apart in a list, and a
        // keep by name would be refused, so the name must be free before it arrives.
        let name = match wanted {
            Some(name) => name.to_string(),
            // A file is something people are sent. The name inside it is held to the same
            // rules as one typed here: a name that looks like another folder's ID would
            // otherwise be found before that folder.
            None => validate_name(&found.name).map_err(|e| {
                format!("The name this folder carries can't be used here ({}) Give it one: geniuz folder import \"{}\" --as \"<NAME>\"", e, source.display())
            })?,
        };
        if let Some(other) = scan.folders.iter().find(|f| name_key(&f.name) == name_key(&name)) {
            let mut why = format!(
                "There's already a folder called \"{}\" ({}). Import this one under another name:\n  geniuz folder import \"{}\" --as \"<NAME>\"",
                other.name,
                other.short_id(),
                source.display()
            );
            // An empty folder with this name is most often a placeholder made for the
            // one arriving. Archiving it frees the name; the first folder can't be archived.
            let empty = other.stats().map(|(n, _)| n == 0).unwrap_or(false);
            if empty && self.can_archive(other) {
                why.push_str(&format!(
                    "\n{} is empty. If it was made as a placeholder for this folder, archive it and import again:\n  geniuz folder archive {}",
                    other.name,
                    other.short_id()
                ));
            }
            return Err(why);
        }
        let was_called = (name != found.name).then(|| found.name.clone());
        if was_called.is_some() {
            replace_meta(&copy, KEY_NAME, &name)?;
        }

        // The folder about to take its place is the one that was checked: nothing run
        // on the copy since has changed which folder it says it is.
        let checked = read_identity(&copy).ok_or_else(|| not_a_folder(source))?;
        if !same(&checked.uuid) || checked.name != name {
            return Err(format!("{} changed its own identity while being imported, and was refused.", source.display()));
        }

        // Leave one complete file, then take its place.
        let conn = open_read_write(&copy)?;
        let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
        drop(conn);
        sync(&copy)?;
        let target = self.home.join(&uuid);
        if std::fs::symlink_metadata(&target).is_ok() {
            return Err(format!(
                "{} already exists, and Geniuz doesn't read it as folder {}. Move it out of the way first; nothing was changed.",
                target.display(),
                short
            ));
        }
        // On Windows a directory can't be renamed while anything holds a file in it
        // open, and a virus scanner or an indexer may be reading the new copy for a
        // moment. Nothing of Geniuz's own is open in it by now, so a short wait is enough.
        // First the directory itself, on disk: on some file systems a crash after the
        // rename could otherwise bring the folder back without the file in it.
        #[cfg(unix)]
        std::fs::File::open(staging)
            .and_then(|d| d.sync_all())
            .map_err(|e| format!("Couldn't finish writing {}: {}", staging.display(), e))?;
        #[allow(unused_mut)]
        let mut placed = std::fs::rename(staging, &target);
        // Only there, and only for that refusal: anywhere else an error is final, and
        // waiting would hold the registry lock for nothing.
        #[cfg(windows)]
        for _ in 0..10 {
            if !matches!(&placed, Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
            placed = std::fs::rename(staging, &target);
        }
        placed.map_err(|e| format!("Couldn't put the folder in place: {}", e))?;
        // The new name, on disk: a crash now must not leave the folder half there.
        #[cfg(unix)]
        if let Ok(home) = std::fs::File::open(&self.home) {
            let _ = home.sync_all();
        }
        // It is in place from here on, so this can no longer fail: what was checked a
        // moment ago is reported, at its new path.
        let db_path = target.join(DB_FILE);
        let folder = read_identity(&db_path).unwrap_or(Folder { db_path, ..checked });
        Ok(Imported { folder, memories, was_called })
    }
}

impl Registry {
    /// Remove what an import that was killed left behind: a whole copy of someone's
    /// notes in a hidden directory nobody would find. Every import runs from start to
    /// finish under the registry lock, which this caller holds, so any such directory
    /// here belongs to a process that has stopped. Only real directories with this
    /// prefix, directly in the data directory; a link is left alone.
    fn clear_dead_imports(&self) {
        let Ok(entries) = std::fs::read_dir(&self.home) else { return };
        for entry in entries.flatten() {
            let ours = entry.file_name().to_string_lossy().starts_with(IMPORTING_PREFIX);
            let real_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if ours && real_dir {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }

    /// Whether `folder archive` would take this folder: not the first folder, not a
    /// link, and not a directory that was added directly.
    fn can_archive(&self, folder: &Folder) -> bool {
        let Some(dir) = folder.db_path.parent() else { return false };
        let linked = std::fs::symlink_metadata(dir).map(|m| m.file_type().is_symlink()).unwrap_or(true);
        !linked
            && canonical(dir) != canonical(&self.home)
            && !self.added().iter().any(|d| canonical(d) == canonical(dir))
    }

    /// Where this folder is if it is here but between two places: closed for a move to
    /// another drive, or the copy such a move was making. Neither is in a scan's folders.
    fn mid_move(&self, uuid: &str) -> Option<PathBuf> {
        let mut roots = vec![self.home.clone()];
        roots.extend(self.added().into_iter().filter(|d| d.is_dir()));
        let (_, paused) = read_all(&roots);
        if let Some(f) = paused.iter().find(|f| f.uuid.eq_ignore_ascii_case(uuid)) {
            return Some(f.db_path.clone());
        }
        let moving = format!("{}{}", MOVING_PREFIX, uuid).to_lowercase();
        roots.iter().find_map(|root| {
            std::fs::read_dir(root).ok()?.flatten().map(|e| e.path()).find(|p| {
                p.file_name().map(|n| n.to_string_lossy().to_lowercase() == moving).unwrap_or(false)
            })
        })
    }
}

/// Open a copy of a file from somewhere else. What its schema says is not trusted to
/// call anything beyond SQLite's harmless functions, and the file's own bookkeeping
/// tables can't be written through SQL.
fn open_untrusted(db: &Path) -> Result<Connection, String> {
    use rusqlite::config::DbConfig;
    let conn = open_read_write(db)?;
    let set = |config, on| conn.set_db_config(config, on).map(|_| ()).map_err(|e| format!("Couldn't open {} safely: {}", db.display(), e));
    set(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
    set(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)?;
    Ok(conn)
}

/// Leave the copy with Geniuz's own triggers and views and no others. Every trigger
/// and view it came with is dropped, unread and unrun, and the schema this version
/// creates is applied, which puts back the ones that keep memories permanent. They
/// hold no notes, so nothing a person kept is lost; a trigger left in place could
/// change the folder's ID on the next keep, to another folder's, and block both.
fn only_our_schema(conn: &Connection) -> Result<(), String> {
    let fail = |e: rusqlite::Error| format!("its triggers and views couldn't be replaced ({})", e);
    let mut found: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare("SELECT type, name FROM sqlite_master WHERE type IN ('trigger', 'view')")
            .map_err(fail)?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map_err(fail)?;
        rows.collect::<Result<_, _>>().map_err(fail)?
    };
    // Triggers first: dropping a view takes its own triggers with it.
    found.sort_by_key(|(kind, _)| kind != "trigger");
    let mut batch = String::from("BEGIN IMMEDIATE;\n");
    for (kind, name) in &found {
        let kind = if kind == "trigger" { "TRIGGER" } else { "VIEW" };
        batch.push_str(&format!("DROP {} IF EXISTS \"{}\";\n", kind, name.replace('"', "\"\"")));
    }
    batch.push_str("COMMIT;\n");
    conn.execute_batch(&batch).map_err(fail)?;
    conn.execute_batch(crate::db::SCHEMA).map_err(fail)
}

/// Have the file's bytes on the disk, not only on their way there.
fn sync(file: &Path) -> Result<(), String> {
    std::fs::OpenOptions::new()
        .write(true)
        .open(file)
        .and_then(|f| f.sync_all())
        .map_err(|e| format!("Couldn't finish writing {}: {}", file.display(), e))
}

/// The two tables that decide what a folder is and holds must have plain columns: none
/// computed from an expression of the file's choosing. (A table can also carry CHECK
/// and DEFAULT expressions. Geniuz registers no SQL functions of its own, so the worst
/// those can do is refuse a keep or misdate one, in that folder alone; they are left.)
fn plain_columns(conn: &Connection) -> Result<(), String> {
    for (table, needed) in [("memories", &["memory_uuid", "payload", "created_at", "parent_uuid"][..]), ("_meta", &["key", "value"][..])] {
        let mut stmt = conn
            .prepare(&format!("SELECT name, hidden FROM pragma_table_xinfo('{}')", table))
            .map_err(|e| format!("its {} table couldn't be read ({})", table, e))?;
        let columns: Vec<(String, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .and_then(|rows| rows.collect())
            .map_err(|e| format!("its {} table couldn't be read ({})", table, e))?;
        if columns.iter().any(|(_, hidden)| *hidden != 0) {
            return Err(format!("its {} table has a computed column, which Geniuz never writes", table));
        }
        if let Some(missing) = needed.iter().find(|n| !columns.iter().any(|(c, _)| c.eq_ignore_ascii_case(n))) {
            return Err(format!("its {} table has no {} column", table, missing));
        }
    }
    Ok(())
}

fn not_a_folder(file: &Path) -> String {
    format!(
        "{} isn't a Geniuz folder. Import takes a file written by `geniuz folder export` or the window's Export.",
        file.display()
    )
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text)
}

/// `work-backup.db` gives "work-backup"; a stem that can't be a name gives a plain one.
fn name_from_file(file: &Path) -> String {
    file.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .and_then(|s| validate_name(&s).ok())
        .unwrap_or_else(|| IMPORTED_NAME.to_string())
}

/// A directory only its owner can enter, whatever the process was started with.
fn make_private_dir(dir: &Path) -> Result<(), String> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir).map_err(|e| format!("Couldn't create {}: {}", dir.display(), e))
}

/// A copy takes its permissions from what it was copied from, which may have been
/// readable by everyone. Notes here are their owner's alone.
fn make_private_file(file: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("Couldn't make {} private: {}", file.display(), e))?;
    }
    // Elsewhere a copy of a read-only file is read-only too, and the copy must be written to.
    #[cfg(not(unix))]
    {
        let mut perms = std::fs::metadata(file)
            .map_err(|e| format!("Couldn't read {}: {}", file.display(), e))?
            .permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(file, perms).map_err(|e| format!("Couldn't prepare {}: {}", file.display(), e))?;
    }
    Ok(())
}

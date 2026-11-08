//! Folders: the named memory stores Geniuz can see.
//!
//! A folder is a directory holding one `memory.db`, whose identity lives in
//! that file's `_meta` table. Geniuz looks in exactly two kinds of place: its
//! data directory, and the directories a person added on purpose (the allow
//! list). It never searches the disk. Each of those is read one level down,
//! so moving a folder into `archive/` is all it takes to retire it.
//!
//! The list decides what Geniuz shows. File permissions decide who can read,
//! so an unreadable file is simply left out.
//!
//! A folder can be relocated but not copied. Its UUID and the UUID of every
//! memory in it are permanent, and addresses throughout other memories depend
//! on them, so nothing here may ever issue a new UUID to data that already has
//! one. Identity comes from `_meta`, not the directory name, so a copy under a
//! new directory name is still recognised as the same folder. `add` refuses a copy
//! outright. If one appears later, every copy is blocked until one is moved,
//! because nothing can tell which is the original, and guessing wrong would fork
//! the history. Nothing here ever deletes a file.
//!
//! `relocate` is the supported way to move a folder. Copying a live database by
//! hand can miss whatever is still in its `-wal` file, so relocating flushes
//! that file and holds off writers first.
//!
//! Reads use a read-only connection and never `DatabaseManager::new`, which
//! applies the schema on open: listing must not change a file it doesn't own.

use crate::db::MemoryEntry;
use crate::embedding::EmbeddingBackend;
use crate::window::Window;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const DB_FILE: &str = "memory.db";
const ADDED_FILE: &str = "added-folders";
const CURRENT_FILE: &str = "current";
/// What `current` holds when the person chose no default folder.
const NO_DEFAULT: &str = "none";
const PINNED_FILE: &str = "pinned-folders";
const ARCHIVE_DIR: &str = "archive";
const MOVED_MARK: &str = ".moved-";
const MOVING_PREFIX: &str = ".moving-";
const ABANDONED_MARK: &str = ".abandoned-";
const LOCK_FILE: &str = "registry.lock";
const FENCE_TRIGGER: &str = "geniuz_moved_away";
/// In a hidden copy: held locked by the process moving it, so a lock that can be
/// taken proves that process has stopped. The system releases it if the process dies.
const MOVE_OWNER: &str = "move-owner";
/// In a hidden copy: written once the original is fenced, as proof that the copy is
/// final. Without this or a fenced original, the copy may have fallen behind.
const MOVE_COMMITTED: &str = "move-committed";
const DOTFILE: &str = ".geniuz-folder";
const DEFAULT_NAME: &str = "My memory";
const MAX_NAME: usize = 60;
const SHORT_ID: usize = 8;

const KEY_UUID: &str = "folder_uuid";
const KEY_NAME: &str = "folder_name";
const KEY_DESCRIPTION: &str = "folder_description";
const KEY_CREATED: &str = "folder_created";

#[derive(Debug, Clone, PartialEq)]
pub struct Folder {
    pub uuid: String,
    pub name: String,
    pub description: String,
    pub created: String,
    pub db_path: PathBuf,
}

impl Folder {
    /// First eight characters, the form used in addresses like `13625F6B:3487B9B0`.
    pub fn short_id(&self) -> String {
        self.uuid.chars().take(SHORT_ID).collect::<String>().to_uppercase()
    }

    /// How many memories it holds and when the last was written.
    pub fn stats(&self) -> Result<(i64, Option<String>), String> {
        let conn = open_read_only(&self.db_path)?;
        conn.query_row("SELECT COUNT(*), MAX(created_at) FROM memories", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(|e| format!("Couldn't read {}: {}", self.db_path.display(), e))
    }
}

/// A folder with more than one copy. Every copy is blocked until one is moved.
#[derive(Debug, Clone, PartialEq)]
pub struct Blocked {
    pub uuid: String,
    pub name: String,
    pub db_path: PathBuf,
    /// Where another copy is.
    pub other: PathBuf,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct Scan {
    pub folders: Vec<Folder>,
    pub blocked: Vec<Blocked>,
    /// Added directories that can't be reached right now, such as an unplugged drive.
    pub unavailable: Vec<PathBuf>,
    /// Interrupted moves whose original can't be reached, so can't be settled yet.
    pub pending_moves: Vec<PathBuf>,
}

/// The result of a move. `left_behind` is set when it crossed drives.
#[derive(Debug)]
pub struct Relocated {
    pub folder: Folder,
    pub left_behind: Option<PathBuf>,
}

pub struct Registry {
    home: PathBuf,
}

impl Registry {
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self { home: home.into() }
    }

    /// The registry for this user's data directory.
    pub fn from_env() -> Self {
        Self::new(crate::data_dir())
    }

    fn added_path(&self) -> PathBuf {
        self.home.join(ADDED_FILE)
    }

    fn current_path(&self) -> PathBuf {
        self.home.join(CURRENT_FILE)
    }

    /// The database of the first folder.
    fn default_db(&self) -> PathBuf {
        self.home.join(DB_FILE)
    }

    /// Create the first folder if needed and give it an identity in place. The
    /// file never moves, and older versions can still open it.
    pub fn ensure_default(&self) -> Result<PathBuf, String> {
        let db = self.default_db();
        if !db.exists() {
            crate::db::DatabaseManager::new(&db.to_string_lossy())?;
        }
        stamp(&db, DEFAULT_NAME)?;
        Ok(db)
    }

    /// The directories Geniuz reads, data directory first.
    pub fn added(&self) -> Vec<PathBuf> {
        std::fs::read_to_string(self.added_path())
            .map(|s| {
                s.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(PathBuf::from)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Everything visible right now, resolving duplicates by list order.
    pub fn scan(&self) -> Scan {
        let mut out = Scan::default();
        if self.ensure_default().is_err() {
            out.unavailable.push(self.home.clone());
        }

        let mut roots = vec![self.home.clone()];
        for dir in self.added() {
            if dir.is_dir() {
                roots.push(dir);
            } else {
                out.unavailable.push(dir);
            }
        }

        let (found, paused) = read_all(&roots);
        let mut changed = false;
        for root in &roots {
            changed |= finish_interrupted_moves(root, &found, &paused, &mut out.pending_moves);
        }
        let found = if changed { read_all(&roots).0 } else { found };
        self.settle(&mut out, found);
        out
    }

    /// Sort what was found into usable folders and blocked copies.
    fn settle(&self, out: &mut Scan, found: Vec<Folder>) {
        let mut same_id: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for f in &found {
            same_id.entry(f.uuid.to_lowercase()).or_default().push(f.db_path.clone());
        }
        for folder in found {
            let paths = &same_id[&folder.uuid.to_lowercase()];
            if paths.len() > 1 {
                let other = paths.iter().find(|p| **p != folder.db_path).cloned().unwrap_or_default();
                let reason = if same_file(&folder.db_path, &other) {
                    // Each name would keep its own -wal file, which can corrupt the store.
                    "has two names for one file, which isn't safe"
                } else {
                    "has two copies, and nothing can tell which is the original"
                };
                out.blocked.push(Blocked {
                    reason: reason.to_string(),
                    uuid: folder.uuid,
                    name: folder.name,
                    db_path: folder.db_path,
                    other,
                });
            } else {
                out.folders.push(folder);
            }
        }
        // Pinned first, then by name, so every surface offers the same order.
        let pinned = self.pinned();
        out.folders.sort_by_key(|f| {
            (!pinned.iter().any(|p| p.eq_ignore_ascii_case(&f.uuid)), name_key(&f.name))
        });
    }

    /// The UUIDs of pinned folders. A pin outlives a rename or a move.
    pub fn pinned(&self) -> Vec<String> {
        std::fs::read_to_string(self.home.join(PINNED_FILE))
            .map(|s| s.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
            .unwrap_or_default()
    }

    /// Pin a folder to the top of every list, or stop.
    pub fn set_pinned(&self, query: &str, on: bool) -> Result<Folder, String> {
        self.with_registry_lock(|| {
            let folder = self.find(query)?;
            let mut all = self.pinned();
            all.retain(|p| !p.eq_ignore_ascii_case(&folder.uuid));
            if on {
                all.push(folder.uuid.clone());
            }
            let body: String = all.iter().map(|p| format!("{}\n", p)).collect();
            write_atomic(&self.home.join(PINNED_FILE), &body)?;
            Ok(folder)
        })
    }

    /// Find a folder by name (ignoring case), UUID, or the first eight
    /// characters of its UUID. A name or UUID that matches nothing is an error:
    /// a typo must never send a memory somewhere else.
    pub fn find(&self, query: &str) -> Result<Folder, String> {
        let scan = self.scan();
        let q = query.trim().to_lowercase();
        let id_hit = |uuid: &str| {
            let u = uuid.to_lowercase();
            u == q || (q.len() == SHORT_ID && u.starts_with(&q))
        };
        // IDs first: a persisted UUID must always mean that folder, whatever
        // anything else is named.
        if let Some(b) = scan.blocked.iter().find(|b| id_hit(&b.uuid) || name_key(&b.name) == name_key(query)) {
            return Err(format!(
                "\"{}\" {}: {} and {}. Move one of them out of Geniuz's reach first.",
                b.name,
                b.reason,
                b.db_path.display(),
                b.other.display()
            ));
        }
        if let Some(f) = scan.folders.iter().find(|f| f.uuid.to_lowercase() == q) {
            return Ok(f.clone());
        }
        let named: Vec<_> = scan.folders.iter().filter(|f| name_key(&f.name) == name_key(query)).collect();
        match named.as_slice() {
            [one] => return Ok((*one).clone()),
            [] => {}
            _ => return Err(format!(
                "More than one folder is called \"{}\". Use an ID from `geniuz folder list`, and rename one of them.",
                query
            )),
        }
        if q.len() == SHORT_ID {
            let hits: Vec<_> = scan.folders.iter().filter(|f| f.uuid.to_lowercase().starts_with(&q)).collect();
            match hits.as_slice() {
                [one] => return Ok((*one).clone()),
                [] => {}
                _ => return Err(format!("\"{}\" matches more than one folder. Use its name.", query)),
            }
        }
        Err(format!(
            "No folder called \"{}\". Run `geniuz folder list` to see them.",
            query
        ))
    }

    /// Which database a command should use. In order: the `--in` flag, the
    /// `GENIUZ_DB` file path, a `.geniuz-folder` file here or in a parent
    /// directory, the default folder, then the first folder. If the default was
    /// set to none, there is no last step: it's an error asking for a folder.
    pub fn resolve(
        &self,
        flag: Option<&str>,
        env_station: Option<&str>,
        cwd: Option<&Path>,
    ) -> Result<PathBuf, String> {
        if let Some(q) = flag {
            return Ok(self.find(q)?.db_path);
        }
        if let Some(path) = env_station.filter(|s| !s.is_empty()) {
            return Ok(PathBuf::from(path));
        }
        if let Some(dir) = cwd {
            if let Some((file, q)) = read_dotfile(dir) {
                return self
                    .find(&q)
                    .map(|f| f.db_path)
                    .map_err(|e| format!("{} (from {})", e, file.display()));
            }
        }
        if self.no_default() {
            return Err(NO_DEFAULT_MESSAGE.to_string());
        }
        if let Some(q) = self.current() {
            return self
                .find(&q)
                .map(|f| f.db_path)
                .map_err(|e| {
                    if e.contains("Move one of them") {
                        e
                    } else {
                        format!("The current folder ({}) isn't available. Choose one with `geniuz folder use <folder>`.", q)
                    }
                });
        }
        let first = self.ensure_default()?;
        let scan = self.scan();
        if let Some(b) = scan.blocked.iter().find(|b| canonical(&b.db_path) == canonical(&first)) {
            return Err(format!(
                "Your first folder {}: {}. Move it out of Geniuz's reach first.",
                b.reason,
                b.other.display()
            ));
        }
        Ok(first)
    }

    /// The folder chosen with `use`, if any.
    pub fn current(&self) -> Option<String> {
        std::fs::read_to_string(self.current_path())
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case(NO_DEFAULT))
    }

    /// Whether the default was set to none: a command that names no folder must
    /// ask for one rather than guess: on a machine many
    /// agents share, a memory in the wrong folder can't be moved afterwards).
    pub fn no_default(&self) -> bool {
        std::fs::read_to_string(self.current_path())
            .map(|s| s.trim().eq_ignore_ascii_case(NO_DEFAULT))
            .unwrap_or(false)
    }

    /// Set the default to none (see `no_default`).
    pub fn clear_default(&self) -> Result<(), String> {
        self.with_registry_lock(|| write_atomic(&self.current_path(), &format!("{}\n", NO_DEFAULT)))
    }

    /// Make a folder the one used when none is named. Records its UUID, so
    /// renaming it changes nothing.
    pub fn use_folder(&self, query: &str) -> Result<Folder, String> {
        self.with_registry_lock(|| self.use_folder_unlocked(query))
    }

    fn use_folder_unlocked(&self, query: &str) -> Result<Folder, String> {
        let folder = self.find(query)?;
        write_atomic(&self.current_path(), &format!("{}\n", folder.uuid))?;
        Ok(folder)
    }

    /// Create a folder, in the data directory unless `at` names another place.
    /// A new place goes on the allow list so the folder can be seen.
    pub fn create(&self, name: &str, description: &str, at: Option<&Path>) -> Result<Folder, String> {
        self.with_registry_lock(|| self.create_unlocked(name, description, at))
    }

    fn create_unlocked(&self, name: &str, description: &str, at: Option<&Path>) -> Result<Folder, String> {
        let name = validate_name(name)?;
        self.check_name_free(&name, None)?;

        let parent = at.unwrap_or(&self.home);
        let uuid = uuid::Uuid::new_v4().to_string().to_uppercase();
        let db = parent.join(&uuid).join(DB_FILE);
        crate::db::DatabaseManager::new(&db.to_string_lossy())?;

        let conn = open_read_write(&db)?;
        set_meta(&conn, KEY_UUID, &uuid)?;
        set_meta(&conn, KEY_NAME, &name)?;
        set_meta(&conn, KEY_CREATED, &now())?;
        if !description.trim().is_empty() {
            set_meta(&conn, KEY_DESCRIPTION, description.trim())?;
        }
        drop(conn);

        if at.is_some() {
            self.remember_added_unlocked(parent)?;
        }
        read_identity(&db).ok_or_else(|| format!("Couldn't read {} after creating it", db.display()))
    }

    /// Put a directory on the allow list. All or nothing: if anything in it
    /// would duplicate a folder already here, nothing is added or changed.
    ///
    /// A `memory.db` written by an older Geniuz gets an identity, named after
    /// its directory, unless it shares memories with a folder already here:
    /// that makes it a copy, and a new folder UUID would give the same memories
    /// two identities. A database from anything else has no identity and is
    /// left untouched, so adding a directory of them finds nothing.
    pub fn add(&self, dir: &Path) -> Result<Vec<Folder>, String> {
        self.with_registry_lock(|| self.add_unlocked(dir))
    }

    fn add_unlocked(&self, dir: &Path) -> Result<Vec<Folder>, String> {
        let dir = std::fs::canonicalize(dir)
            .map(plain_path)
            .map_err(|e| format!("Couldn't open {}: {}", dir.display(), e))?;
        if dir == canonical(&self.home) || self.added().iter().any(|d| canonical(d) == dir) {
            return Err(format!("{} is already added.", dir.display()));
        }

        let existing = self.scan();
        let mut uuids: HashMap<String, PathBuf> = existing
            .folders
            .iter()
            .map(|f| (f.uuid.to_lowercase(), f.db_path.clone()))
            .collect();
        uuids.extend(existing.blocked.iter().map(|b| (b.uuid.to_lowercase(), b.db_path.clone())));
        let mut names: HashMap<String, PathBuf> = existing
            .folders
            .iter()
            .map(|f| (name_key(&f.name), f.db_path.clone()))
            .collect();
        // Every memory ID already here. Copies can differ in identity and in their
        // oldest memories, so only comparing all of them can establish this.
        let mut memories: HashMap<String, PathBuf> = HashMap::new();
        for f in &existing.folders {
            for id in memory_ids(&f.db_path) {
                memories.insert(id, f.db_path.clone());
            }
        }

        // Check everything before writing anything.
        let mut to_stamp = Vec::new();
        let mut found = 0;
        for db in candidates(&dir) {
            if canonical(&db).starts_with(canonical(&self.home)) {
                continue;
            }
            let (uuid, name) = match read_identity(&db) {
                Some(f) => (Some(f.uuid), f.name),
                None if is_geniuz_store(&db) => (None, directory_name(&db)),
                None => continue,
            };
            for id in memory_ids(&db) {
                if let Some(other) = memories.insert(id, db.clone()) {
                    if other != db {
                        return Err(format!(
                            "Not added. {} holds memories that are also in {}. Copies aren't supported; move or retire one of them first.",
                            db.display(),
                            other.display()
                        ));
                    }
                }
            }
            if uuid.is_none() {
                to_stamp.push((db.clone(), name.clone()));
            }
            found += 1;
            if let Some(uuid) = uuid {
                if let Some(other) = uuids.insert(uuid.to_lowercase(), db.clone()) {
                    return Err(format!(
                        "Not added. {} is a copy of the folder at {}. Copies aren't supported; move or retire one of them first.",
                        db.display(),
                        other.display()
                    ));
                }
            }
            if let Some(other) = names.insert(name_key(&name), db.clone()) {
                return Err(format!(
                    "Not added. {} uses the name \"{}\", already used by {}. Rename one of them first.",
                    db.display(),
                    name,
                    other.display()
                ));
            }
        }
        if found == 0 {
            return Err(format!("No Geniuz folders in {}.", dir.display()));
        }

        self.remember_added_unlocked(&dir)?;
        for (db, name) in &to_stamp {
            stamp(db, name)?;
        }

        let scan = self.scan();
        Ok(scan
            .folders
            .into_iter()
            .filter(|f| canonical(&f.db_path).starts_with(&dir))
            .collect())
    }

    /// Take a directory off the allow list. Its files aren't touched.
    pub fn remove(&self, dir: &Path) -> Result<(), String> {
        self.with_registry_lock(|| self.remove_unlocked(dir))
    }

    fn remove_unlocked(&self, dir: &Path) -> Result<(), String> {
        let target = canonical(dir);
        let current = self.added();
        let kept: Vec<_> = current.iter().filter(|d| canonical(d) != target).cloned().collect();
        if kept.len() == current.len() {
            return Err(format!("{} isn't on the list.", dir.display()));
        }
        self.write_added(&kept)
    }

    pub fn rename(&self, query: &str, new_name: &str) -> Result<Folder, String> {
        self.with_registry_lock(|| self.rename_unlocked(query, new_name))
    }

    fn rename_unlocked(&self, query: &str, new_name: &str) -> Result<Folder, String> {
        let folder = self.find(query)?;
        let name = validate_name(new_name)?;
        self.check_name_free(&name, Some(&folder.uuid))?;
        replace_meta(&folder.db_path, KEY_NAME, &name)?;
        self.find(&folder.uuid)
    }

    pub fn describe(&self, query: &str, description: &str) -> Result<Folder, String> {
        let folder = self.find(query)?;
        replace_meta(&folder.db_path, KEY_DESCRIPTION, description.trim())?;
        self.find(&folder.uuid)
    }

    /// Retire a folder by moving it into `archive/` beside itself, one level
    /// below where Geniuz reads. Nothing is deleted, and `restore` brings it back.
    pub fn archive(&self, query: &str) -> Result<PathBuf, String> {
        self.with_registry_lock(|| self.archive_unlocked(query))
    }

    fn archive_unlocked(&self, query: &str) -> Result<PathBuf, String> {
        let folder = self.find(query)?;
        let dir = folder
            .db_path
            .parent()
            .ok_or_else(|| "That folder has no parent directory.".to_string())?
            .to_path_buf();
        if canonical(&dir) == canonical(&self.home) {
            return Err("Your first folder can't be archived, because other folders live beside it.".into());
        }
        refuse_symlink(&dir)?;
        // The default folder is a setting someone chose; archiving it would quietly
        // send memories somewhere else. Ask for the setting to change first.
        if self.current().map(|c| c.eq_ignore_ascii_case(&folder.uuid)).unwrap_or(false) {
            return Err(format!(
                "\"{}\" is the default folder. Choose another default first (Settings, or `geniuz folder use <folder>`), then archive it.",
                folder.name
            ));
        }
        if self.added().iter().any(|d| canonical(d) == canonical(&dir)) {
            return Err(format!(
                "{} was added directly. Take it off the list with `geniuz folder remove`.",
                dir.display()
            ));
        }
        let parent = dir
            .parent()
            .ok_or_else(|| "That folder has no parent directory.".to_string())?;
        let archive = parent.join(ARCHIVE_DIR);
        std::fs::create_dir_all(&archive)
            .map_err(|e| format!("Couldn't create {}: {}", archive.display(), e))?;
        let target = archive.join(dir.file_name().unwrap_or_default());
        if target.exists() {
            return Err(format!("{} already exists.", target.display()));
        }
        std::fs::rename(&dir, &target)
            .map_err(|e| format!("Couldn't move {}: {}", dir.display(), e))?;
        Ok(target)
    }

    /// Folders that were archived and can be restored, in every place Geniuz
    /// reads. A `.moved-` directory is the original of a folder that moved to
    /// another drive, not an archived folder: it duplicates one in use.
    pub fn archived(&self) -> Vec<Folder> {
        let mut roots = vec![self.home.clone()];
        roots.extend(self.added());
        let mut out = Vec::new();
        for root in roots {
            let Ok(entries) = std::fs::read_dir(root.join(ARCHIVE_DIR)) else { continue };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.contains(MOVED_MARK) || name.contains(ABANDONED_MARK) {
                    continue;
                }
                let db = entry.path().join(DB_FILE);
                if let Some(folder) = read_identity(&db) {
                    out.push(folder);
                }
            }
        }
        out.sort_by_key(|f| f.name.to_lowercase());
        out
    }

    /// Bring an archived folder back to where it was. Refused if that would
    /// duplicate a folder in use, by UUID or by name.
    pub fn restore(&self, query: &str) -> Result<Folder, String> {
        self.with_registry_lock(|| self.restore_unlocked(query))
    }

    fn restore_unlocked(&self, query: &str) -> Result<Folder, String> {
        let q = query.trim().to_lowercase();
        let archived = self.archived();
        let folder = archived
            .iter()
            .find(|f| {
                name_key(&f.name) == name_key(query)
                    || f.uuid.to_lowercase() == q
                    || (q.len() == SHORT_ID && f.uuid.to_lowercase().starts_with(&q))
            })
            .ok_or_else(|| format!("No archived folder called \"{}\".", query))?;

        let scan = self.scan();
        let same_id = |u: &str| u.eq_ignore_ascii_case(&folder.uuid);
        if let Some(other) = scan.blocked.iter().find(|b| same_id(&b.uuid)) {
            return Err(format!("Not restored. The same folder is in use at {}.", other.db_path.display()));
        }
        if let Some(other) = scan.folders.iter().find(|f| same_id(&f.uuid)) {
            return Err(format!(
                "Not restored. The same folder is in use at {}.",
                other.db_path.display()
            ));
        }
        if scan.folders.iter().any(|f| name_key(&f.name) == name_key(&folder.name)) {
            return Err(format!(
                "Not restored. There's already a folder called \"{}\". Rename that one first.",
                folder.name
            ));
        }

        let dir = folder.db_path.parent().ok_or("That folder has no parent directory.")?;
        let root = dir
            .parent()
            .and_then(Path::parent)
            .ok_or("That folder has no parent directory.")?;
        let target = root.join(dir.file_name().unwrap_or_default());
        if target.exists() {
            return Err(format!("{} already exists.", target.display()));
        }
        std::fs::rename(dir, &target).map_err(|e| format!("Couldn't move {}: {}", dir.display(), e))?;
        self.find(&folder.uuid)
    }

    /// Move a folder to another directory, keeping every UUID. Writers are held
    /// off for the whole move, and on another drive the new copy is verified
    /// before the original is retired. Across drives the original is moved to
    /// `archive/` beside itself and is returned, for the person to delete once
    /// they are satisfied; Geniuz never deletes it.
    pub fn relocate(&self, query: &str, dest: &Path) -> Result<Relocated, String> {
        self.with_registry_lock(|| self.relocate_unlocked(query, dest))
    }

    fn relocate_unlocked(&self, query: &str, dest: &Path) -> Result<Relocated, String> {
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

    fn check_name_free(&self, name: &str, except_uuid: Option<&str>) -> Result<(), String> {
        let lower = name_key(name);
        let taken = self.scan().folders.iter().any(|f| {
            name_key(&f.name) == lower
                && except_uuid.map(|u| !f.uuid.eq_ignore_ascii_case(u)).unwrap_or(true)
        });
        if taken {
            return Err(format!("There's already a folder called \"{}\".", name));
        }
        Ok(())
    }

    #[cfg(test)]
    fn remember_added(&self, dir: &Path) -> Result<(), String> {
        self.with_registry_lock(|| self.remember_added_unlocked(dir))
    }

    fn remember_added_unlocked(&self, dir: &Path) -> Result<(), String> {
        let dir = canonical(dir);
        if dir == canonical(&self.home) {
            return Ok(());
        }
        let mut all = self.added();
        if all.iter().any(|d| canonical(d) == dir) {
            return Ok(());
        }
        all.push(dir);
        self.write_added(&all)
    }

    /// Every change to folders happens under this lock, across processes: the
    /// list, the current folder, creating, renaming, archiving, moving. A check
    /// and the change it allows must not have another change between them. Never
    /// nest it: a process waits on its own lock. Reading needs no lock.
    fn with_registry_lock<T>(&self, change: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        std::fs::create_dir_all(&self.home)
            .map_err(|e| format!("Couldn't create {}: {}", self.home.display(), e))?;
        let path = self.home.join(LOCK_FILE);
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| format!("Couldn't open {}: {}", path.display(), e))?;
        lock.lock().map_err(|e| format!("Couldn't lock {}: {}", path.display(), e))?;
        change()
    }

    fn write_added(&self, dirs: &[PathBuf]) -> Result<(), String> {
        let body: String = dirs.iter().map(|d| format!("{}\n", d.display())).collect();
        write_atomic(&self.added_path(), &body)
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

/// Split an address like `13625F6B:3487B9B0` into folder and memory.
pub fn parse_address(text: &str) -> Option<(String, String)> {
    let (folder, memory) = text.trim().split_once(':')?;
    let hex8 = |s: &str| s.len() == SHORT_ID && s.chars().all(|c| c.is_ascii_hexdigit());
    (hex8(folder) && hex8(memory)).then(|| (folder.to_uppercase(), memory.to_uppercase()))
}

/// What to look for across every folder.
pub enum Search<'a> {
    Recent,
    Keyword(&'a str),
    Semantic(&'a str, Option<&'a dyn EmbeddingBackend>),
}

/// One result, with the folder it came from.
pub struct Hit {
    pub folder: Folder,
    pub entry: MemoryEntry,
}

impl Hit {
    /// The address that opens this memory directly.
    pub fn address(&self) -> String {
        let memory: String = self.entry.memory_uuid.chars().take(SHORT_ID).collect();
        format!("{}:{}", self.folder.short_id(), memory.to_uppercase())
    }
}

/// The result of searching every folder.
pub struct Across {
    pub hits: Vec<Hit>,
    pub searched: usize,
    pub memories: usize,
    /// Folders that couldn't be read, reported rather than quietly left out.
    pub skipped: Vec<String>,
}

/// Search every visible folder and merge: best score first, then newest.
pub fn search_across(reg: &Registry, search: &Search, window: &Window, limit: usize) -> Across {
    let mut out = Across { hits: Vec::new(), searched: 0, memories: 0, skipped: Vec::new() };
    for folder in reg.scan().folders {
        let found = crate::db::DatabaseManager::open_existing(&folder.db_path.to_string_lossy()).and_then(|db| {
            let entries = match search {
                Search::Recent => db.recent_in(window, limit),
                Search::Keyword(q) => db.keyword_search_in(q, window, limit),
                // As in one folder: failing to search by meaning falls back to words.
                Search::Semantic(q, backend) => db
                    .semantic_search_with(q, window, limit, *backend)
                    .or_else(|_| db.keyword_search_in(q, window, limit)),
            }?;
            Ok((entries, db.count_in(window)?))
        });
        match found {
            Ok((entries, count)) => {
                out.searched += 1;
                out.memories += count;
                out.hits.extend(entries.into_iter().map(|entry| Hit { folder: folder.clone(), entry }));
            }
            Err(_) => out.skipped.push(folder.name),
        }
    }
    out.hits.sort_by(|a, b| {
        b.entry
            .score
            .unwrap_or(f32::MIN)
            .total_cmp(&a.entry.score.unwrap_or(f32::MIN))
            .then_with(|| b.entry.created_at.cmp(&a.entry.created_at))
    });
    out.hits.truncate(limit);
    out
}

/// How names are compared, everywhere: ignoring case, by Unicode rules.
fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// A name is short, one line, and can't be mistaken for an address or a path.
pub fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("A folder needs a name.".into());
    }
    if name.chars().count() > MAX_NAME {
        return Err(format!("Keep the name to {} characters. Longer notes belong in its description.", MAX_NAME));
    }
    if name.contains(['\n', '\r', '/', '\\', ':']) {
        return Err("A folder name can't contain line breaks, slashes or colons.".into());
    }
    if name.len() == SHORT_ID && name.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("\"{}\" looks like a folder ID. Choose a word.", name));
    }
    if uuid::Uuid::parse_str(name).is_ok() {
        return Err(format!("\"{}\" looks like a folder ID. Choose a word.", name));
    }
    Ok(name.to_string())
}

/// The databases directly in `root` and one level below it.
fn candidates(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let db = root.join(DB_FILE);
    if db.is_file() {
        out.push(db);
    }
    let Ok(entries) = std::fs::read_dir(root) else { return out };
    let mut dirs: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .map(|n| n != ARCHIVE_DIR && !n.to_string_lossy().starts_with('.'))
                    .unwrap_or(false)
        })
        .collect();
    dirs.sort();
    for dir in dirs {
        let db = dir.join(DB_FILE);
        if db.is_file() {
            out.push(db);
        }
    }
    out
}

fn open_read_only(db: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|e| format!("Couldn't open {}: {}", db.display(), e))
}

fn open_read_write(db: &Path) -> Result<Connection, String> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|e| format!("Couldn't open {}: {}", db.display(), e))?;
    conn.busy_timeout(std::time::Duration::from_secs(30))
        .map_err(|e| format!("Couldn't open {}: {}", db.display(), e))?;
    Ok(conn)
}

fn get_meta(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM _meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .ok()
        .flatten()
}

fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute("INSERT OR REPLACE INTO _meta (key, value) VALUES (?1, ?2)", [key, value])
        .map(|_| ())
        .map_err(|e| format!("Couldn't save {}: {}", key, e))
}

/// Change a value and keep the old one beside it, dated, so a description's
/// history survives every edit.
fn replace_meta(db: &Path, key: &str, value: &str) -> Result<(), String> {
    let mut conn = open_read_write(db)?;
    let tx = conn.transaction().map_err(|e| format!("Couldn't save {}: {}", key, e))?;
    if let Some(old) = get_meta(&tx, key) {
        // The earlier wording is kept under key@time. Two changes in the same second
        // would share that key, so a later one is numbered (#2, #3) rather than lost.
        let stamp = format!("{}@{}", key, now());
        let mut n = 1;
        loop {
            let name = if n == 1 { stamp.clone() } else { format!("{}#{}", stamp, n) };
            let added = tx
                .execute("INSERT OR IGNORE INTO _meta (key, value) VALUES (?1, ?2)", [name, old.clone()])
                .map_err(|e| format!("Couldn't save {}: {}", key, e))?;
            if added == 1 {
                break;
            }
            n += 1;
        }
    }
    set_meta(&tx, key, value)?;
    tx.commit().map_err(|e| format!("Couldn't save {}: {}", key, e))
}

/// Everything in these places: folders in use, and those fenced mid-move.
fn read_all(roots: &[PathBuf]) -> (Vec<Folder>, Vec<Folder>) {
    let mut seen = HashSet::new();
    let (mut found, mut paused) = (Vec::new(), Vec::new());
    for root in roots {
        for db in candidates(root) {
            if !seen.insert(canonical(&db)) {
                continue;
            }
            let Ok(conn) = open_read_only(&db) else { continue };
            let Some(folder) = identity_from(&conn, &db) else { continue };
            if fenced(&conn) {
                paused.push(folder);
            } else {
                found.push(folder);
            }
        }
    }
    (found, paused)
}

/// Whether two paths are hard links to one file.
#[cfg(unix)]
fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn same_file(_: &Path, _: &Path) -> bool {
    false
}

/// Read a file's identity. `None` means it isn't a folder, or can't be read.
/// What a command that names no folder is told when the default is none. Shared, so
/// callers can tell this refusal from other reasons a folder doesn't resolve.
pub const NO_DEFAULT_MESSAGE: &str = "No default folder is set, so name a folder: the folder argument, or -f <folder> \
     on the command line. A default can be chosen in Settings, or with `geniuz folder use <folder>`.";

/// The folder whose database is at `db`, read without changing anything on disk.
pub fn identity_at(db: &Path) -> Option<Folder> {
    read_identity(db)
}

fn read_identity(db: &Path) -> Option<Folder> {
    identity_from(&open_read_only(db).ok()?, db)
}

fn identity_from(conn: &Connection, db: &Path) -> Option<Folder> {
    let uuid = get_meta(conn, KEY_UUID)?;
    Some(Folder {
        name: get_meta(conn, KEY_NAME).unwrap_or_else(|| uuid.chars().take(SHORT_ID).collect()),
        description: get_meta(conn, KEY_DESCRIPTION).unwrap_or_default(),
        created: get_meta(conn, KEY_CREATED).unwrap_or_default(),
        uuid,
        db_path: db.to_path_buf(),
    })
}

/// A store written by Geniuz that has no folder identity yet.
fn is_geniuz_store(db: &Path) -> bool {
    open_read_only(db)
        .ok()
        .and_then(|c| get_meta(&c, "product"))
        .map(|p| crate::db::OWN_PRODUCTS.contains(&p.as_str()))
        .unwrap_or(false)
}

/// The table holding a store's memories and its UUID column. Files from before
/// Geniuz 4.0 used other names; they are brought forward by a script, not read here.
fn memory_table(conn: &Connection) -> Result<(&'static str, &'static str), String> {
    if conn.prepare("SELECT memory_uuid FROM memories LIMIT 1").is_ok() {
        return Ok(("memories", "memory_uuid"));
    }
    Err("It has no memory table from Geniuz 4.0 or later.".to_string())
}

/// Every memory ID in a store, normalised for comparing.
fn memory_ids(db: &Path) -> Vec<String> {
    let read = || -> Option<Vec<String>> {
        let conn = open_read_only(db).ok()?;
        let (table, column) = memory_table(&conn).ok()?;
        let mut stmt = conn.prepare(&format!("SELECT {} FROM {}", column, table)).ok()?;
        let ids = stmt.query_map([], |r| r.get::<_, String>(0)).ok()?.flatten();
        Some(ids.map(|id| id.to_lowercase()).collect())
    };
    read().unwrap_or_default()
}

/// The UUID for a store getting its first identity. It comes from the store's
/// oldest memory, so if a 4.x file was copied to another machine and both copies
/// upgrade, they get the same UUID rather than giving one set of memories two
/// identities. Only an empty store, with nothing to duplicate, gets a random one.
///
/// This is best effort across machines: importing a memory dated earlier into one
/// copy before upgrading changes which is oldest. On one machine, `add` compares
/// the memories themselves.
fn identity_for(conn: &Connection) -> Result<String, String> {
    let (table, column) = memory_table(conn)?;
    let sql = format!("SELECT {c} FROM {t} ORDER BY created_at ASC, {c} ASC LIMIT 1", c = column, t = table);
    let oldest: Option<String> = conn
        .query_row(&sql, [], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Couldn't read its memories: {}", e))?;
    let id = match oldest {
        Some(memory) => {
            let name = format!("geniuz.folder:{}", memory.to_lowercase());
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, name.as_bytes())
        }
        None => uuid::Uuid::new_v4(),
    };
    Ok(id.to_string().to_uppercase())
}

/// Flush the `-wal` file into the database and hold off writers. The returned
/// connection keeps them off until it is dropped.
fn quiesce(db: &Path) -> Result<Connection, String> {
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
fn fenced(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'trigger' AND name = ?1",
        [format!("{}_memories_insert", FENCE_TRIGGER)],
        |_| Ok(()),
    )
    .is_ok()
}

/// Triggers that refuse every change to a store's contents, in the file itself.
fn fence_sql() -> String {
    let mut sql = String::new();
    for table in ["memories", "memory_embeddings", "embedding_meta", "_meta"] {
        for op in ["insert", "update", "delete"] {
            sql.push_str(&format!(
                "CREATE TRIGGER {f}_{t}_{o} BEFORE {o} ON {t} BEGIN SELECT RAISE(ABORT, 'This folder has moved to another drive'); END;\n",
                f = FENCE_TRIGGER,
                t = table,
                o = op
            ));
        }
    }
    sql
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
fn copy_then_retire(dir: &Path, dest: &Path, folder: &Folder, lock: Connection) -> Result<PathBuf, String> {
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
        std::fs::File::open(&copied)
            .and_then(|f| f.sync_all())
            .map_err(|e| format!("Couldn't save {}: {}", copied.display(), e))?;
    }

    verify_copy(&staging.join(DB_FILE), folder, expected)
        .map_err(|e| format!("{} The original is untouched; the partial copy is at {}.", e, staging.display()))?;

    lock.execute_batch(&format!("{}COMMIT;", fence_sql()))
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
fn publish(staging: &Path, target: &Path) -> Result<(), String> {
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
fn finish_interrupted_moves(root: &Path, live: &[Folder], paused: &[Folder], pending: &mut Vec<PathBuf>) -> bool {
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

fn refuse_symlink(dir: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(dir) {
        Ok(m) if m.file_type().is_symlink() => Err(format!(
            "{} is a link to a folder elsewhere. Move the folder itself, or remove the link.",
            dir.display()
        )),
        _ => Ok(()),
    }
}

fn verify_copy(db: &Path, folder: &Folder, expected: i64) -> Result<(), String> {
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

/// Give a store an identity if it has none. Safe to repeat, and safe beside a
/// running older version, which ignores these rows.
fn stamp(db: &Path, name: &str) -> Result<(), String> {
    if read_identity(db).is_some() {
        return Ok(());
    }
    let conn = open_read_write(db)?;
    let uuid = identity_for(&conn).map_err(|e| format!("Couldn't identify {}: {}", db.display(), e))?;
    for (key, value) in [(KEY_UUID, uuid.as_str()), (KEY_NAME, name), (KEY_CREATED, now().as_str())] {
        conn.execute("INSERT OR IGNORE INTO _meta (key, value) VALUES (?1, ?2)", [key, value])
            .map_err(|e| format!("Couldn't update {}: {}", db.display(), e))?;
    }
    Ok(())
}

fn read_dotfile(start: &Path) -> Option<(PathBuf, String)> {
    for dir in start.ancestors() {
        let file = dir.join(DOTFILE);
        if let Ok(body) = std::fs::read_to_string(&file) {
            let q = body.trim().to_string();
            if !q.is_empty() {
                return Some((file, q));
            }
        }
    }
    None
}

fn directory_name(db: &Path) -> String {
    db.parent()
        .and_then(|d| d.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .and_then(|n| validate_name(&n).ok())
        .unwrap_or_else(|| "Added folder".to_string())
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).map(plain_path).unwrap_or_else(|_| path.to_path_buf())
}

/// Windows answers canonicalize with `\\\\?\\C:\\...`, which nobody should have to read.
/// A network path keeps its prefix: it isn't the same path without it.
fn plain_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with(r"UNC\") => PathBuf::from(rest),
        _ => path,
    }
}

fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn write_atomic(path: &Path, body: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Couldn't create {}: {}", parent.display(), e))?;
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp.{}.{}", std::process::id(), n));
    std::fs::write(&tmp, body).map_err(|e| format!("Couldn't write {}: {}", tmp.display(), e))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Couldn't write {}: {}", path.display(), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Registry) {
        let tmp = TempDir::new().unwrap();
        let reg = Registry::new(tmp.path().join("home"));
        std::fs::create_dir_all(tmp.path().join("home")).unwrap();
        (tmp, reg)
    }

    fn copy_dir(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }

    fn names(reg: &Registry) -> Vec<String> {
        reg.scan().folders.into_iter().map(|f| f.name).collect()
    }

    #[test]
    fn no_default_asks_instead_of_guessing() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = Registry::new(tmp.path());
        reg.create("Work", "", None).unwrap();
        reg.use_folder("Work").unwrap();
        assert!(reg.resolve(None, None, None).unwrap().to_string_lossy().contains(&reg.find("Work").unwrap().uuid));
        reg.clear_default().unwrap();
        assert!(reg.no_default());
        assert_eq!(reg.current(), None);
        let err = reg.resolve(None, None, None).unwrap_err();
        assert!(err.contains("No default folder"), "{err}");
        // Naming a folder still works, and choosing a default again ends it.
        assert!(reg.resolve(Some("Work"), None, None).is_ok());
        reg.use_folder("Work").unwrap();
        assert!(!reg.no_default());
        assert!(reg.resolve(None, None, None).is_ok());
    }

    #[test]
    fn first_folder_appears_with_an_identity_and_no_memories() {
        let (_tmp, reg) = setup();
        let scan = reg.scan();
        assert_eq!(scan.folders.len(), 1);
        assert_eq!(scan.folders[0].name, DEFAULT_NAME);
        assert_eq!(scan.folders[0].stats().unwrap().0, 0);
    }

    #[test]
    fn existing_memory_db_keeps_its_place_and_its_identity() {
        let (_tmp, reg) = setup();
        let db = reg.ensure_default().unwrap();
        let first = read_identity(&db).unwrap();
        reg.ensure_default().unwrap();
        assert_eq!(read_identity(&db).unwrap().uuid, first.uuid);
        assert_eq!(db.file_name().unwrap(), DB_FILE);
    }

    #[test]
    fn a_file_from_before_4_0_is_refused_and_left_unchanged() {
        let tmp = TempDir::new().unwrap();
        for schema in [
            "CREATE TABLE signals (signal_uuid TEXT, payload TEXT, created_at TEXT);",
            "CREATE TABLE memories (signal_uuid TEXT, payload TEXT, created_at TEXT);",
        ] {
            let db = tmp.path().join("old.db");
            let _ = std::fs::remove_file(&db);
            Connection::open(&db).unwrap().execute_batch(schema).unwrap();
            let before = std::fs::read(&db).unwrap();
            let err = crate::db::DatabaseManager::new(&db.to_string_lossy()).err().unwrap();
            assert!(err.contains("before 4.0") && err.contains(crate::db::MIGRATION_SCRIPT), "{err}");
            assert_eq!(std::fs::read(&db).unwrap(), before, "not touched");
        }
    }

    #[test]
    fn plain_path_drops_only_the_windows_drive_prefix() {
        assert_eq!(plain_path(PathBuf::from(r"\\?\C:\build\x")), PathBuf::from(r"C:\build\x"));
        assert_eq!(plain_path(PathBuf::from(r"\\?\UNC\host\share")), PathBuf::from(r"\\?\UNC\host\share"));
        assert_eq!(plain_path(PathBuf::from("/home/x")), PathBuf::from("/home/x"));
    }

    #[test]
    fn create_goes_in_the_data_directory_by_default() {
        let (_tmp, reg) = setup();
        let f = reg.create("Work", "Client projects", None).unwrap();
        assert_eq!(f.db_path, reg.home.join(&f.uuid).join(DB_FILE));
        assert_eq!(f.description, "Client projects");
        assert!(reg.added().is_empty());
        assert_eq!(names(&reg), vec![DEFAULT_NAME, "Work"]);
    }

    #[test]
    fn create_elsewhere_goes_on_the_list() {
        let (tmp, reg) = setup();
        let at = tmp.path().join("notes");
        let f = reg.create("Notes", "", Some(&at)).unwrap();
        assert!(f.db_path.starts_with(&at));
        assert_eq!(reg.added(), vec![canonical(&at)]);
        assert_eq!(reg.find("notes").unwrap().uuid, f.uuid);
    }

    #[test]
    fn list_sorts_ignoring_case() {
        let (_tmp, reg) = setup();
        for n in ["zebra", "Apple", "mango"] {
            reg.create(n, "", None).unwrap();
        }
        assert_eq!(names(&reg), vec!["Apple", "mango", DEFAULT_NAME, "zebra"]);
    }

    #[test]
    fn names_must_be_unique_ignoring_case() {
        let (_tmp, reg) = setup();
        reg.create("Work", "", None).unwrap();
        assert!(reg.create("WORK", "", None).is_err());
        reg.create("Home", "", None).unwrap();
        assert!(reg.rename("Home", "work").is_err());
        assert_eq!(reg.rename("Home", "HOME").unwrap().name, "HOME");
    }

    #[test]
    fn bad_names_are_refused() {
        for bad in ["", "   ", "a/b", "a:b", "13625F6B", "two\nlines", &"x".repeat(61)] {
            assert!(validate_name(bad).is_err(), "{:?} should be refused", bad);
        }
        assert_eq!(validate_name("  Work  ").unwrap(), "Work");
    }

    #[test]
    fn find_takes_a_name_a_uuid_or_a_short_id() {
        let (_tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        assert_eq!(reg.find("work").unwrap().uuid, f.uuid);
        assert_eq!(reg.find(&f.uuid.to_lowercase()).unwrap().uuid, f.uuid);
        assert_eq!(reg.find(&f.short_id()).unwrap().uuid, f.uuid);
        assert!(reg.find("nope").is_err());
    }

    #[test]
    fn add_refuses_a_copy_and_changes_nothing() {
        let (tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        let backup = tmp.path().join("backup");
        copy_dir(f.db_path.parent().unwrap(), &backup.join("renamed-copy"));

        let err = reg.add(&backup).unwrap_err();
        assert!(err.contains("copy"), "{}", err);
        assert!(reg.added().is_empty());
        assert_eq!(reg.scan().folders.len(), 2);
    }

    #[test]
    fn add_refuses_a_name_already_taken() {
        let (tmp, reg) = setup();
        reg.create("Work", "", None).unwrap();
        let other = Registry::new(tmp.path().join("other"));
        let there = tmp.path().join("other");
        std::fs::create_dir_all(&there).unwrap();
        other.create("work", "", None).unwrap();

        let err = reg.add(&there).unwrap_err();
        assert!(err.contains("name") || err.contains("copy"), "{}", err);
        assert!(reg.added().is_empty());
    }

    #[test]
    fn add_refuses_a_duplicate_inside_the_directory_being_added() {
        let (tmp, reg) = setup();
        let other_home = tmp.path().join("other-home");
        let f = Registry::new(&other_home).create("Solo", "", None).unwrap();
        let there = tmp.path().join("there");
        copy_dir(f.db_path.parent().unwrap(), &there.join("a"));
        copy_dir(f.db_path.parent().unwrap(), &there.join("b"));

        assert!(reg.add(&there).is_err());
        assert!(reg.added().is_empty());
    }

    #[test]
    fn add_finds_folders_and_a_second_add_is_refused() {
        let (tmp, reg) = setup();
        let other_home = tmp.path().join("other-home");
        let f = Registry::new(&other_home).create("Solo", "", None).unwrap();
        let there = tmp.path().join("there");
        copy_dir(f.db_path.parent().unwrap(), &there.join(&f.uuid));

        let added = reg.add(&there).unwrap();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].uuid, f.uuid);
        assert!(reg.add(&there).is_err());
    }

    #[test]
    fn add_gives_an_older_store_an_identity() {
        let (tmp, reg) = setup();
        let old = tmp.path().join("old-machine");
        crate::db::DatabaseManager::new(&old.join(DB_FILE).to_string_lossy()).unwrap();

        let added = reg.add(&old).unwrap();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].name, "old-machine");
    }

    #[test]
    fn add_never_gives_a_new_identity_to_a_copy_of_an_older_store() {
        let (tmp, reg) = setup();
        let insert = |db: &Path| {
            Connection::open(db)
                .unwrap()
                .execute(
                    "INSERT INTO memories (memory_uuid, payload) VALUES (?1, '{}')",
                    ["11111111-2222-3333-4444-555555555555"],
                )
                .unwrap();
        };
        // Same memory in the first folder and in a store with no identity yet,
        // which is what copying a pre-5.0 memory.db leaves behind.
        insert(&reg.ensure_default().unwrap());
        let copy = tmp.path().join("restored");
        let copy_db = copy.join(DB_FILE);
        crate::db::DatabaseManager::new(&copy_db.to_string_lossy()).unwrap();
        insert(&copy_db);

        let err = reg.add(&copy).unwrap_err();
        assert!(err.contains("memories that are also in"), "{}", err);
        assert!(read_identity(&copy_db).is_none(), "the copy must not be stamped");
        assert!(reg.added().is_empty());
    }

    #[test]
    fn a_relocated_folder_keeps_its_uuid() {
        let (tmp, reg) = setup();
        let from = tmp.path().join("from");
        let f = reg.create("Work", "", Some(&from)).unwrap();
        let to = tmp.path().join("to");
        std::fs::create_dir_all(&to).unwrap();
        std::fs::rename(f.db_path.parent().unwrap(), to.join(&f.uuid)).unwrap();
        reg.remove(&from).unwrap();

        reg.add(&to).unwrap();
        assert_eq!(reg.find("Work").unwrap().uuid, f.uuid);
    }

    fn add_memory(db: &Path, uuid: &str) {
        Connection::open(db)
            .unwrap()
            .execute("INSERT INTO memories (memory_uuid, payload) VALUES (?1, '{}')", [uuid])
            .unwrap();
    }

    const M1: &str = "aaaaaaaa-2222-3333-4444-555555555555";
    const M2: &str = "bbbbbbbb-2222-3333-4444-555555555555";

    #[test]
    fn two_copies_of_an_older_store_get_the_same_identity() {
        let tmp = TempDir::new().unwrap();
        let (a, b) = (tmp.path().join("a").join(DB_FILE), tmp.path().join("b").join(DB_FILE));
        for db in [&a, &b] {
            crate::db::DatabaseManager::new(&db.to_string_lossy()).unwrap();
            add_memory(db, M1);
            stamp(db, "x").unwrap();
        }
        assert_eq!(read_identity(&a).unwrap().uuid, read_identity(&b).unwrap().uuid);
    }

    #[test]
    fn empty_stores_get_different_identities() {
        let tmp = TempDir::new().unwrap();
        let (a, b) = (tmp.path().join("a").join(DB_FILE), tmp.path().join("b").join(DB_FILE));
        for db in [&a, &b] {
            crate::db::DatabaseManager::new(&db.to_string_lossy()).unwrap();
            stamp(db, "x").unwrap();
        }
        assert_ne!(read_identity(&a).unwrap().uuid, read_identity(&b).unwrap().uuid);
    }

    #[test]
    fn relocate_keeps_every_uuid_and_memory() {
        let (tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        add_memory(&f.db_path, M1);
        reg.use_folder("Work").unwrap();
        let dest = tmp.path().join("elsewhere");

        let moved = reg.relocate("Work", &dest).unwrap();
        assert_eq!(moved.folder.uuid, f.uuid);
        assert!(moved.left_behind.is_none());
        assert!(!f.db_path.exists());
        assert_eq!(moved.folder.stats().unwrap().0, 1);
        assert_eq!(reg.added(), vec![canonical(&dest)]);
        assert!(reg.scan().blocked.is_empty());
        assert_eq!(canonical(&reg.resolve(None, None, None).unwrap()), canonical(&moved.folder.db_path));
    }

    #[test]
    fn a_move_takes_an_emptied_directory_off_the_list_and_keeps_one_still_in_use() {
        let (tmp, reg) = setup();
        let projects = tmp.path().join("projects");
        let shared = tmp.path().join("shared");
        reg.create("Work", "", Some(&projects)).unwrap();
        reg.create("Home", "", Some(&shared)).unwrap();
        reg.create("Garden", "", Some(&shared)).unwrap();
        let dest = tmp.path().join("moved");

        // projects held only Work, so once Work leaves, projects leaves the list.
        reg.relocate("Work", &dest).unwrap();
        let listed: Vec<_> = reg.added().iter().map(|d| canonical(d)).collect();
        assert!(!listed.contains(&canonical(&projects)));
        assert!(listed.contains(&canonical(&dest)));

        // shared still holds Garden after Home moves, so it stays on the list.
        reg.relocate("Home", &dest).unwrap();
        let listed: Vec<_> = reg.added().iter().map(|d| canonical(d)).collect();
        assert!(listed.contains(&canonical(&shared)));
        assert!(reg.find("Garden").is_ok());
    }

    #[test]
    fn relocate_refuses_the_first_folder_and_a_directory_of_unknown_stores() {
        let (tmp, reg) = setup();
        reg.ensure_default().unwrap();
        assert!(reg.relocate(DEFAULT_NAME, &tmp.path().join("x")).is_err());

        let f = reg.create("Work", "", None).unwrap();
        let other = tmp.path().join("other");
        Registry::new(&other).ensure_default().unwrap();
        assert!(reg.relocate("Work", &other).unwrap_err().contains("Add it first"));
        assert!(f.db_path.exists());
    }

    #[test]
    fn crossing_drives_carries_writes_still_in_the_wal_and_retires_the_original() {
        let (tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        let writer = crate::db::DatabaseManager::new(&f.db_path.to_string_lossy()).unwrap();
        drop(writer);
        // Keep a connection open so the write stays in the -wal file, as it would
        // beside a running app. A plain copy of memory.db would lose it.
        let held = Connection::open(&f.db_path).unwrap();
        held.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;").unwrap();
        held.execute("INSERT INTO memories (memory_uuid, payload) VALUES (?1, '{}')", [M1]).unwrap();
        held.execute("INSERT INTO memories (memory_uuid, payload) VALUES (?1, '{}')", [M2]).unwrap();
        let wal = PathBuf::from(format!("{}-wal", f.db_path.display()));
        assert!(std::fs::metadata(&wal).unwrap().len() > 0, "the test needs a live WAL");

        let dest = tmp.path().join("drive");
        std::fs::create_dir_all(&dest).unwrap();
        let dir = f.db_path.parent().unwrap().to_path_buf();
        let lock = quiesce(&f.db_path).unwrap();
        let retired = copy_then_retire(&dir, &dest, &f, lock).unwrap();
        drop(held);

        let moved = read_identity(&dest.join(&f.uuid).join(DB_FILE)).unwrap();
        assert_eq!(moved.uuid, f.uuid);
        assert_eq!(moved.stats().unwrap().0, 2);
        assert!(!dir.exists());
        assert!(retired.join(DB_FILE).is_file(), "the original is retired, never deleted");
    }

    #[test]
    fn a_folder_being_moved_is_not_visible_at_its_destination() {
        let (tmp, reg) = setup();
        let there = tmp.path().join("there");
        reg.create("Stays", "", Some(&there)).unwrap();
        let other_home = tmp.path().join("other-home");
        let f = Registry::new(&other_home).create("Mover", "", None).unwrap();
        copy_dir(f.db_path.parent().unwrap(), &there.join(".moving-x"));
        assert!(reg.find("Mover").is_err());
    }

    #[test]
    fn another_products_store_is_refused_and_left_unchanged() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join(DB_FILE);
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT);
             INSERT INTO _meta VALUES ('product', 'elsewhere');
             CREATE TABLE signals (signal_uuid TEXT);",
        )
        .unwrap();
        drop(conn);
        let before = std::fs::read(&db).unwrap();

        let err = crate::db::DatabaseManager::new(&db.to_string_lossy()).err().unwrap();
        assert!(err.contains("another product"), "{}", err);
        assert_eq!(std::fs::read(&db).unwrap(), before);
    }

    #[test]
    fn stores_from_earlier_names_can_be_added() {
        let (tmp, reg) = setup();
        let old = tmp.path().join("old");
        let db = old.join(DB_FILE);
        crate::db::DatabaseManager::new(&db.to_string_lossy()).unwrap();
        Connection::open(&db)
            .unwrap()
            .execute("UPDATE _meta SET value = 'clawmark' WHERE key = 'product'", [])
            .unwrap();
        assert_eq!(reg.add(&old).unwrap().len(), 1);
    }

    #[test]
    fn a_backup_holds_writes_still_in_the_wal() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join(DB_FILE);
        crate::db::DatabaseManager::new(&db.to_string_lossy()).unwrap();
        let held = Connection::open(&db).unwrap();
        held.execute_batch("PRAGMA wal_autocheckpoint=0;").unwrap();
        held.execute("INSERT INTO memories (memory_uuid, payload) VALUES (?1, '{}')", [M1]).unwrap();

        let target = tmp.path().join("backup.db");
        backup_to(&db, &target).unwrap();
        drop(held);
        let count: i64 = Connection::open(&target)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    // ---- From the outside review of v5 (Codex Astra, 2 Nov). ----

    #[test]
    fn review1_a_waiting_writer_cannot_write_to_a_moved_original() {
        let (tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        let waiting = Connection::open(&f.db_path).unwrap();
        waiting.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get::<_, i64>(0)).unwrap();
        let dest = tmp.path().join("drive");
        std::fs::create_dir_all(&dest).unwrap();
        let dir = f.db_path.parent().unwrap().to_path_buf();
        copy_then_retire(&dir, &dest, &f, quiesce(&f.db_path).unwrap()).unwrap();

        let wrote = waiting.execute("INSERT INTO memories (memory_uuid, payload) VALUES (?1, '{}')", [M1]);
        assert!(wrote.is_err(), "a write to the retired original must fail, not succeed where nobody reads");
    }

    #[test]
    fn review3_a_failure_to_retire_never_publishes_a_second_copy() {
        let (tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        std::fs::write(reg.home.join(ARCHIVE_DIR), "in the way").unwrap();
        let dest = tmp.path().join("drive");
        std::fs::create_dir_all(&dest).unwrap();
        let dir = f.db_path.parent().unwrap().to_path_buf();
        let result = copy_then_retire(&dir, &dest, &f, quiesce(&f.db_path).unwrap());

        assert!(result.is_err());
        assert!(f.db_path.exists());
        assert!(!dest.join(&f.uuid).exists(), "nothing may be published when the original can't be retired");
    }

    #[test]
    fn review1_a_waiting_writer_follows_a_move_on_the_same_drive() {
        let (tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        let waiting = Connection::open(&f.db_path).unwrap();
        waiting.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get::<_, i64>(0)).unwrap();

        let moved = reg.relocate("Work", &tmp.path().join("elsewhere")).unwrap();
        waiting.execute("INSERT INTO memories (memory_uuid, payload) VALUES (?1, '{}')", [M1]).unwrap();
        drop(waiting);

        assert_eq!(moved.folder.stats().unwrap().0, 1, "the write must land in the moved folder");
        assert!(!f.db_path.exists());
    }

    #[test]
    fn review2_opening_a_moved_folder_fails_and_creates_nothing() {
        let (tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        let old = f.db_path.clone();
        reg.relocate("Work", &tmp.path().join("elsewhere")).unwrap();

        assert!(crate::db::DatabaseManager::open_existing(&old.to_string_lossy()).is_err());
        assert!(!old.exists(), "no empty store may appear at the old path");
        assert!(!old.parent().unwrap().exists());
    }

    #[test]
    fn review3_a_move_interrupted_after_the_fence_is_finished() {
        let (tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        add_memory(&f.db_path, M1);
        let dest = tmp.path().join("drive");
        std::fs::create_dir_all(&dest).unwrap();
        reg.remember_added(&dest).unwrap();
        // The state after copying and fencing, before retiring.
        let staging = dest.join(format!("{}{}", MOVING_PREFIX, f.uuid));
        copy_dir(f.db_path.parent().unwrap(), &staging);
        Connection::open(&f.db_path)
            .unwrap()
            .execute_batch(&format!(
                "CREATE TRIGGER {}_memories_insert BEFORE INSERT ON memories BEGIN SELECT RAISE(ABORT, 'moved'); END;",
                FENCE_TRIGGER
            ))
            .unwrap();

        let found = reg.find("Work").unwrap();
        assert!(canonical(&found.db_path).starts_with(canonical(&dest)));
        assert_eq!(found.stats().unwrap().0, 1);
        assert!(!f.db_path.exists());
        assert!(reg.scan().blocked.is_empty());
    }

    #[test]
    fn review3_a_copy_left_while_the_original_is_live_is_set_aside() {
        let (tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        let dest = tmp.path().join("drive");
        std::fs::create_dir_all(&dest).unwrap();
        reg.remember_added(&dest).unwrap();
        let staging = dest.join(format!("{}{}", MOVING_PREFIX, f.uuid));
        copy_dir(f.db_path.parent().unwrap(), &staging);

        assert_eq!(canonical(&reg.find("Work").unwrap().db_path), canonical(&f.db_path));
        assert!(!staging.exists());
        assert!(reg.archived().is_empty(), "an abandoned copy must not be offered for restore");
    }

    #[test]
    fn review3_a_registry_failure_leaves_the_folder_findable() {
        let (tmp, reg) = setup();
        reg.create("Work", "", None).unwrap();
        std::fs::create_dir_all(reg.home.join("added-folders.tmp")).unwrap();
        let _ = reg.relocate("Work", &tmp.path().join("elsewhere"));
        assert!(reg.find("Work").is_ok(), "the folder must stay findable whatever the registry did");
    }

    #[test]
    fn review4_a_new_copy_never_takes_over_from_the_original() {
        let (_tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        reg.use_folder("Work").unwrap();
        copy_dir(f.db_path.parent().unwrap(), &reg.home.join("000-copy"));
        add_memory(&f.db_path, M1);

        match reg.resolve(None, None, None) {
            Ok(path) => assert_eq!(canonical(&path), canonical(&f.db_path), "the older copy was selected"),
            Err(e) => assert!(e.contains("copy") || e.contains("copies"), "{}", e),
        }
    }

    #[test]
    fn review6_two_unstamped_copies_in_one_add_are_refused() {
        let (tmp, reg) = setup();
        reg.ensure_default().unwrap();
        let there = tmp.path().join("there");
        for name in ["a", "b"] {
            let db = there.join(name).join(DB_FILE);
            crate::db::DatabaseManager::new(&db.to_string_lossy()).unwrap();
            add_memory(&db, M1);
        }
        assert!(reg.add(&there).is_err());
        assert!(read_identity(&there.join("a").join(DB_FILE)).is_none(), "nothing may be stamped");
    }

    #[test]
    fn review6_identical_unstamped_stores_get_one_identity() {
        let tmp = TempDir::new().unwrap();
        let (a, b) = (tmp.path().join("a.db"), tmp.path().join("b.db"));
        for db in [&a, &b] {
            Connection::open(db).unwrap().execute_batch(&format!(
                "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT);
                 INSERT INTO _meta VALUES ('product','geniuz');
                 CREATE TABLE memories (memory_uuid TEXT, payload TEXT, created_at TEXT);
                 INSERT INTO memories VALUES ('{}','{{}}','2026-01-01 00:00:00');", M1
            )).unwrap();
            stamp(db, "x").unwrap();
        }
        assert_eq!(read_identity(&a).unwrap().uuid, read_identity(&b).unwrap().uuid);
    }

    #[test]
    fn review7_simultaneous_additions_are_all_kept() {
        let (tmp, reg) = setup();
        reg.ensure_default().unwrap();
        let home = reg.home.clone();
        let mut threads = Vec::new();
        for i in 0..24 {
            let (home, at) = (home.clone(), tmp.path().join(format!("d{}", i)));
            threads.push(std::thread::spawn(move || {
                std::fs::create_dir_all(&at).unwrap();
                Registry::new(home).remember_added(&at)
            }));
        }
        let ok = threads.into_iter().map(|t| t.join().unwrap()).filter(Result::is_ok).count();
        assert_eq!(reg.added().len(), ok, "every addition reported as done must be on the list");
        assert_eq!(ok, 24);
    }

    #[test]
    fn review8_a_name_cannot_shadow_a_uuid() {
        let (_tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        reg.use_folder("Work").unwrap();
        let _ = reg.create(&f.uuid, "", None);
        assert_eq!(canonical(&reg.resolve(None, None, None).unwrap()), canonical(&f.db_path));
        assert!(validate_name(&f.uuid).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn review9_a_symlinked_folder_is_refused_before_anything_moves() {
        let (tmp, reg) = setup();
        let elsewhere = Registry::new(tmp.path().join("other"));
        let f = elsewhere.create("Linked", "", None).unwrap();
        std::os::unix::fs::symlink(f.db_path.parent().unwrap(), reg.home.join("link")).unwrap();
        assert_eq!(reg.find("Linked").unwrap().uuid, f.uuid);

        assert!(reg.relocate("Linked", &tmp.path().join("deep").join("er")).is_err());
        assert!(reg.find("Linked").is_ok(), "the folder must still be there");
        assert!(reg.archive("Linked").is_err());
    }

    #[test]
    fn review10_restore_compares_names_like_everything_else() {
        let (_tmp, reg) = setup();
        reg.create("École", "", None).unwrap();
        reg.archive("École").unwrap();
        reg.create("école", "", None).unwrap();
        assert!(reg.restore("École").is_err());
    }

    // ---- From the second review (Codex Astra, 3 Nov). ----

    fn staged(reg: &Registry, f: &Folder, dest: &Path) -> PathBuf {
        std::fs::create_dir_all(dest).unwrap();
        reg.remember_added(dest).unwrap();
        let staging = dest.join(format!("{}{}", MOVING_PREFIX, f.uuid));
        copy_dir(f.db_path.parent().unwrap(), &staging);
        staging
    }

    #[test]
    fn pass2_a_scan_never_touches_a_move_still_running() {
        let (tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        let staging = staged(&reg, &f, &tmp.path().join("drive"));
        let running = std::fs::File::create(staging.join(MOVE_OWNER)).unwrap();
        running.lock().unwrap();

        reg.scan();
        assert!(staging.exists(), "the running mover still needs this");
        drop(running);
    }

    #[test]
    fn pass2_a_copy_with_no_proof_waits_when_its_original_is_out_of_reach() {
        let (tmp, reg) = setup();
        let source = tmp.path().join("removable");
        let f = reg.create("Work", "", Some(&source)).unwrap();
        let staging = staged(&reg, &f, &tmp.path().join("drive"));
        add_memory(&f.db_path, M1); // newer than the copy
        std::fs::rename(&source, tmp.path().join("unplugged")).unwrap();

        let scan = reg.scan();
        assert_eq!(scan.pending_moves.len(), 1);
        assert!(staging.exists());
        assert!(reg.find("Work").is_err(), "a stale copy must not become the folder");
    }

    #[test]
    fn pass2_a_committed_copy_is_finished_even_with_its_original_out_of_reach() {
        let (tmp, reg) = setup();
        let source = tmp.path().join("removable");
        let f = reg.create("Work", "", Some(&source)).unwrap();
        let dest = tmp.path().join("drive");
        let staging = staged(&reg, &f, &dest);
        std::fs::File::create(staging.join(MOVE_COMMITTED)).unwrap();
        std::fs::rename(&source, tmp.path().join("unplugged")).unwrap();

        let found = reg.find("Work").unwrap();
        assert!(canonical(&found.db_path).starts_with(canonical(&dest)));
        assert!(!found.db_path.parent().unwrap().join(MOVE_COMMITTED).exists());
    }

    #[test]
    fn pass2_the_fence_refuses_every_kind_of_write() {
        let (tmp, reg) = setup();
        let f = reg.create("Work", "note", None).unwrap();
        let waiting = Connection::open(&f.db_path).unwrap();
        waiting.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get::<_, i64>(0)).unwrap();
        let dest = tmp.path().join("drive");
        std::fs::create_dir_all(&dest).unwrap();
        let dir = f.db_path.parent().unwrap().to_path_buf();
        copy_then_retire(&dir, &dest, &f, quiesce(&f.db_path).unwrap()).unwrap();

        for sql in [
            "UPDATE _meta SET value = 'changed' WHERE key = 'folder_description'",
            "INSERT OR REPLACE INTO _meta (key, value) VALUES ('folder_description', 'changed')",
            "INSERT OR REPLACE INTO embedding_meta (key, value) VALUES ('model', 'other')",
            "INSERT INTO memory_embeddings (memory_uuid, embedding) VALUES ('x', x'00')",
        ] {
            assert!(waiting.execute(sql, []).is_err(), "{}", sql);
        }
        assert!(!dest.join(&f.uuid).join(MOVE_COMMITTED).exists());
        assert!(!dest.join(&f.uuid).join(MOVE_OWNER).exists());
    }

    #[test]
    fn pass2_simultaneous_creates_of_one_name_leave_one_folder() {
        let (_tmp, reg) = setup();
        reg.ensure_default().unwrap();
        let home = reg.home.clone();
        let threads: Vec<_> = (0..16)
            .map(|_| {
                let home = home.clone();
                std::thread::spawn(move || Registry::new(home).create("Work", "", None).is_ok())
            })
            .collect();
        let ok = threads.into_iter().map(|t| t.join().unwrap()).filter(|x| *x).count();
        assert_eq!(ok, 1);
        assert_eq!(names(&reg), vec![DEFAULT_NAME, "Work"]);
    }

    #[test]
    fn pass2_a_name_clash_made_by_hand_is_refused_by_name_and_mended_by_id() {
        let (tmp, reg) = setup();
        let first = reg.create("Work", "", None).unwrap();
        let other = tmp.path().join("other");
        let second = Registry::new(&other).create("work", "", None).unwrap();
        reg.remember_added(&other).unwrap();

        assert!(reg.find("Work").unwrap_err().contains("More than one"));
        assert_eq!(reg.find(&first.uuid).unwrap().uuid, first.uuid);
        reg.rename(&second.uuid, "Other").unwrap();
        assert_eq!(reg.find("Work").unwrap().uuid, first.uuid);
    }

    #[test]
    fn pass2_a_stamped_and_an_unstamped_copy_are_refused_together() {
        let (tmp, reg) = setup();
        reg.ensure_default().unwrap();
        let there = tmp.path().join("there");
        let stamped = there.join("a").join(DB_FILE);
        crate::db::DatabaseManager::new(&stamped.to_string_lossy()).unwrap();
        add_memory(&stamped, M1);
        stamp(&stamped, "A").unwrap();
        let plain = there.join("b").join(DB_FILE);
        crate::db::DatabaseManager::new(&plain.to_string_lossy()).unwrap();
        add_memory(&plain, M1);

        assert!(reg.add(&there).is_err());
        assert!(read_identity(&plain).is_none());
    }

    #[test]
    fn pass2_a_shared_memory_outside_the_oldest_rows_is_still_found() {
        let (tmp, reg) = setup();
        let first = reg.ensure_default().unwrap();
        add_memory(&first, M1);
        let copy = tmp.path().join("copy");
        let db = copy.join(DB_FILE);
        crate::db::DatabaseManager::new(&db.to_string_lossy()).unwrap();
        add_memory(&db, M1);
        let conn = Connection::open(&db).unwrap();
        for i in 0..25 {
            conn.execute(
                "INSERT INTO memories (memory_uuid, payload, created_at) VALUES (?1, '{}', '2001-01-01 00:00:00')",
                [format!("dddddddd-2222-3333-4444-{:012}", i)],
            )
            .unwrap();
        }
        drop(conn);
        assert!(reg.add(&copy).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn pass2_a_hard_link_is_named_for_what_it_is() {
        let (_tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        let linked = reg.home.join("linked");
        std::fs::create_dir_all(&linked).unwrap();
        std::fs::hard_link(&f.db_path, linked.join(DB_FILE)).unwrap();
        let err = reg.find("Work").unwrap_err();
        assert!(err.contains("two names for one file"), "{}", err);
    }

    #[test]
    fn pinned_folders_come_first_and_stay_pinned_through_a_rename() {
        let (_tmp, reg) = setup();
        reg.create("Apple", "", None).unwrap();
        reg.create("Zebra", "", None).unwrap();
        reg.set_pinned("Zebra", true).unwrap();
        assert_eq!(names(&reg), vec!["Zebra", "Apple", DEFAULT_NAME]);

        reg.rename("Zebra", "Yak").unwrap();
        assert_eq!(names(&reg)[0], "Yak");

        reg.set_pinned("Yak", false).unwrap();
        assert_eq!(names(&reg), vec!["Apple", DEFAULT_NAME, "Yak"]);
    }

    #[test]
    fn addresses_are_two_eight_character_ids() {
        assert_eq!(
            parse_address("13625f6b:3487B9B0"),
            Some(("13625F6B".to_string(), "3487B9B0".to_string()))
        );
        for bad in ["13625F6B", "13625F6B:", "note: 3487B9B0", "13625F6B:3487B9B", "ZZZZZZZZ:3487B9B0"] {
            assert!(parse_address(bad).is_none(), "{}", bad);
        }
    }

    #[test]
    fn searching_across_labels_each_hit_and_merges_newest_first() {
        let (_tmp, reg) = setup();
        let a = reg.create("Alpha", "", None).unwrap();
        let b = reg.create("Beta", "", None).unwrap();
        let open = |f: &Folder| crate::db::DatabaseManager::new(&f.db_path.to_string_lossy()).unwrap();
        open(&a).keep("the older note", Some("older"), None, Some("2026-01-01 00:00:00")).unwrap();
        open(&b).keep("the newer note", Some("newer"), None, Some("2026-02-01 00:00:00")).unwrap();
        open(&b).keep("unrelated", Some("other"), None, Some("2026-03-01 00:00:00")).unwrap();

        let window = Window::parse(None, None).unwrap();
        let found = search_across(&reg, &Search::Keyword("note"), &window, 10);
        assert_eq!(found.searched, 3);
        assert_eq!(found.memories, 3);
        let names: Vec<_> = found.hits.iter().map(|h| h.folder.name.as_str()).collect();
        assert_eq!(names, vec!["Beta", "Alpha"]);
        assert!(found.hits[0].address().starts_with(&b.short_id()));

        let recent = search_across(&reg, &Search::Recent, &window, 1);
        assert_eq!(recent.hits[0].entry.gist, "other");
    }

    #[test]
    fn add_leaves_a_database_it_does_not_own_untouched() {
        let (tmp, reg) = setup();
        let foreign = tmp.path().join("foreign");
        std::fs::create_dir_all(foreign.join("ABC")).unwrap();
        let db = foreign.join("ABC").join(DB_FILE);
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE other (x);").unwrap();
        drop(conn);
        let before = std::fs::read(&db).unwrap();

        assert!(reg.add(&foreign).unwrap_err().contains("No Geniuz folders"));
        assert_eq!(std::fs::read(&db).unwrap(), before);
        assert!(reg.added().is_empty());
    }

    #[test]
    fn a_copy_appearing_later_blocks_every_copy_until_one_is_moved() {
        let (tmp, reg) = setup();
        let first = tmp.path().join("first");
        let f = reg.create("Work", "", Some(&first)).unwrap();
        let second = tmp.path().join("second");
        reg.create("Other", "", Some(&second)).unwrap();
        copy_dir(f.db_path.parent().unwrap(), &second.join("dropped-in"));

        // Nothing can tell which is the original, so neither may be used.
        let scan = reg.scan();
        assert_eq!(scan.blocked.len(), 2);
        assert_eq!(scan.folders.iter().filter(|x| x.uuid == f.uuid).count(), 0);
        assert!(reg.find("Work").unwrap_err().contains("two copies"));
        std::fs::rename(second.join("dropped-in"), tmp.path().join("set-aside")).unwrap();
        assert_eq!(canonical(&reg.find("Work").unwrap().db_path), canonical(&f.db_path));
    }

    #[test]
    fn an_unreachable_added_directory_is_reported_and_nothing_else_breaks() {
        let (tmp, reg) = setup();
        let gone = tmp.path().join("drive");
        reg.create("OnDrive", "", Some(&gone)).unwrap();
        std::fs::rename(&gone, tmp.path().join("unplugged")).unwrap();

        let scan = reg.scan();
        assert_eq!(scan.unavailable.len(), 1);
        assert_eq!(names(&reg), vec![DEFAULT_NAME]);
        assert_eq!(reg.added().len(), 1);
    }

    #[test]
    fn resolve_order_is_flag_env_dotfile_current_first() {
        let (tmp, reg) = setup();
        let work = reg.create("Work", "", None).unwrap();
        let home = reg.create("Home", "", None).unwrap();
        let cur = reg.create("Current", "", None).unwrap();
        let project = tmp.path().join("project").join("deep");
        std::fs::create_dir_all(&project).unwrap();

        let first = reg.resolve(None, None, None).unwrap();
        assert_eq!(first.file_name().unwrap(), DB_FILE);
        assert_eq!(first.parent().unwrap(), reg.home);

        reg.use_folder("Current").unwrap();
        assert_eq!(reg.resolve(None, None, Some(&project)).unwrap(), cur.db_path);

        std::fs::write(tmp.path().join("project").join(DOTFILE), "Home\n").unwrap();
        assert_eq!(reg.resolve(None, None, Some(&project)).unwrap(), home.db_path);

        assert_eq!(
            reg.resolve(None, Some("/elsewhere/x.db"), Some(&project)).unwrap(),
            PathBuf::from("/elsewhere/x.db")
        );
        assert_eq!(reg.resolve(Some("Work"), Some("/elsewhere/x.db"), Some(&project)).unwrap(), work.db_path);
    }

    #[test]
    fn a_wrong_name_errors_and_never_falls_back() {
        let (tmp, reg) = setup();
        assert!(reg.resolve(Some("typo"), None, None).is_err());

        let dir = tmp.path().join("p");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(DOTFILE), "typo").unwrap();
        assert!(reg.resolve(None, None, Some(&dir)).unwrap_err().contains(DOTFILE));
    }

    #[test]
    fn current_survives_a_rename() {
        let (_tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        reg.use_folder("Work").unwrap();
        reg.rename("Work", "Clients").unwrap();
        assert_eq!(reg.resolve(None, None, None).unwrap(), f.db_path);
    }

    #[test]
    fn edits_keep_the_old_value() {
        let (_tmp, reg) = setup();
        let f = reg.create("Work", "first", None).unwrap();
        assert_eq!(reg.describe("Work", "second").unwrap().description, "second");

        let conn = open_read_only(&f.db_path).unwrap();
        let old: String = conn
            .query_row(
                "SELECT value FROM _meta WHERE key LIKE 'folder_description@%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(old, "first");
    }

    #[test]
    fn archive_moves_without_deleting_and_the_folder_stays_hidden() {
        let (_tmp, reg) = setup();
        let f = reg.create("Scratch", "", None).unwrap();

        let moved = reg.archive("Scratch").unwrap();
        assert!(moved.join(DB_FILE).is_file());
        assert!(!f.db_path.exists());
        assert!(reg.find("Scratch").is_err());
    }

    #[test]
    fn the_default_folder_is_not_archived_until_another_is_chosen() {
        let (_tmp, reg) = setup();
        let f = reg.create("Scratch", "", None).unwrap();
        reg.use_folder("Scratch").unwrap();

        let err = reg.archive("Scratch").unwrap_err();
        assert!(err.contains("default folder"), "{err}");
        assert!(f.db_path.exists(), "nothing moved");
        assert!(reg.current().unwrap().eq_ignore_ascii_case(&f.uuid), "the default is unchanged");

        reg.ensure_default().unwrap();
        reg.use_folder(DEFAULT_NAME).unwrap();
        assert!(reg.archive("Scratch").is_ok());
    }

    #[test]
    fn an_archived_folder_can_be_seen_and_restored_unchanged() {
        let (_tmp, reg) = setup();
        let f = reg.create("Scratch", "note", None).unwrap();
        assert!(reg.archived().is_empty());

        reg.archive("Scratch").unwrap();
        let archived = reg.archived();
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].uuid, f.uuid);
        assert!(reg.find("Scratch").is_err());

        let back = reg.restore("scratch").unwrap();
        assert_eq!(back.uuid, f.uuid);
        assert_eq!(back.db_path, f.db_path);
        assert_eq!(back.description, "note");
        assert!(reg.archived().is_empty());
    }

    #[test]
    fn restoring_is_refused_if_the_name_has_been_taken() {
        let (_tmp, reg) = setup();
        reg.create("Scratch", "", None).unwrap();
        reg.archive("Scratch").unwrap();
        reg.create("scratch", "", None).unwrap();

        assert!(reg.restore("Scratch").unwrap_err().contains("already a folder"));
        assert_eq!(reg.archived().len(), 1);
    }

    #[test]
    fn restoring_is_refused_if_a_copy_is_in_use() {
        let (tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        let elsewhere = tmp.path().join("elsewhere");
        copy_dir(f.db_path.parent().unwrap(), &elsewhere.join("copy"));
        reg.archive("Work").unwrap();
        reg.add(&elsewhere).unwrap();

        assert!(reg.restore(&f.uuid).unwrap_err().contains("in use"));
        assert_eq!(reg.archived().len(), 1);
    }

    #[test]
    fn the_original_left_by_a_move_across_drives_is_not_offered_for_restore() {
        let (tmp, reg) = setup();
        let f = reg.create("Work", "", None).unwrap();
        let dest = tmp.path().join("drive");
        std::fs::create_dir_all(&dest).unwrap();
        let dir = f.db_path.parent().unwrap().to_path_buf();
        copy_then_retire(&dir, &dest, &f, quiesce(&f.db_path).unwrap()).unwrap();
        assert!(reg.archived().is_empty());
    }

    #[test]
    fn every_earlier_description_is_kept_even_within_one_second() {
        let (_tmp, reg) = setup();
        let f = reg.create("Work", "one", None).unwrap();
        reg.describe("Work", "two").unwrap();
        reg.describe("Work", "three").unwrap();
        reg.describe("Work", "four").unwrap();
        let conn = open_read_only(&f.db_path).unwrap();
        let mut kept: Vec<String> = conn
            .prepare("SELECT value FROM _meta WHERE key LIKE 'folder_description@%'")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        kept.sort();
        assert_eq!(kept, ["one", "three", "two"]);
        assert_eq!(reg.find("Work").unwrap().description, "four");
    }

    #[test]
    fn the_first_folder_cannot_be_archived() {
        let (_tmp, reg) = setup();
        reg.ensure_default().unwrap();
        assert!(reg.archive(DEFAULT_NAME).is_err());
    }

    #[test]
    fn identity_is_invisible_to_memory_counts_and_old_versions() {
        let (_tmp, reg) = setup();
        let f = reg.create("Work", "note", None).unwrap();
        let db = crate::db::DatabaseManager::new(&f.db_path.to_string_lossy()).unwrap();
        drop(db);
        assert_eq!(f.stats().unwrap().0, 0);
        let conn = open_read_only(&f.db_path).unwrap();
        let product: String = conn
            .query_row("SELECT value FROM _meta WHERE key='product'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(product, "geniuz");
    }
}

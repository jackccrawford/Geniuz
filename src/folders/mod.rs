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
/// The file name before Geniuz 4.0: found only to say what to do with it.
const OLD_DB_FILE: &str = "station.db";
const ADDED_FILE: &str = "added-folders";
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
const KEY_FACE: &str = "folder_face";

#[derive(Debug, Clone, PartialEq)]
pub struct Folder {
    pub uuid: String,
    pub name: String,
    pub description: String,
    /// One emoji its writer chose to show beside its name. Empty when none was chosen.
    pub face: String,
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
    /// Why the first folder couldn't be opened, when it couldn't (a file from before
    /// Geniuz 4.0, say), so lists can say so rather than just "unavailable".
    pub first_folder_problem: Option<String>,
    /// Folders still in a pre-4.0 `station.db`: not opened, but named, so they
    /// don't vanish from lists without a word.
    pub old_format: Vec<PathBuf>,
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

    /// The database of the first folder.
    fn default_db(&self) -> PathBuf {
        self.home.join(DB_FILE)
    }

    /// Create the first folder if needed and give it an identity in place. The
    /// file never moves, and older versions can still open it.
    pub fn ensure_default(&self) -> Result<PathBuf, String> {
        let db = self.default_db();
        // A first folder from before Geniuz 4.0 was station.db. Making a new, empty
        // memory.db beside it would hide its memories without a word, so it's refused
        // with the way forward instead.
        let old = self.home.join(OLD_DB_FILE);
        if !db.exists() && old.exists() {
            return Err(format!(
                "Your first folder is {}, from a Geniuz before 4.0. Back it up and run {} on it, \
                 then rename it memory.db.",
                old.display(),
                crate::db::MIGRATION_SCRIPT
            ));
        }
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
        if let Err(e) = self.ensure_default() {
            out.unavailable.push(self.home.clone());
            out.first_folder_problem = Some(e);
        }

        let mut roots = vec![self.home.clone()];
        for dir in self.added() {
            if dir.is_dir() {
                roots.push(dir);
            } else {
                out.unavailable.push(dir);
            }
        }

        // A station.db with no memory.db beside it is a folder from before 4.0. The
        // first folder's is reported above; any other is listed here.
        for root in &roots {
            let mut dirs = vec![root.clone()];
            if let Ok(entries) = std::fs::read_dir(root) {
                dirs.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
            }
            for dir in dirs {
                if canonical(&dir) == canonical(&self.home) {
                    continue;
                }
                if dir.join(OLD_DB_FILE).is_file() && !dir.join(DB_FILE).exists() {
                    out.old_format.push(dir.join(OLD_DB_FILE));
                }
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

    /// Which database a command reads. In order: the `--in` flag, the `GENIUZ_DB`
    /// file path, a `.geniuz-folder` file here or in a parent directory, then the
    /// first folder ("My memory"). Writing doesn't use this: a keep goes only to the
    /// folder it names (see `get_db_to_keep` in main.rs, and the MCP keep tool).
    pub fn resolve(
        &self,
        flag: Option<&str>,
        env_db: Option<&str>,
        cwd: Option<&Path>,
    ) -> Result<PathBuf, String> {
        if let Some(q) = flag {
            return Ok(self.find(q)?.db_path);
        }
        if let Some(path) = env_db.filter(|s| !s.is_empty()) {
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
            let old = dir.join(OLD_DB_FILE).is_file()
                || std::fs::read_dir(&dir).map(|es| es.flatten().any(|e| e.path().join(OLD_DB_FILE).is_file())).unwrap_or(false);
            if old {
                return Err(format!(
                    "No Geniuz folders from 4.0 or later in {}. Its station.db files are from an older Geniuz: back them up and run {} on each.",
                    dir.display(),
                    crate::db::MIGRATION_SCRIPT
                ));
            }
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

    /// Set the face shown beside a folder's name, or clear it with an empty one.
    /// Earlier faces are kept, as earlier names and descriptions are.
    pub fn set_face(&self, query: &str, face: &str) -> Result<Folder, String> {
        let folder = self.find(query)?;
        let face = validate_face(face)?;
        if face != folder.face {
            replace_meta(&folder.db_path, KEY_FACE, &face)?;
        }
        self.find(&folder.uuid)
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

/// How names are compared, everywhere: ignoring case, by Unicode rules.
fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// A name is short, one line, and can't be mistaken for an address or a path.
/// A face is one emoji, or one or two characters. It sits on a small tile, so
/// a word won't fit; the name is the place for words.
pub fn validate_face(face: &str) -> Result<String, String> {
    let face = face.trim();
    if face.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("A face is one emoji, with no spaces or line breaks.".into());
    }
    // An emoji can be several code points joined together (a flag, a family),
    // so the limit is generous for those and strict for plain letters.
    let count = face.chars().count();
    let limit = if face.is_ascii() { 2 } else { 12 };
    if count > limit {
        return Err("A face is one emoji, or one or two characters.".into());
    }
    Ok(face.to_string())
}

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

/// What `geniuz folder use` says since 5.0.8, which has no default folder. The
/// command stays so scripts and agents that learned it are told the new way.
/// (A `current` file left by an older version is no longer read, and is left alone.)
pub const NO_DEFAULT_FOLDER: &str = "Geniuz 5.0.8 has no default folder: every keep names its folder, with \
     -f <folder>. `geniuz folder list` lists them.";

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

mod across;
mod archive;
mod identity;
mod moving;

pub use across::*;
pub use identity::identity_at;
pub use moving::backup_to;
use identity::*;
use moving::*;

#[cfg(test)]
mod tests;

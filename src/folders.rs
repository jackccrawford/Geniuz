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
//! new directory name is still recognised as the same folder: `add` refuses it
//! outright, and one that appears later is blocked until it is moved. Nothing
//! here ever deletes a file.
//!
//! `relocate` is the supported way to move a folder. Copying a live database by
//! hand can miss whatever is still in its `-wal` file, so relocating flushes
//! that file and holds off writers first.
//!
//! Reads use a read-only connection and never `DatabaseManager::new`, which
//! applies the schema on open: listing must not change a file it doesn't own.

use crate::db::SignalEntry;
use crate::embedding::EmbeddingBackend;
use crate::window::Window;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const DB_FILE: &str = "memory.db";
const LEGACY_DB_FILE: &str = "station.db";
const ADDED_FILE: &str = "added-folders";
const CURRENT_FILE: &str = "current";
const ARCHIVE_DIR: &str = "archive";
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

/// A second copy of a folder, or a second folder using a name already taken.
#[derive(Debug, Clone, PartialEq)]
pub struct Blocked {
    pub db_path: PathBuf,
    pub kept: PathBuf,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct Scan {
    pub folders: Vec<Folder>,
    pub blocked: Vec<Blocked>,
    /// Added directories that can't be reached right now, such as an unplugged drive.
    pub unavailable: Vec<PathBuf>,
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

    /// The database of the first folder: `memory.db`, or an older `station.db`
    /// if that is all there is.
    fn default_db(&self) -> PathBuf {
        let db = self.home.join(DB_FILE);
        let legacy = self.home.join(LEGACY_DB_FILE);
        if !db.exists() && legacy.exists() { legacy } else { db }
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

        let mut seen_paths = HashSet::new();
        let mut by_uuid: HashMap<String, PathBuf> = HashMap::new();
        let mut by_name: HashMap<String, PathBuf> = HashMap::new();

        for root in &roots {
            for db in candidates(root) {
                let key = std::fs::canonicalize(&db).unwrap_or_else(|_| db.clone());
                if !seen_paths.insert(key) {
                    continue;
                }
                let Some(folder) = read_identity(&db) else { continue };

                if let Some(kept) = by_uuid.get(&folder.uuid.to_lowercase()) {
                    out.blocked.push(Blocked {
                        db_path: db,
                        kept: kept.clone(),
                        reason: format!("Another copy of {}", folder.short_id()),
                    });
                    continue;
                }
                if let Some(kept) = by_name.get(&folder.name.to_lowercase()) {
                    out.blocked.push(Blocked {
                        db_path: db,
                        kept: kept.clone(),
                        reason: format!("The name \"{}\" is already in use", folder.name),
                    });
                    continue;
                }
                by_uuid.insert(folder.uuid.to_lowercase(), db.clone());
                by_name.insert(folder.name.to_lowercase(), db.clone());
                out.folders.push(folder);
            }
        }

        out.folders.sort_by_key(|f| f.name.to_lowercase());
        out
    }

    /// Find a folder by name (ignoring case), UUID, or the first eight
    /// characters of its UUID. A name or UUID that matches nothing is an error:
    /// a typo must never send a memory somewhere else.
    pub fn find(&self, query: &str) -> Result<Folder, String> {
        let scan = self.scan();
        let q = query.trim().to_lowercase();
        if let Some(f) = scan.folders.iter().find(|f| f.name.to_lowercase() == q) {
            return Ok(f.clone());
        }
        if let Some(f) = scan.folders.iter().find(|f| f.uuid.to_lowercase() == q) {
            return Ok(f.clone());
        }
        if q.len() == SHORT_ID {
            let hits: Vec<_> = scan
                .folders
                .iter()
                .filter(|f| f.uuid.to_lowercase().starts_with(&q))
                .collect();
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
    /// `GENIUZ_STATION` file path, a `.geniuz-folder` file here or in a parent
    /// directory, the current folder, then the first folder.
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
        if let Some(q) = self.current() {
            return self
                .find(&q)
                .map(|f| f.db_path)
                .map_err(|_| format!("The current folder ({}) isn't available. Choose one with `geniuz folder use`.", q));
        }
        self.ensure_default()
    }

    /// The folder chosen with `use`, if any.
    pub fn current(&self) -> Option<String> {
        std::fs::read_to_string(self.current_path())
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Make a folder the one used when none is named. Records its UUID, so
    /// renaming it changes nothing.
    pub fn use_folder(&self, query: &str) -> Result<Folder, String> {
        let folder = self.find(query)?;
        write_atomic(&self.current_path(), &format!("{}\n", folder.uuid))?;
        Ok(folder)
    }

    /// Create a folder, in the data directory unless `at` names another place.
    /// A new place goes on the allow list so the folder can be seen.
    pub fn create(&self, name: &str, description: &str, at: Option<&Path>) -> Result<Folder, String> {
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
            self.remember_added(parent)?;
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
        let dir = std::fs::canonicalize(dir)
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
        let mut names: HashMap<String, PathBuf> = existing
            .folders
            .iter()
            .map(|f| (f.name.to_lowercase(), f.db_path.clone()))
            .collect();

        // Check everything before writing anything.
        let mut to_stamp = Vec::new();
        let mut found = 0;
        for db in candidates(&dir) {
            if canonical(&db).starts_with(canonical(&self.home)) {
                continue;
            }
            let (uuid, name) = match read_identity(&db) {
                Some(f) => (Some(f.uuid), f.name),
                None if is_geniuz_store(&db) => {
                    if let Some(other) = stores_sharing_memories(&db, &existing.folders) {
                        return Err(format!(
                            "Not added. {} holds the same memories as the folder at {}. Copies aren't supported; move or retire one of them first.",
                            db.display(),
                            other.display()
                        ));
                    }
                    let name = directory_name(&db);
                    to_stamp.push((db.clone(), name.clone()));
                    (None, name)
                }
                None => continue,
            };
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
            if let Some(other) = names.insert(name.to_lowercase(), db.clone()) {
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

        for (db, name) in &to_stamp {
            stamp(db, name)?;
        }
        self.remember_added(&dir)?;

        let scan = self.scan();
        Ok(scan
            .folders
            .into_iter()
            .filter(|f| canonical(&f.db_path).starts_with(&dir))
            .collect())
    }

    /// Take a directory off the allow list. Its files aren't touched.
    pub fn remove(&self, dir: &Path) -> Result<(), String> {
        let target = canonical(dir);
        let current = self.added();
        let kept: Vec<_> = current.iter().filter(|d| canonical(d) != target).cloned().collect();
        if kept.len() == current.len() {
            return Err(format!("{} isn't on the list.", dir.display()));
        }
        self.write_added(&kept)
    }

    pub fn rename(&self, query: &str, new_name: &str) -> Result<Folder, String> {
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
    /// below where Geniuz reads. Nothing is deleted. To bring one back, add
    /// its directory or move it out again.
    pub fn archive(&self, query: &str) -> Result<PathBuf, String> {
        let folder = self.find(query)?;
        let dir = folder
            .db_path
            .parent()
            .ok_or_else(|| "That folder has no parent directory.".to_string())?
            .to_path_buf();
        if canonical(&dir) == canonical(&self.home) {
            return Err("Your first folder can't be archived, because other folders live beside it.".into());
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

        if self.current().map(|c| c.eq_ignore_ascii_case(&folder.uuid)).unwrap_or(false) {
            let _ = std::fs::rename(self.current_path(), self.home.join("current.archived"));
        }
        Ok(target)
    }

    /// Move a folder to another directory, keeping every UUID. Writers are held
    /// off for the whole move, and on another drive the new copy is verified
    /// before the original is retired. Across drives the original is moved to
    /// `archive/` beside itself and is returned, for the person to delete once
    /// they are satisfied; Geniuz never deletes it.
    pub fn relocate(&self, query: &str, dest: &Path) -> Result<Relocated, String> {
        let folder = self.find(query)?;
        let dir = folder
            .db_path
            .parent()
            .ok_or_else(|| "That folder has no parent directory.".to_string())?
            .to_path_buf();
        if canonical(&dir) == canonical(&self.home) {
            return Err("Your first folder stays where it is, because other folders live beside it.".into());
        }
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

        if self.added().iter().any(|d| canonical(d) == canonical(&dir)) || !dir.exists() {
            let keep: Vec<_> = self.added().into_iter().filter(|d| *d != dir).collect();
            self.write_added(&keep)?;
        }
        self.remember_added(&dest)?;
        Ok(Relocated { folder: self.find(&folder.uuid)?, left_behind })
    }

    fn check_name_free(&self, name: &str, except_uuid: Option<&str>) -> Result<(), String> {
        let lower = name.to_lowercase();
        let taken = self.scan().folders.iter().any(|f| {
            f.name.to_lowercase() == lower
                && except_uuid.map(|u| !f.uuid.eq_ignore_ascii_case(u)).unwrap_or(true)
        });
        if taken {
            return Err(format!("There's already a folder called \"{}\".", name));
        }
        Ok(())
    }

    fn remember_added(&self, dir: &Path) -> Result<(), String> {
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

    fn write_added(&self, dirs: &[PathBuf]) -> Result<(), String> {
        let body: String = dirs.iter().map(|d| format!("{}\n", d.display())).collect();
        write_atomic(&self.added_path(), &body)
    }
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
    pub entry: SignalEntry,
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
        let found = crate::db::DatabaseManager::new(&folder.db_path.to_string_lossy()).and_then(|db| {
            let entries = match search {
                Search::Recent => db.recent_in(window, limit),
                Search::Keyword(q) => db.keyword_search_in(q, window, limit),
                Search::Semantic(q, backend) => db.semantic_search_with(q, window, limit, *backend),
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
    Ok(name.to_string())
}

/// The databases directly in `root` and one level below it.
fn candidates(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for file in [DB_FILE, LEGACY_DB_FILE] {
        let db = root.join(file);
        if db.is_file() {
            out.push(db);
            break;
        }
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
        tx.execute(
            "INSERT OR IGNORE INTO _meta (key, value) VALUES (?1, ?2)",
            [format!("{}@{}", key, now()), old],
        )
        .map_err(|e| format!("Couldn't save {}: {}", key, e))?;
    }
    set_meta(&tx, key, value)?;
    tx.commit().map_err(|e| format!("Couldn't save {}: {}", key, e))
}

/// Read a file's identity. `None` means it isn't a folder, or can't be read.
fn read_identity(db: &Path) -> Option<Folder> {
    let conn = open_read_only(db).ok()?;
    let uuid = get_meta(&conn, KEY_UUID)?;
    Some(Folder {
        name: get_meta(&conn, KEY_NAME).unwrap_or_else(|| uuid.chars().take(SHORT_ID).collect()),
        description: get_meta(&conn, KEY_DESCRIPTION).unwrap_or_default(),
        created: get_meta(&conn, KEY_CREATED).unwrap_or_default(),
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

/// The first folder holding any of this store's oldest memories. Every copy
/// shares the original's oldest rows, whenever it was taken, so a small sample
/// finds one without reading whole stores.
fn stores_sharing_memories(db: &Path, folders: &[Folder]) -> Option<PathBuf> {
    let conn = open_read_only(db).ok()?;
    let mut stmt = conn
        .prepare("SELECT memory_uuid FROM memories ORDER BY created_at ASC LIMIT 20")
        .ok()?;
    let sample: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .ok()?
        .flatten()
        .collect();
    if sample.is_empty() {
        return None;
    }
    folders.iter().find_map(|f| {
        let other = open_read_only(&f.db_path).ok()?;
        sample
            .iter()
            .any(|uuid| {
                other
                    .query_row("SELECT 1 FROM memories WHERE memory_uuid = ?1", [uuid], |_| Ok(()))
                    .optional()
                    .ok()
                    .flatten()
                    .is_some()
            })
            .then(|| f.db_path.clone())
    })
}

/// The UUID for a store getting its first identity. It comes from the store's
/// oldest memory, so if a 4.x file was copied to another machine and both copies
/// upgrade, they get the same UUID rather than giving one set of memories two
/// identities. Only an empty store, with nothing to duplicate, gets a random one.
fn identity_for(conn: &Connection) -> String {
    let oldest: Option<String> = conn
        .query_row(
            "SELECT memory_uuid FROM memories ORDER BY created_at ASC, memory_uuid ASC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten();
    let id = match oldest {
        Some(memory) => {
            let name = format!("geniuz.folder:{}", memory.to_lowercase());
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, name.as_bytes())
        }
        None => uuid::Uuid::new_v4(),
    };
    id.to_string().to_uppercase()
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

/// Move across drives: copy into a hidden directory, verify, put it in place,
/// then retire the original to `archive/`. Writers stay locked out throughout.
fn copy_then_retire(dir: &Path, dest: &Path, folder: &Folder, lock: Connection) -> Result<PathBuf, String> {
    let expected: i64 = lock
        .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
        .map_err(|e| format!("Couldn't read {}: {}", dir.display(), e))?;

    let staging = dest.join(format!(".moving-{}", folder.uuid));
    if staging.exists() {
        return Err(format!("An earlier move left {} behind. Check it, then move it aside.", staging.display()));
    }
    std::fs::create_dir_all(&staging)
        .map_err(|e| format!("Couldn't create {}: {}", staging.display(), e))?;
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
        std::fs::copy(&path, staging.join(entry.file_name()))
            .map_err(|e| format!("Couldn't copy {}: {}", path.display(), e))?;
    }

    verify_copy(&staging.join(DB_FILE), folder, expected)
        .map_err(|e| format!("{} The original is untouched; the partial copy is at {}.", e, staging.display()))?;

    let target = dest.join(&folder.uuid);
    std::fs::rename(&staging, &target)
        .map_err(|e| format!("Couldn't finish the move to {}: {}", target.display(), e))?;

    let parent = dir.parent().ok_or_else(|| "That folder has no parent directory.".to_string())?;
    let archive = parent.join(ARCHIVE_DIR);
    std::fs::create_dir_all(&archive)
        .map_err(|e| format!("Couldn't create {}: {}", archive.display(), e))?;
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let retired = archive.join(format!("{}.moved-{}", folder.uuid, stamp));
    if std::fs::rename(dir, &retired).is_err() {
        // Windows again: release the file, then move the directory.
        drop(lock);
        std::fs::rename(dir, &retired)
            .map_err(|e| format!("The folder moved, but {} couldn't be retired: {}. Move it aside.", dir.display(), e))?;
    }
    Ok(retired)
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
    let uuid = identity_for(&conn);
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
        .unwrap_or_else(|| "Added folder".to_string())
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn write_atomic(path: &Path, body: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Couldn't create {}: {}", parent.display(), e))?;
    }
    let tmp = path.with_extension("tmp");
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
    fn older_station_db_is_still_found() {
        let (_tmp, reg) = setup();
        crate::db::DatabaseManager::new(&reg.home.join(LEGACY_DB_FILE).to_string_lossy()).unwrap();
        assert_eq!(reg.ensure_default().unwrap().file_name().unwrap(), LEGACY_DB_FILE);
        assert!(!reg.home.join(DB_FILE).exists());
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
        let at = tmp.path().join("LIMEN");
        let f = reg.create("Limen", "", Some(&at)).unwrap();
        assert!(f.db_path.starts_with(&at));
        assert_eq!(reg.added(), vec![canonical(&at)]);
        assert_eq!(reg.find("limen").unwrap().uuid, f.uuid);
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
        assert!(err.contains("same memories"), "{}", err);
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
        open(&a).signal("the older note", Some("older"), None, Some("2026-01-01 00:00:00")).unwrap();
        open(&b).signal("the newer note", Some("newer"), None, Some("2026-02-01 00:00:00")).unwrap();
        open(&b).signal("unrelated", Some("other"), None, Some("2026-03-01 00:00:00")).unwrap();

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
    fn a_copy_appearing_later_is_blocked_and_the_original_keeps_working() {
        let (tmp, reg) = setup();
        let first = tmp.path().join("first");
        let f = reg.create("Work", "", Some(&first)).unwrap();
        let second = tmp.path().join("second");
        reg.create("Other", "", Some(&second)).unwrap();
        copy_dir(f.db_path.parent().unwrap(), &second.join("dropped-in"));

        let scan = reg.scan();
        assert_eq!(scan.blocked.len(), 1);
        assert_eq!(canonical(&scan.blocked[0].kept), canonical(&f.db_path));
        assert_eq!(canonical(&reg.find("Work").unwrap().db_path), canonical(&f.db_path));
        assert_eq!(scan.folders.iter().filter(|x| x.uuid == f.uuid).count(), 1);
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
        reg.use_folder("Scratch").unwrap();

        let moved = reg.archive("Scratch").unwrap();
        assert!(moved.join(DB_FILE).is_file());
        assert!(!f.db_path.exists());
        assert!(reg.find("Scratch").is_err());
        assert_eq!(reg.resolve(None, None, None).unwrap().parent().unwrap(), reg.home);
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

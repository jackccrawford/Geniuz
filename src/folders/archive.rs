//! Archiving a folder and restoring it: a move into `archive/` beside it and back.

use super::*;

impl Registry {
    /// Retire a folder by moving it into `archive/` beside itself, one level
    /// below where Geniuz reads. Nothing is deleted, and `restore` brings it back.
    pub fn archive(&self, query: &str) -> Result<PathBuf, String> {
        self.with_registry_lock(|| self.archive_unlocked(query))
    }

    pub(super) fn archive_unlocked(&self, query: &str) -> Result<PathBuf, String> {
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

    pub(super) fn restore_unlocked(&self, query: &str) -> Result<Folder, String> {
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
}

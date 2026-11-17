//! Keeps what Geniuz writes private to the person it belongs to.
//!
//! Until 6.0 the data directory was created with the system's defaults, so on a computer
//! with several users any of them could read every note (found in the 5.1.4 security
//! review, 16 Nov 2026). Two things fix it:
//!
//! 1. Every Geniuz program starts by asking the system to create its files and
//!    directories readable by their owner alone. That covers what is listed here and
//!    whatever is added later, including the files SQLite makes beside a database.
//! 2. The data directory itself is checked at each start and closed if it is open.
//!    Once a directory can't be entered by others, nothing inside it can be reached.
//!
//! On Windows the user's profile is private already, and nothing here applies.

use std::path::Path;

/// What the check of the data directory found.
#[derive(Debug, PartialEq)]
pub enum Found {
    /// Only its owner can enter it, or it doesn't exist yet.
    Private,
    /// It was open to others and has been closed; this many files and directories changed.
    Tightened(usize),
    /// It is open to others and was left that way, because the person chose this
    /// directory with GENIUZ_HOME and may share it on purpose. The message says so.
    OpenByChoice(String),
}

/// Files and directories this program creates from here on belong to their owner alone.
/// Call it first in every Geniuz program.
pub fn by_default() {
    #[cfg(unix)]
    unsafe {
        libc::umask(0o077);
    }
}

/// Check the data directory, and close it if it is the default one and open to others.
pub fn data_dir() -> Found {
    let dir = crate::data_dir();
    let chosen = std::env::var_os("GENIUZ_HOME").is_some() && dir != crate::home_dir().join(".geniuz");
    check(&dir, chosen)
}

#[cfg(unix)]
fn check(dir: &Path, chosen: bool) -> Found {
    use std::os::unix::fs::PermissionsExt;
    // lstat, not stat: a link put where the directory should be is not followed.
    let Ok(meta) = std::fs::symlink_metadata(dir) else { return Found::Private };
    if !meta.is_dir() || meta.permissions().mode() & 0o077 == 0 {
        return Found::Private;
    }
    if chosen {
        return Found::OpenByChoice(format!(
            "{} can be read by other users of this computer. Geniuz leaves a directory you chose as it is; `chmod 700` closes it.",
            dir.display()
        ));
    }
    Found::Tightened(tighten(dir))
}

#[cfg(not(unix))]
fn check(_dir: &Path, _chosen: bool) -> Found {
    Found::Private
}

/// Close a directory tree: directories to their owner alone (700), files likewise (600).
/// Links are left alone and never followed. Returns how many entries changed. An entry
/// that can't be changed (not ours, say) is skipped: the top directory is what matters.
#[cfg(unix)]
fn tighten(dir: &Path) -> usize {
    use std::os::unix::fs::PermissionsExt;
    let mut changed = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if meta.file_type().is_symlink() {
            continue;
        }
        let want = if meta.is_dir() { 0o700 } else { 0o600 };
        if meta.permissions().mode() & 0o777 != want
            && std::fs::set_permissions(&path, std::fs::Permissions::from_mode(want)).is_ok()
        {
            changed += 1;
        }
        if meta.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                stack.extend(entries.flatten().map(|e| e.path()));
            }
        }
    }
    changed
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode(p: &Path) -> u32 {
        std::fs::symlink_metadata(p).unwrap().permissions().mode() & 0o777
    }

    fn tree(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("geniuz-private-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("ABCD1234")).unwrap();
        std::fs::write(dir.join("ABCD1234/memory.db"), "x").unwrap();
        std::fs::write(dir.join("ABCD1234/memory.db-wal"), "x").unwrap();
        std::fs::write(dir.join("settings.json"), "{}").unwrap();
        for p in [dir.clone(), dir.join("ABCD1234")] {
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        for p in ["ABCD1234/memory.db", "ABCD1234/memory.db-wal", "settings.json"] {
            std::fs::set_permissions(dir.join(p), std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        dir
    }

    #[test]
    fn an_open_default_directory_is_closed_all_the_way_down() {
        let dir = tree("default");
        assert_eq!(check(&dir, false), Found::Tightened(5));
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("ABCD1234")), 0o700);
        assert_eq!(mode(&dir.join("ABCD1234/memory.db")), 0o600);
        assert_eq!(mode(&dir.join("ABCD1234/memory.db-wal")), 0o600, "SQLite's side files too");
        assert_eq!(check(&dir, false), Found::Private, "and a second look changes nothing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_directory_the_person_chose_is_reported_and_left_alone() {
        let dir = tree("chosen");
        match check(&dir, true) {
            Found::OpenByChoice(msg) => assert!(msg.contains("chmod 700"), "{}", msg),
            other => panic!("expected OpenByChoice, got {:?}", other),
        }
        assert_eq!(mode(&dir), 0o755, "it may be shared on purpose");
        assert_eq!(mode(&dir.join("settings.json")), 0o644);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_link_is_neither_followed_nor_changed() {
        let dir = tree("link");
        let outside = std::env::temp_dir().join(format!("geniuz-private-outside-{}", std::process::id()));
        std::fs::write(&outside, "someone else's").unwrap();
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("link")).unwrap();
        check(&dir, false);
        assert_eq!(mode(&outside), 0o644, "a file outside the directory is not touched through a link");
        let _ = std::fs::remove_file(&outside);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_directory_that_is_not_there_yet_is_fine() {
        assert_eq!(check(Path::new("/nonexistent/geniuz-private-test"), false), Found::Private);
    }
}

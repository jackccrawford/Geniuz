//! Where the embed helper listens, and who may talk to it.
//!
//! The helper (`geniuz-embed`) keeps the search model loaded so each keep and recall
//! doesn't pay to load it. Programs send it the text of notes and searches. Until 6.0 it
//! listened at one fixed name shared by every user of the computer, and the client sent
//! text to whatever answered there, so another local user could put a listener at that
//! name and read it (found by Droplet in the 5.1.4 security review, 16 Nov 2026).
//!
//! On Mac and Linux the helper now listens on a socket file inside a directory that only
//! this user can enter, and both ends check the other is the same user:
//!
//! - the directory is `$XDG_RUNTIME_DIR/geniuz` where the system provides one (Linux:
//!   per-user, in memory, gone at logout), otherwise `<data dir>/run`;
//! - it is checked on every connect and every bind: a real directory (not a link), owned
//!   by this user, closed to everyone else;
//! - the client asks the system who is listening before sending any text, and the server
//!   asks who connected before reading any.
//!
//! If no such place can be had (the path is too long for a socket, say), there is no
//! helper: the model is loaded in-process, which is slower and safe.
//!
//! Windows still uses a named pipe at a fixed name. Restricting it to its owner needs a
//! security descriptor and a check of the serving process, to be built and tested on a
//! Windows machine; until then this limitation remains there.

use interprocess::local_socket::{prelude::*, Listener, ListenerOptions, Stream};
use std::path::PathBuf;

const FILE_NAME: &str = "embed.sock";

/// A socket's path must fit the system's limit (104 bytes on macOS, 108 on Linux).
#[cfg(unix)]
const MAX_SOCKET_PATH: usize = 100;

/// The name within the directory. `GENIUZ_EMBED_SOCKET` may choose it (tests run several
/// helpers side by side), but only a bare file name: it can never point elsewhere.
fn file_name() -> String {
    match std::env::var("GENIUZ_EMBED_SOCKET") {
        Ok(name) if !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".." => name,
        _ => FILE_NAME.to_string(),
    }
}

#[cfg(unix)]
mod place {
    use super::*;
    use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
    use std::path::Path;

    fn me() -> u32 {
        unsafe { libc::geteuid() }
    }

    /// A real directory (not a link), ours, and closed to everyone else.
    pub(super) fn is_private_dir(dir: &Path) -> bool {
        match std::fs::symlink_metadata(dir) {
            Ok(m) => m.is_dir() && m.uid() == me() && m.permissions().mode() & 0o077 == 0,
            Err(_) => false,
        }
    }

    /// Make the directory if it is missing, close it if it is ours and open, then check it.
    fn private_dir(dir: &Path) -> bool {
        if std::fs::symlink_metadata(dir).is_err() {
            let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir);
        }
        if let Ok(m) = std::fs::symlink_metadata(dir) {
            if m.is_dir() && m.uid() == me() && m.permissions().mode() & 0o077 != 0 {
                let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
            }
        }
        is_private_dir(dir)
    }

    /// The directory to use: the system's per-user runtime directory when it has one and
    /// it is private, otherwise `run` inside the data directory.
    fn dir() -> Option<PathBuf> {
        if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
            if runtime.is_absolute() && is_private_dir(&runtime) {
                let d = runtime.join("geniuz");
                if private_dir(&d) {
                    return Some(d);
                }
            }
        }
        let d = crate::data_dir().join("run");
        private_dir(&d).then_some(d)
    }

    /// The socket's path, or None when there is no safe place for one.
    pub(super) fn path() -> Option<PathBuf> {
        let p = dir()?.join(file_name());
        (p.as_os_str().len() <= MAX_SOCKET_PATH).then_some(p)
    }

    /// Whether the other end of this connection is the same user.
    pub(super) fn same_user(stream: &Stream) -> bool {
        matches!(stream.peer_creds().ok().and_then(|c| c.euid()), Some(uid) if uid == me())
    }

    /// Remove a socket left by a helper that is gone, but only a socket, and only ours.
    pub(super) fn clear_stale(path: &Path) {
        if let Ok(m) = std::fs::symlink_metadata(path) {
            if m.file_type().is_socket() && m.uid() == me() {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// Connect to the helper, if one is listening where only this user's helper can be.
/// On Mac and Linux, a listener that is not this user is refused before any text is sent.
pub fn connect() -> Option<Stream> {
    #[cfg(unix)]
    {
        use interprocess::local_socket::GenericFilePath;
        let path = place::path()?;
        let stream = Stream::connect(path.as_os_str().to_fs_name::<GenericFilePath>().ok()?).ok()?;
        place::same_user(&stream).then_some(stream)
    }
    #[cfg(not(unix))]
    {
        use interprocess::local_socket::GenericNamespaced;
        let name = format!("geniuz-{}", file_name());
        Stream::connect(name.as_str().to_ns_name::<GenericNamespaced>().ok()?).ok()
    }
}

/// Start listening. Err says why not, in words fit for the helper's log.
pub fn listen() -> Result<Listener, String> {
    #[cfg(unix)]
    {
        use interprocess::local_socket::GenericFilePath;
        use std::os::unix::fs::PermissionsExt;
        let path = place::path().ok_or("no private place for the socket (the path may be too long)")?;
        place::clear_stale(&path);
        let name = path.as_os_str().to_fs_name::<GenericFilePath>().map_err(|e| e.to_string())?;
        // The socket must never be open to others, even for the moment between its
        // creation and a chmod: so it is created under a umask that allows only its owner.
        // (The library's own `mode` option changes the mode through the open socket, which
        // macOS doesn't support.)
        let before = unsafe { libc::umask(0o077) };
        let made = ListenerOptions::new().name(name).create_sync();
        unsafe { libc::umask(before) };
        let listener = made.map_err(|e| format!("{}: {}", path.display(), e))?;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        Ok(listener)
    }
    #[cfg(not(unix))]
    {
        use interprocess::local_socket::GenericNamespaced;
        let name = format!("geniuz-{}", file_name());
        let ns = name.as_str().to_ns_name::<GenericNamespaced>().map_err(|e| e.to_string())?;
        ListenerOptions::new().name(ns).create_sync().map_err(|e| e.to_string())
    }
}

/// Whether whoever connected to the helper may be served: on Mac and Linux, only this user.
pub fn may_serve(stream: &Stream) -> bool {
    #[cfg(unix)]
    {
        place::same_user(stream)
    }
    #[cfg(not(unix))]
    {
        let _ = stream;
        true
    }
}

/// Where the helper listens, for its log. None when there is no safe place.
pub fn describe() -> Option<String> {
    #[cfg(unix)]
    {
        place::path().map(|p| p.display().to_string())
    }
    #[cfg(not(unix))]
    {
        Some(format!(r"\\.\pipe\geniuz-{}", file_name()))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::place::is_private_dir;
    use std::os::unix::fs::PermissionsExt;

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("geniuz-sock-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn only_a_closed_real_directory_of_ours_will_do() {
        let d = scratch("dir");
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(is_private_dir(&d));

        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o750)).unwrap();
        assert!(!is_private_dir(&d), "a group that can enter it is one too many");

        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700)).unwrap();
        let link = d.with_extension("link");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&d, &link).unwrap();
        assert!(!is_private_dir(&link), "a link to a private directory is not one");

        assert!(!is_private_dir(&d.join("missing")));
        let _ = std::fs::remove_file(&link);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_name_can_be_chosen_but_never_a_path() {
        // file_name() reads the environment, which tests share, so the rule is checked directly.
        let ok = |name: &str| !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != "..";
        assert!(ok("limen-test.sock"));
        for bad in ["/tmp/evil.sock", "../evil.sock", "a/b", r"a\b", "..", ".", ""] {
            assert!(!ok(bad), "{:?} must not be accepted", bad);
        }
    }
}

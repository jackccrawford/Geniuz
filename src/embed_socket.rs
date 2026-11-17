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
//! The second of those is the guarantee. The directory check keeps strangers out in the
//! ordinary case, but a directory could in principle be swapped between the check and the
//! connect; asking the system who is at the other end cannot be got around that way. Never
//! remove the same-user check on the strength of the directory check.
//!
//! If no such place can be had (the path is too long for a socket, say), there is no
//! helper: the model is loaded in-process, which is slower and safe.
//!
//! On Windows the helper listens on a named pipe, and the same two things hold by other
//! means (written by SSH on orbit, 16 Nov 2026):
//!
//! - the pipe is created as the first instance of its name, so creation fails if anyone
//!   already holds the name; it refuses remote clients; and its access list names this
//!   user and nobody else, so the system itself turns every other user away when they
//!   try to open it. Unlike the directory on Mac and Linux, that check cannot be raced:
//!   the system makes it on the pipe itself at every open;
//! - the client asks the system who OWNS the pipe it has just opened, through the handle
//!   it holds, before sending any text. A pipe someone else created first is owned by
//!   them, whatever access list they put on it, and is refused. This is the guarantee on
//!   the client side. It also checks the serving process's user, and the server checks the
//!   calling process's user, as a second line: those go by process id, which the system
//!   can reuse, so they are not relied on alone.
//!
//! The pipe's name carries the user's identifier, so two users' helpers don't collide and
//! one user can't keep another from having a helper by taking a shared name. The name is
//! not the protection: it can be guessed.

use interprocess::local_socket::{prelude::*, Listener, ListenerOptions, Stream};
#[cfg(unix)]
use std::path::PathBuf;

const FILE_NAME: &str = "embed.sock";

/// A socket's path must fit the system's limit (104 bytes on macOS, 108 on Linux).
#[cfg(unix)]
const MAX_SOCKET_PATH: usize = 100;

/// The name within the directory. `GENIUZ_EMBED_SOCKET` may choose it (tests run several
/// helpers side by side), but only a bare file name: it can never point elsewhere.
fn file_name() -> String {
    match std::env::var("GENIUZ_EMBED_SOCKET") {
        Ok(name) if valid_file_name(&name) => name,
        _ => FILE_NAME.to_string(),
    }
}

/// A bare file name: not empty, no separators, no NUL, and not a way up or out.
fn valid_file_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\', '\0']) && name != "." && name != ".."
}

#[cfg(windows)]
mod pipe {
    use super::*;
    use interprocess::local_socket::{ConnectOptions, GenericNamespaced};
    use interprocess::os::windows::local_socket::ListenerOptionsExt;
    use interprocess::os::windows::security_descriptor::SecurityDescriptor;
    use interprocess::ConnectWaitMode;
    use std::os::windows::io::{AsHandle, AsRawHandle};
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, PSID};
    use windows_sys::Win32::Security::Authorization::{ConvertSidToStringSidW, GetSecurityInfo, SE_KERNEL_OBJECT};
    use windows_sys::Win32::Security::{
        EqualSid, GetTokenInformation, IsValidSid, TokenUser, OWNER_SECURITY_INFORMATION, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// How long a client waits for a busy pipe. Without a limit, someone holding the name
    /// with every instance busy could keep a keep or a recall waiting for ever.
    const CONNECT_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

    /// A user's identifier (SID), as the system handed it over in a TOKEN_USER block.
    /// The block is kept whole because the SID pointer inside it points into the block.
    pub(super) struct User {
        block: Vec<u64>, // u64 so the block is aligned for TOKEN_USER
    }

    impl User {
        fn sid(&self) -> PSID {
            // SAFETY: `block` was filled by GetTokenInformation(TokenUser) and is not moved
            // or changed afterwards (a Vec's heap block stays put when the Vec is moved).
            unsafe { (*(self.block.as_ptr() as *const TOKEN_USER)).User.Sid }
        }

        /// The user a process token belongs to. Closes nothing: the caller owns `token`.
        fn of_token(token: HANDLE) -> Option<User> {
            let mut needed = 0u32;
            // The first call only asks how big the block must be; it is expected to fail.
            unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed) };
            if needed == 0 || needed > 4096 {
                return None;
            }
            let mut block = vec![0u64; (needed as usize + 7) / 8];
            let ok = unsafe {
                GetTokenInformation(token, TokenUser, block.as_mut_ptr().cast(), needed, &mut needed)
            };
            if ok == 0 {
                return None;
            }
            let user = User { block };
            (unsafe { IsValidSid(user.sid()) } != 0).then_some(user)
        }

        /// The user a process runs as. None if the process is gone or may not be asked.
        fn of_process(process: HANDLE) -> Option<User> {
            let mut token: HANDLE = 0 as HANDLE;
            if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
                return None;
            }
            let user = User::of_token(token);
            unsafe { CloseHandle(token) };
            user
        }

        fn of_pid(pid: u32) -> Option<User> {
            let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
            if process as isize == 0 {
                return None;
            }
            let user = User::of_process(process);
            unsafe { CloseHandle(process) };
            user
        }

        fn is(&self, sid: PSID) -> bool {
            !sid.is_null() && unsafe { IsValidSid(sid) != 0 && EqualSid(self.sid(), sid) != 0 }
        }

        /// The identifier as text, such as S-1-5-21-…-1001.
        fn text(&self) -> Option<String> {
            let mut wide: *mut u16 = std::ptr::null_mut();
            if unsafe { ConvertSidToStringSidW(self.sid(), &mut wide) } == 0 || wide.is_null() {
                return None;
            }
            // SAFETY: on success `wide` is a NUL-terminated string the system allocated.
            let text = unsafe {
                let mut len = 0;
                while *wide.add(len) != 0 {
                    len += 1;
                }
                String::from_utf16(std::slice::from_raw_parts(wide, len)).ok()
            };
            unsafe { LocalFree(wide as _) };
            text.filter(|t| t.starts_with("S-1-") && t.bytes().all(|b| b.is_ascii_digit() || b == b'S' || b == b'-'))
        }
    }

    /// This process's own user, asked once.
    pub(super) fn me() -> Option<&'static User> {
        static ME: OnceLock<Option<User>> = OnceLock::new();
        // GetCurrentProcess gives a pseudo-handle that needs no closing.
        ME.get_or_init(|| User::of_process(unsafe { GetCurrentProcess() })).as_ref()
    }

    /// The pipe's name for this user, without the `\\.\pipe\` in front.
    pub(super) fn name(file: &str) -> Option<String> {
        Some(format!("geniuz-{}-{}", me()?.text()?, file))
    }

    /// The raw handle of a connection, client or server end.
    fn handle(stream: &Stream) -> Option<HANDLE> {
        #[allow(unreachable_patterns)]
        match stream {
            Stream::NamedPipe(s) => Some(s.inner().as_handle().as_raw_handle() as HANDLE),
            _ => None,
        }
    }

    /// Whether the pipe at the other end of this handle is owned by this user. The system
    /// answers from the pipe object the handle is open on, so this cannot be raced, and a
    /// pipe's creator cannot name someone else as its owner.
    fn owned_by_me(stream: &Stream) -> bool {
        let (Some(me), Some(h)) = (me(), handle(stream)) else { return false };
        let mut owner: PSID = std::ptr::null_mut();
        let mut descriptor = std::ptr::null_mut();
        let failed = unsafe {
            GetSecurityInfo(
                h,
                SE_KERNEL_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &mut owner,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut descriptor,
            )
        };
        if failed != 0 {
            return false;
        }
        let mine = me.is(owner); // `owner` points into `descriptor`, so compare before freeing
        if !descriptor.is_null() {
            unsafe { LocalFree(descriptor as _) };
        }
        mine
    }

    /// Whether the process at the other end runs as this user. By process id, so a second
    /// line only: see the module comment.
    fn peer_is_me(stream: &Stream) -> bool {
        let Some(me) = me() else { return false };
        let Some(pid) = stream.peer_creds().ok().and_then(|c| c.pid()) else { return false };
        matches!(User::of_pid(pid), Some(peer) if me.is(peer.sid()))
    }

    /// Client side: the helper we reached is this user's own. Checked before any text is sent.
    pub(super) fn server_is_me(stream: &Stream) -> bool {
        owned_by_me(stream) && peer_is_me(stream)
    }

    /// Server side: the caller is this user. The pipe's access list has already turned
    /// everyone else away; this is the second line.
    pub(super) fn client_is_me(stream: &Stream) -> bool {
        peer_is_me(stream)
    }

    pub(super) fn connect(file: &str) -> Option<Stream> {
        let name = name(file)?;
        let stream = ConnectOptions::new()
            .name(name.as_str().to_ns_name::<GenericNamespaced>().ok()?)
            .wait_mode(ConnectWaitMode::Timeout(CONNECT_WAIT))
            .connect_sync()
            .ok()?;
        server_is_me(&stream).then_some(stream)
    }

    /// Owner: this user. Access: this user, everything; nobody else, nothing. `P` stops any
    /// inherited entries being added.
    /// The owner is named outright because the system's default is not always the user: a
    /// process running elevated (as over ssh) gets the Administrators group as owner, and
    /// the client's owner check would then refuse this user's own helper.
    fn descriptor() -> Result<SecurityDescriptor, String> {
        let sid = me().and_then(User::text).ok_or("this user's identifier could not be read")?;
        let sddl = widestring::U16CString::from_str(format!("O:{sid}D:P(A;;GA;;;{sid})")).map_err(|e| e.to_string())?;
        SecurityDescriptor::deserialize(&sddl).map_err(|e| format!("security descriptor: {e}"))
    }

    pub(super) fn listen(file: &str) -> Result<Listener, String> {
        let name = name(file).ok_or("this user's identifier could not be read")?;
        let ns = name.as_str().to_ns_name::<GenericNamespaced>().map_err(|e| e.to_string())?;
        // The library creates the first instance with FILE_FLAG_FIRST_PIPE_INSTANCE and with
        // PIPE_REJECT_REMOTE_CLIENTS (interprocess 2.4.0, named_pipe/listener/create_instance.rs),
        // so this fails if the name is already held, by anyone.
        match ListenerOptions::new().name(ns).security_descriptor(descriptor()?).create_sync() {
            Ok(listener) => Ok(listener),
            // Held already. If it is our own helper, say so; if not, there is no helper.
            Err(_) if connect(file).is_some() => Err(ALREADY_RUNNING.to_string()),
            Err(e) => Err(format!(r"\\.\pipe\{name}: {e}")),
        }
    }

    #[cfg(test)]
    pub(super) mod probe {
        //! What the tests need to see of the pieces above.
        use super::*;
        use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;

        pub fn my_text() -> Option<String> { me().and_then(User::text) }
        pub fn pid_is_me(pid: u32) -> bool { matches!((me(), User::of_pid(pid)), (Some(a), Some(b)) if a.is(b.sid())) }
        /// Whether this user is the one a SID string names.
        pub fn i_am(sid: &str) -> bool {
            let wide = widestring::U16CString::from_str(sid).unwrap();
            let mut p: PSID = std::ptr::null_mut();
            if unsafe { ConvertStringSidToSidW(wide.as_ptr(), &mut p) } == 0 { return false; }
            let same = me().is_some_and(|m| m.is(p));
            unsafe { LocalFree(p as _) };
            same
        }
        pub fn owned_by_me(stream: &Stream) -> bool { super::owned_by_me(stream) }
        pub fn peer_is_me(stream: &Stream) -> bool { super::peer_is_me(stream) }
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

    /// Whether `path` is a socket of ours (and not a link to something else).
    pub(super) fn is_our_socket(path: &Path) -> bool {
        matches!(std::fs::symlink_metadata(path), Ok(m) if m.file_type().is_socket() && m.uid() == me())
    }

    /// Remove a socket left by a helper that is gone, but only a socket, and only ours.
    /// Call it only when nothing answers there: a live helper's socket must not be removed.
    pub(super) fn clear_stale(path: &Path) {
        if is_our_socket(path) {
            let _ = std::fs::remove_file(path);
        }
    }

    /// Held while a helper decides whether to start: two helpers launched at the same
    /// moment would otherwise both find nobody listening, and the second would remove the
    /// first one's socket. The lock is released when the returned file is dropped.
    pub(super) fn start_lock(socket: &Path) -> Option<std::fs::File> {
        use std::os::unix::io::AsRawFd;
        let file = std::fs::OpenOptions::new().create(true).write(true).truncate(false)
            .open(socket.with_extension("lock")).ok()?;
        (unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0).then_some(file)
    }
}

/// Connect to the helper, if one is listening where only this user's helper can be.
/// A listener that is not this user's is refused before any text is sent.
pub fn connect() -> Option<Stream> {
    #[cfg(unix)]
    {
        use interprocess::local_socket::GenericFilePath;
        let path = place::path()?;
        let stream = Stream::connect(path.as_os_str().to_fs_name::<GenericFilePath>().ok()?).ok()?;
        place::same_user(&stream).then_some(stream)
    }
    #[cfg(windows)]
    {
        pipe::connect(&file_name())
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// What `listen` answers when another helper got there first.
pub const ALREADY_RUNNING: &str = "already running";

/// Start listening. Err says why not, in words fit for the helper's log; it is
/// `ALREADY_RUNNING` when a helper of this user's is already listening.
pub fn listen() -> Result<Listener, String> {
    #[cfg(unix)]
    {
        use interprocess::local_socket::GenericFilePath;
        use std::os::unix::fs::PermissionsExt;
        let path = place::path().ok_or("no private place for the socket (the path may be too long)")?;
        // One helper at a time decides: look for a live one, clear a dead one's socket, bind.
        // Without the lock two helpers could each find nobody listening, and one would
        // remove the other's socket: so no lock, no helper.
        let _lock = place::start_lock(&path).ok_or("couldn't take the start lock in the socket's directory")?;
        if connect().is_some() {
            return Err(ALREADY_RUNNING.to_string());
        }
        place::clear_stale(&path);
        let name = path.as_os_str().to_fs_name::<GenericFilePath>().map_err(|e| e.to_string())?;
        // The socket must never be open to others, even for the moment between its
        // creation and a chmod: so it is created under a umask that allows only its owner.
        // (The library's own `mode` option changes the mode through the open socket, which
        // macOS doesn't support.) Geniuz's programs already run under this umask
        // (`private::by_default`); setting it here keeps this function safe on its own.
        // A umask belongs to the whole process, which is harmless here for that reason.
        let before = unsafe { libc::umask(0o077) };
        let made = ListenerOptions::new().name(name).create_sync();
        unsafe { libc::umask(before) };
        let listener = made.map_err(|e| format!("{}: {}", path.display(), e))?;
        if place::is_our_socket(&path) {
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(listener)
    }
    #[cfg(windows)]
    {
        pipe::listen(&file_name())
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err("no helper on this system".to_string())
    }
}

/// Whether whoever connected to the helper may be served: only this user.
pub fn may_serve(stream: &Stream) -> bool {
    #[cfg(unix)]
    {
        place::same_user(stream)
    }
    #[cfg(windows)]
    {
        pipe::client_is_me(stream)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = stream;
        false
    }
}

/// Where the helper listens, for its log. None when there is no safe place.
pub fn describe() -> Option<String> {
    #[cfg(unix)]
    {
        place::path().map(|p| p.display().to_string())
    }
    #[cfg(windows)]
    {
        pipe::name(&file_name()).map(|n| format!(r"\\.\pipe\{n}"))
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
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
        // file_name() reads the environment, which tests share; its rule is the function below.
        let ok = super::valid_file_name;
        assert!(ok("limen-test.sock"));
        for bad in ["/tmp/evil.sock", "../evil.sock", "a/b", r"a\b", "..", ".", "", "a\0b"] {
            assert!(!ok(bad), "{:?} must not be accepted", bad);
        }
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::pipe::{self, probe};
    use std::io::{Read, Write};

    /// A pipe name no other test or helper uses.
    fn unique(tag: &str) -> String {
        format!("test-{}-{}.sock", tag, std::process::id())
    }

    #[test]
    fn this_user_can_be_read_and_recognised() {
        let sid = probe::my_text().expect("this user's identifier");
        assert!(sid.starts_with("S-1-"), "{sid}");
        assert!(probe::i_am(&sid));
        assert!(!probe::i_am("S-1-1-0"), "Everyone is not this user");
        assert!(!probe::i_am("S-1-5-18"), "nor is the system account, unless the tests run as it");
        assert!(probe::pid_is_me(std::process::id()));
        assert!(pipe::name("embed.sock").unwrap().contains(&sid));
    }

    #[test]
    fn our_own_pipe_is_accepted_both_ways_and_carries_text() {
        let file = unique("ok");
        let listener = pipe::listen(&file).expect("listen");
        let server = std::thread::spawn(move || {
            use interprocess::local_socket::traits::Listener as _;
            let mut stream = listener.accept().expect("accept");
            let served = pipe::client_is_me(&stream) && probe::peer_is_me(&stream);
            let mut got = [0u8; 5];
            stream.read_exact(&mut got).expect("read");
            stream.write_all(b"world").expect("write");
            (served, got)
        });
        let mut client = pipe::connect(&file).expect("the client accepts this user's own pipe");
        assert!(probe::owned_by_me(&client), "the pipe's owner is this user");
        client.write_all(b"hello").unwrap();
        let mut back = [0u8; 5];
        client.read_exact(&mut back).unwrap();
        let (served, got) = server.join().unwrap();
        assert!(served, "the server recognises the caller as this user");
        assert_eq!(&got, b"hello");
        assert_eq!(&back, b"world");
    }

    #[test]
    fn a_name_already_held_cannot_be_taken() {
        let file = unique("held");
        let _first = pipe::listen(&file).expect("the first listener");
        // The second is told a helper of ours is there (the first one answers the check).
        assert_eq!(pipe::listen(&file).err().as_deref(), Some(super::ALREADY_RUNNING));
    }

    #[test]
    fn nobody_listening_means_no_connection() {
        let started = std::time::Instant::now();
        assert!(pipe::connect(&unique("nobody")).is_none());
        assert!(started.elapsed() < std::time::Duration::from_secs(5), "and it does not wait long to say so");
    }
}

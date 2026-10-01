pub mod embedding;
pub mod embed_client;
pub mod db;
pub mod settings;
pub mod window;
pub mod dig;

use std::path::PathBuf;

/// User's home directory, cross-platform.
/// On Windows this is %USERPROFILE% (HOME is not standard there).
pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Resolve the Geniuz data directory. Precedence:
///   1. GENIUZ_HOME env var (set by the installer's directory picker,
///      and propagated to the MCP subprocess via the Claude Desktop config
///      entry's env block — critical on Windows where HOME is not set).
///   2. ~/.geniuz on every platform.
///
/// The data directory holds memory.db, the embedding model cache, and any
/// other per-user state. Both the binary and the library (e.g. the
/// embedding backend looking for its model file) must agree on this path,
/// which is why it lives at library scope — the MCP subprocess inherits
/// a different environment from the user's interactive shell, and having
/// two path resolvers with different env-var precedence produced a real
/// bug (v1.1.6 and prior: embedding backend read `HOME` directly, missed
/// `GENIUZ_HOME`, tried to `create_dir_all` in the MSIX-sandboxed CWD,
/// failed with access-denied, fell through to the Ollama fallback, which
/// produced wrong-dimension embeddings only the MCP path ever saw).
pub fn data_dir() -> PathBuf {
    std::env::var("GENIUZ_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join(".geniuz"))
}

/// Platform-specific path where Claude Desktop stores its MCP server
/// configuration. The dashboard's Status surface reads this file to verify
/// whether Geniuz is wired into Claude Desktop as an MCP server.
///
/// Returns None on unsupported platforms (Linux has no Claude Desktop).
pub fn claude_desktop_config_path() -> Option<PathBuf> {
    let home = home_dir();
    #[cfg(target_os = "macos")]
    return Some(home.join("Library/Application Support/Claude/claude_desktop_config.json"));
    #[cfg(target_os = "windows")]
    return Some(home.join("AppData/Roaming/Claude/claude_desktop_config.json"));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = home;
        None
    }
}

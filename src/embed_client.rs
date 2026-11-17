//! Client for geniuz-embed IPC server
//!
//! Try connecting first. If unavailable, fall back to inline ONNX loading.
//! If GENIUZ_EMBED_SPAWN=1, auto-spawn the server on first use.
//!
//! Cross-platform IPC via interprocess crate:
//!   Unix/macOS: Unix domain socket
//!   Windows:    Named pipe
//! No network surface on any platform — silent to firewalls and netstat.

use std::io::{Read, Write};
use interprocess::local_socket::Stream;

const EMBEDDING_BYTES: usize = 384 * 4;

/// Embed text via the local IPC server. Returns None if server unavailable.
/// The connection is made only to this user's own helper (see `embed_socket`).
pub fn embed_via_socket(text: &str) -> Option<Vec<f32>> {
    // Try connecting first
    if let Some(s) = crate::embed_socket::connect() {
        return embed_on_stream(s, text);
    }

    // Connection failed — try auto-spawning
    if std::env::var("GENIUZ_EMBED_SPAWN").ok().as_deref() == Some("1") {
        spawn_server();
    } else {
        return None;
    }

    embed_on_stream(crate::embed_socket::connect()?, text)
}

/// Send text over a connected stream, receive embedding back
fn embed_on_stream(mut stream: Stream, text: &str) -> Option<Vec<f32>> {
    let text_bytes = text.as_bytes();
    let len = text_bytes.len() as u32;
    stream.write_all(&len.to_le_bytes()).ok()?;
    stream.write_all(text_bytes).ok()?;

    let mut buf = vec![0u8; EMBEDDING_BYTES];
    stream.read_exact(&mut buf).ok()?;

    let is_zero = buf.iter().all(|&b| b == 0);
    if is_zero { return None; }

    crate::embedding::blob_to_embedding(&buf).ok()
}

/// Try to spawn geniuz-embed in background
fn spawn_server() {
    let self_path = std::env::current_exe().ok();
    let embed_name = if cfg!(windows) { "geniuz-embed.exe" } else { "geniuz-embed" };
    let embed_path = self_path.as_ref()
        .and_then(|p| p.parent())
        .map(|dir| dir.join(embed_name));

    let binary = match embed_path {
        Some(p) if p.exists() => p,
        _ => return,
    };

    // The helper works out where to listen the same way this program does.
    let _ = std::process::Command::new(binary)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn();

    // Wait for server to start listening (up to 5 seconds)
    for _ in 0..50 {
        if crate::embed_socket::connect().is_some() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

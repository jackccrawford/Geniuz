//! Client for geniuz-embed socket server
//!
//! Try socket first. If unavailable, fall back to inline ONNX loading.
//! If GENIUZ_EMBED_SPAWN=1, auto-spawn the server on first use.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

const DEFAULT_SOCKET: &str = "/tmp/geniuz-embed.sock";
const EMBEDDING_BYTES: usize = 384 * 4;

/// Embed text via the socket server. Returns None if server unavailable.
pub fn embed_via_socket(text: &str) -> Option<Vec<f32>> {
    let socket_path = std::env::var("GENIUZ_EMBED_SOCKET")
        .unwrap_or_else(|_| DEFAULT_SOCKET.to_string());

    if Path::new(&socket_path).exists() {
        // Socket file exists — try connecting. If it fails, it's stale.
        match UnixStream::connect(&socket_path) {
            Ok(s) => {
                s.set_read_timeout(Some(std::time::Duration::from_secs(2))).ok()?;
                // Connected — use this stream below
                return embed_on_stream(s, text);
            }
            Err(_) => {
                // Stale socket — remove it
                let _ = std::fs::remove_file(&socket_path);
            }
        }
    }

    // No socket (or stale socket removed) — try auto-spawning
    if std::env::var("GENIUZ_EMBED_SPAWN").ok().as_deref() == Some("1") {
        spawn_server(&socket_path);
    } else {
        return None;
    }

    let stream = UnixStream::connect(&socket_path).ok()?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(2))).ok()?;
    embed_on_stream(stream, text)
}

/// Send text over a connected stream, receive embedding back
fn embed_on_stream(mut stream: UnixStream, text: &str) -> Option<Vec<f32>> {
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
fn spawn_server(socket_path: &str) {
    // Find the binary next to ourselves
    let self_path = std::env::current_exe().ok();
    let embed_path = self_path.as_ref()
        .and_then(|p| p.parent())
        .map(|dir| dir.join("geniuz-embed"));

    let binary = match embed_path {
        Some(p) if p.exists() => p,
        _ => return, // Can't find the binary, fall back to inline
    };

    // Spawn detached
    let _ = std::process::Command::new(binary)
        .env("GENIUZ_EMBED_SOCKET", socket_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn();

    // Wait for socket to appear (up to 5 seconds)
    for _ in 0..50 {
        if Path::new(socket_path).exists() {
            std::thread::sleep(std::time::Duration::from_millis(100));
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

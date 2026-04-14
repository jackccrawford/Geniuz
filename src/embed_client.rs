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
use interprocess::local_socket::{
    prelude::*, GenericNamespaced, Stream, ToNsName,
};

const DEFAULT_NAME: &str = "geniuz-embed.sock";
const EMBEDDING_BYTES: usize = 384 * 4;

fn ipc_name() -> String {
    std::env::var("GENIUZ_EMBED_SOCKET")
        .unwrap_or_else(|_| DEFAULT_NAME.to_string())
}

/// Embed text via the local IPC server. Returns None if server unavailable.
pub fn embed_via_socket(text: &str) -> Option<Vec<f32>> {
    let name = ipc_name();
    let ns_name = name.as_str().to_ns_name::<GenericNamespaced>().ok()?;

    // Try connecting first
    if let Ok(s) = Stream::connect(ns_name.clone()) {
        return embed_on_stream(s, text);
    }

    // Connection failed — try auto-spawning
    if std::env::var("GENIUZ_EMBED_SPAWN").ok().as_deref() == Some("1") {
        spawn_server(&name);
    } else {
        return None;
    }

    let stream = Stream::connect(ns_name).ok()?;
    embed_on_stream(stream, text)
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
fn spawn_server(name: &str) {
    let self_path = std::env::current_exe().ok();
    let embed_name = if cfg!(windows) { "geniuz-embed.exe" } else { "geniuz-embed" };
    let embed_path = self_path.as_ref()
        .and_then(|p| p.parent())
        .map(|dir| dir.join(embed_name));

    let binary = match embed_path {
        Some(p) if p.exists() => p,
        _ => return,
    };

    let _ = std::process::Command::new(binary)
        .env("GENIUZ_EMBED_SOCKET", name)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn();

    // Wait for server to start listening (up to 5 seconds)
    let ns_name = match name.to_ns_name::<GenericNamespaced>() {
        Ok(n) => n,
        Err(_) => return,
    };
    for _ in 0..50 {
        if Stream::connect(ns_name.clone()).is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

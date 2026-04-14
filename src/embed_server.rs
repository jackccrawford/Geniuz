//! geniuz-embed — keeps the ONNX session warm for fast embedding
//!
//! Cross-platform IPC server via interprocess crate:
//!   Unix/macOS: Unix domain socket
//!   Windows:    Named pipe
//! Protocol: send text (u32 LE length prefix + UTF-8 bytes), receive 1536 bytes (384 × f32 LE)
//! Auto-exits after idle timeout (default 5 min, configurable via GENIUZ_EMBED_IDLE)

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use interprocess::local_socket::{
    prelude::*, GenericNamespaced, ListenerOptions, ToNsName,
};

use geniuz::embedding;

const DEFAULT_NAME: &str = "geniuz-embed.sock";
const DEFAULT_IDLE_SECS: u64 = 300;
const EMBEDDING_BYTES: usize = 384 * 4;

fn main() {
    let name = std::env::var("GENIUZ_EMBED_SOCKET")
        .unwrap_or_else(|_| DEFAULT_NAME.to_string());
    let idle_secs = std::env::var("GENIUZ_EMBED_IDLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_IDLE_SECS);

    let ns_name = match name.as_str().to_ns_name::<GenericNamespaced>() {
        Ok(n) => n,
        Err(e) => {
            eprintln!("[geniuz-embed] Bad socket name {}: {}", name, e);
            std::process::exit(1);
        }
    };

    // Check if already running (try connecting)
    if interprocess::local_socket::Stream::connect(ns_name.clone()).is_ok() {
        eprintln!("[geniuz-embed] Already running at {}", name);
        std::process::exit(0);
    }

    // Load ONNX backend (one-time cost)
    let backend = match embedding::create_backend() {
        Ok(b) => {
            eprintln!("[geniuz-embed] {} ready", b.name());
            b
        }
        Err(e) => {
            eprintln!("[geniuz-embed] Failed: {}", e);
            std::process::exit(1);
        }
    };

    let listener = match ListenerOptions::new()
        .name(ns_name)
        .create_sync()
    {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[geniuz-embed] Bind failed: {}", e);
            std::process::exit(1);
        }
    };
    listener.set_nonblocking(interprocess::local_socket::ListenerNonblockingMode::Both)
        .expect("set_nonblocking");

    eprintln!("[geniuz-embed] {} (idle: {}s)", name, idle_secs);

    let mut last_activity = Instant::now();
    let idle_timeout = Duration::from_secs(idle_secs);
    let mut served: u64 = 0;

    loop {
        match listener.accept() {
            Ok(mut stream) => {
                last_activity = Instant::now();

                // Read u32 LE length prefix
                let mut len_buf = [0u8; 4];
                if stream.read_exact(&mut len_buf).is_err() { continue; }
                let text_len = u32::from_le_bytes(len_buf) as usize;

                if text_len == 0 || text_len > 1_000_000 {
                    let _ = stream.write_all(&[0u8; EMBEDDING_BYTES]);
                    continue;
                }

                // Read text
                let mut text_buf = vec![0u8; text_len];
                if stream.read_exact(&mut text_buf).is_err() { continue; }

                let text = match String::from_utf8(text_buf) {
                    Ok(t) => t,
                    Err(_) => continue,
                };

                // Embed and respond
                match backend.embed(&text) {
                    Ok(emb) => {
                        let _ = stream.write_all(&embedding::embedding_to_blob(&emb));
                        served += 1;
                    }
                    Err(e) => {
                        eprintln!("[geniuz-embed] Error: {}", e);
                        let _ = stream.write_all(&[0u8; EMBEDDING_BYTES]);
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if last_activity.elapsed() > idle_timeout {
                    eprintln!("[geniuz-embed] Idle. {} served. Exiting.", served);
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                eprintln!("[geniuz-embed] Fatal accept error: {}", e);
                break;
            }
        }
    }
}

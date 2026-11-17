//! geniuz-embed — keeps the ONNX session warm for fast embedding
//!
//! Cross-platform IPC server via interprocess crate:
//!   Unix/macOS: Unix domain socket
//!   Windows:    Named pipe
//! Protocol: send text (u32 LE length prefix + UTF-8 bytes), receive 1536 bytes (384 × f32 LE)
//! Auto-exits after idle timeout (default 5 min, configurable via GENIUZ_EMBED_IDLE)

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use interprocess::local_socket::prelude::*;

use geniuz::{embed_socket, embedding};

const DEFAULT_IDLE_SECS: u64 = 300;
const EMBEDDING_BYTES: usize = 384 * 4;
/// How long one caller may take to send its text or accept the answer. The helper
/// serves one caller at a time, so a stalled one must not hold it.
const CALLER_TIMEOUT: Duration = Duration::from_secs(2);

fn main() {
    geniuz::private::by_default();
    let idle_secs = std::env::var("GENIUZ_EMBED_IDLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_IDLE_SECS);

    let Some(place) = embed_socket::describe() else {
        eprintln!("[geniuz-embed] No private place for the socket (the path may be too long). Geniuz loads the model itself instead.");
        std::process::exit(1);
    };

    // Check if already running (try connecting)
    if embed_socket::connect().is_some() {
        eprintln!("[geniuz-embed] Already running at {}", place);
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

    let listener = match embed_socket::listen() {
        Ok(l) => l,
        // Another helper started in the same moment and got there first.
        Err(e) if e == embed_socket::ALREADY_RUNNING => {
            eprintln!("[geniuz-embed] Already running at {}", place);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("[geniuz-embed] Bind failed: {}", e);
            std::process::exit(1);
        }
    };
    listener.set_nonblocking(interprocess::local_socket::ListenerNonblockingMode::Accept)
        .expect("set_nonblocking");

    eprintln!("[geniuz-embed] {} (idle: {}s)", place, idle_secs);

    let mut last_activity = Instant::now();
    let idle_timeout = Duration::from_secs(idle_secs);
    let mut served: u64 = 0;

    loop {
        match listener.accept() {
            Ok(mut stream) => {
                // Only this user's own programs are served; anyone else is dropped unread.
                if !embed_socket::may_serve(&stream) {
                    continue;
                }
                last_activity = Instant::now();
                // A connection can inherit the listener's non-blocking mode (it does on macOS),
                // which would drop a caller whose text arrives in more than one piece. Each
                // caller is read in blocking mode, with a limit on how long it may take.
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_recv_timeout(Some(CALLER_TIMEOUT));
                let _ = stream.set_send_timeout(Some(CALLER_TIMEOUT));

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

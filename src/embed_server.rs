//! clawmark-embed — keeps the ONNX session warm for fast embedding
//!
//! Unix domain socket server at /tmp/clawmark-embed.sock
//! Protocol: send text (u32 LE length prefix + UTF-8 bytes), receive 1536 bytes (384 × f32 LE)
//! Auto-exits after idle timeout (default 5 min, configurable via CLAWMARK_EMBED_IDLE)

use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::time::{Duration, Instant};

use clawmark::embedding;

const DEFAULT_SOCKET: &str = "/tmp/clawmark-embed.sock";
const DEFAULT_IDLE_SECS: u64 = 300;
const EMBEDDING_BYTES: usize = 384 * 4;

fn main() {
    let socket_path = std::env::var("CLAWMARK_EMBED_SOCKET")
        .unwrap_or_else(|_| DEFAULT_SOCKET.to_string());
    let idle_secs = std::env::var("CLAWMARK_EMBED_IDLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_IDLE_SECS);

    // Clean up stale socket
    if Path::new(&socket_path).exists() {
        if std::os::unix::net::UnixStream::connect(&socket_path).is_ok() {
            eprintln!("[clawmark-embed] Already running at {}", socket_path);
            std::process::exit(0);
        }
        let _ = std::fs::remove_file(&socket_path);
    }

    // Load ONNX backend (one-time cost)
    let backend = match embedding::create_backend() {
        Ok(b) => {
            eprintln!("[clawmark-embed] {} ready", b.name());
            b
        }
        Err(e) => {
            eprintln!("[clawmark-embed] Failed: {}", e);
            std::process::exit(1);
        }
    };

    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[clawmark-embed] Bind failed: {}", e);
            std::process::exit(1);
        }
    };
    listener.set_nonblocking(true).expect("set_nonblocking");

    eprintln!("[clawmark-embed] {} (idle: {}s)", socket_path, idle_secs);

    let mut last_activity = Instant::now();
    let idle_timeout = Duration::from_secs(idle_secs);
    let mut served: u64 = 0;

    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                last_activity = Instant::now();

                // Set read timeout so a stalled client can't hang the server
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));

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
                        eprintln!("[clawmark-embed] Error: {}", e);
                        let _ = stream.write_all(&[0u8; EMBEDDING_BYTES]);
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if last_activity.elapsed() > idle_timeout {
                    eprintln!("[clawmark-embed] Idle. {} served. Exiting.", served);
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                eprintln!("[clawmark-embed] Fatal accept error: {}", e);
                break;
            }
        }
    }

    let _ = std::fs::remove_file(&socket_path);
}

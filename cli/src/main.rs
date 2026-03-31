mod cli;
mod db;
mod mcp;

use clap::FromArgMatches;
use cli::{Cli, Command};
use std::path::PathBuf;

fn main() {
    let cli = Cli::from_arg_matches(&Cli::build().get_matches())
        .expect("Failed to parse arguments");

    match run(cli) {
        Ok(output) => {
            if !output.is_empty() { println!("{}", output); }
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

fn default_station_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".geniuz").join("station.db")
}

fn get_db() -> Result<db::DatabaseManager, String> {
    let path = std::env::var("GENIUZ_STATION")
        .unwrap_or_else(|_| default_station_path().to_string_lossy().to_string());
    db::DatabaseManager::new(&path)
}

fn shorten_ts(ts: &str) -> &str {
    if ts.len() >= 16 { &ts[..16] } else { ts }
}

fn run(cli: Cli) -> Result<String, String> {
    match cli.command {
        Command::Signal { content, gist, parent, json } => {
            // Resolve content: @file or stdin
            let resolved = if content == "-" {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)
                    .map_err(|e| format!("Failed to read stdin: {}", e))?;
                buf
            } else if let Some(path) = content.strip_prefix('@') {
                std::fs::read_to_string(path)
                    .map_err(|e| format!("Failed to read '{}': {}", path, e))?
            } else {
                content
            };

            let db = get_db()?;
            let short_uuid = db.signal(&resolved, gist.as_deref(), parent.as_deref(), None)?;

            if json {
                Ok(serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true, "action": "signal", "uuid": short_uuid
                })).unwrap())
            } else {
                Ok(format!("✅ Signal {} saved", short_uuid))
            }
        }

        Command::Tune { query, recent, random, keyword, full, limit, json } => {
            if query.as_deref() == Some("help") {
                let mut cmd = Cli::build();
                let sub = cmd.find_subcommand_mut("tune").unwrap();
                sub.print_help().ok();
                println!();
                return Ok(String::new());
            }

            let db = get_db()?;

            let mut entries = if random {
                match db.random()? {
                    Some(e) => vec![e],
                    None => vec![],
                }
            } else if recent || query.is_none() {
                db.recent(limit)?
            } else {
                let q = query.as_deref().unwrap();
                if keyword {
                    db.keyword_search(q, limit)?
                } else {
                    db.semantic_search(q, limit)?
                }
            };

            // Enrich with full content if requested
            if full {
                for entry in &mut entries {
                    if let Ok(Some(content)) = db.get_full_content(&entry.signal_uuid) {
                        entry.content = Some(content);
                    }
                }
            }

            if json {
                let data: Vec<serde_json::Value> = entries.iter().map(|e| {
                    let mut v = serde_json::json!({
                        "uuid": &e.signal_uuid[..8],
                        "gist": e.gist,
                        "created_at": e.created_at,
                    });
                    if let Some(ref p) = e.parent_uuid { v["parent"] = serde_json::json!(&p[..8]); }
                    if let Some(s) = e.score { v["score"] = serde_json::json!(format!("{:.3}", s)); }
                    if let Some(ref c) = e.content { v["content"] = serde_json::json!(c); }
                    v
                }).collect();
                return Ok(serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true, "action": "tune", "count": data.len(), "signals": data
                })).unwrap());
            }

            if entries.is_empty() {
                return Ok("No memories found.".to_string());
            }

            let mut lines: Vec<String> = Vec::new();
            for e in &entries {
                let uuid_short = &e.signal_uuid[..8];
                let ts = shorten_ts(&e.created_at);
                let mut suffix = String::new();
                if let Some(ref p) = e.parent_uuid {
                    suffix.push_str(&format!(" <- {}", &p[..8.min(p.len())]));
                }
                if let Some(s) = e.score {
                    suffix.push_str(&format!(" ({:.3})", s));
                }
                lines.push(format!("{} | {} | {}{}", uuid_short, ts, e.gist, suffix));
                if let Some(ref content) = e.content {
                    for line in content.lines() {
                        lines.push(format!("           {}", line));
                    }
                    lines.push(String::new());
                }
            }
            Ok(lines.join("\n"))
        }

        Command::Backfill => {
            let db = get_db()?;
            db.set_embedding_model(geniuz::embedding::model_id())?;

            let uncached = db.get_uncached_signals()?;
            if uncached.is_empty() {
                return Ok("All memories already cached.".to_string());
            }

            println!("[backfill] {} memories to embed...", uncached.len());
            let backend = geniuz::embedding::create_backend()?;

            let mut cached = 0;
            let mut failed = 0;
            for (i, (uuid, content)) in uncached.iter().enumerate() {
                match backend.embed(content) {
                    Ok(emb) => {
                        if db.cache_embedding(uuid, &emb).is_ok() { cached += 1; }
                        else { failed += 1; }
                    }
                    Err(_) => { failed += 1; }
                }
                if (i + 1) % 50 == 0 {
                    println!("[backfill] {}/{}", i + 1, uncached.len());
                }
            }

            Ok(format!("[backfill] Done. {} cached, {} failed.", cached, failed))
        }

        Command::Status => {
            let db = get_db()?;
            let signals = db.count()?;
            let embeddings = db.embedding_count()?;
            let path = std::env::var("GENIUZ_STATION")
                .unwrap_or_else(|_| default_station_path().to_string_lossy().to_string());

            let mut lines = vec![
                format!("Station: {}", path),
                format!("Memories: {}", signals),
                format!("Embeddings: {}/{} cached", embeddings, signals),
            ];
            if embeddings < signals {
                lines.push("Run 'geniuz backfill' to cache remaining.".to_string());
            } else if signals > 0 {
                lines.push("Semantic search: ready".to_string());
            }
            Ok(lines.join("\n"))
        }

        Command::Mcp(mcp_cmd) => {
            use cli::McpCommand;
            match mcp_cmd {
                McpCommand::Serve => {
                    mcp::serve();
                    Ok(String::new())
                }
                McpCommand::Install => {
                    mcp::install()
                }
                McpCommand::Status => {
                    mcp::status()
                }
            }
        }
    }
}

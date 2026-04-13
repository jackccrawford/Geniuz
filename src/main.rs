mod adapter;
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
    PathBuf::from(home).join(".geniuz").join("memory.db")
}

fn legacy_station_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".clawmark").join("memory.db")
}

fn default_claw_workspace() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".openclaw").join("workspace")
}

pub fn get_db() -> Result<db::DatabaseManager, String> {
    // Check env vars: GENIUZ_STATION first, CLAWMARK_STATION as legacy fallback
    let path = std::env::var("GENIUZ_STATION")
        .or_else(|_| std::env::var("CLAWMARK_STATION"))
        .unwrap_or_else(|_| {
            let new_path = default_station_path();
            let old_path = legacy_station_path();
            if new_path.exists() {
                new_path.to_string_lossy().to_string()
            } else if old_path.exists() {
                eprintln!("[geniuz] Using legacy station at ~/.clawmark/memory.db");
                old_path.to_string_lossy().to_string()
            } else {
                new_path.to_string_lossy().to_string()
            }
        });
    db::DatabaseManager::new(&path)
}

pub fn shorten_ts(ts: &str) -> &str {
    if ts.len() >= 16 { &ts[..16] } else { ts }
}

fn run(cli: Cli) -> Result<String, String> {
    match cli.command {
        Command::Skill => {
            Ok(include_str!("../skills/SKILL.md").to_string())
        }

        Command::Capture { paths, openclaw, split, gist_prefix, dry_run } => {
            // OpenClaw mode: use the adapter
            if let Some(oc_path) = openclaw {
                let ws_path = oc_path.map(PathBuf::from)
                    .unwrap_or_else(default_claw_workspace);

                let workspace = adapter::detect_workspace(&ws_path)
                    .ok_or_else(|| format!("No OpenClaw workspace found at {}\nExpected MEMORY.md or memory/ directory.", ws_path.display()))?;

                let summary = adapter::workspace_summary(&workspace);
                println!("{}", summary);

                if dry_run {
                    println!("\n--dry-run: no changes made.");
                    return Ok(String::new());
                }

                let db = get_db()?;
                let (created, errors) = adapter::migrate(&workspace, &db)?;

                let mut lines = vec![
                    format!("\n✅ Captured: {} memories from OpenClaw workspace", created),
                ];
                if errors > 0 {
                    lines.push(format!("⚠️  {} errors (see above)", errors));
                }
                lines.push("Run 'geniuz backfill' to enable semantic search.".to_string());
                return Ok(lines.join("\n"));
            }

            // General mode: capture files and directories
            if paths.is_empty() {
                return Err("No files specified. Use 'geniuz capture <files...>' or 'geniuz capture --openclaw'.".to_string());
            }

            let mut files: Vec<PathBuf> = Vec::new();
            for p in &paths {
                let path = PathBuf::from(p);
                if path.is_dir() {
                    match std::fs::read_dir(&path) {
                        Ok(entries) => {
                            for entry in entries.flatten() {
                                let ep = entry.path();
                                if ep.extension().map(|e| e == "md").unwrap_or(false) {
                                    files.push(ep);
                                }
                            }
                        }
                        Err(e) => eprintln!("[capture] Failed to read directory {}: {}", path.display(), e),
                    }
                } else if path.is_file() {
                    files.push(path);
                } else {
                    eprintln!("[capture] Not found: {}", path.display());
                }
            }
            files.sort();

            if files.is_empty() {
                return Err("No files found to capture.".to_string());
            }

            println!("[capture] {} file(s) to process", files.len());
            for f in &files {
                println!("  {}", f.display());
            }

            if dry_run {
                println!("\n--dry-run: no changes made.");
                return Ok(String::new());
            }

            let db = get_db()?;
            let backend = geniuz::embedding::create_backend()?;
            let prefix = gist_prefix.as_deref().unwrap_or("");
            let mut created = 0usize;
            let mut errors = 0usize;

            for file_path in &files {
                let content = match std::fs::read_to_string(file_path) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[capture] Failed to read {}: {}", file_path.display(), e);
                        errors += 1;
                        continue;
                    }
                };
                let content = content.trim();
                if content.is_empty() { continue; }

                let filename = file_path.file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                if split {
                    let sections = adapter::split_sections(content);
                    let root_gist = format!("{}capture: {}", prefix, filename);
                    let root_uuid = match db.signal_with_backend(content, Some(&root_gist), None, None, Some(backend.as_ref())) {
                        Ok(uuid) => {
                            created += 1;
                            uuid
                        }
                        Err(e) => {
                            eprintln!("[capture] Failed: {}", e);
                            errors += 1;
                            continue;
                        }
                    };
                    for (i, section) in sections.iter().enumerate() {
                        let gist = match &section.header {
                            Some(h) => format!("{}capture: {} — {}", prefix, filename, h),
                            None => format!("{}capture: {} (section {})", prefix, filename, i + 1),
                        };
                        match db.signal_with_backend(&section.content, Some(&gist), Some(&root_uuid), None, Some(backend.as_ref())) {
                            Ok(_) => { created += 1; }
                            Err(e) => {
                                eprintln!("[capture] Failed: {}", e);
                                errors += 1;
                            }
                        }
                    }
                } else {
                    let gist = format!("{}capture: {}", prefix, filename);
                    match db.signal_with_backend(content, Some(&gist), None, None, Some(backend.as_ref())) {
                        Ok(_) => { created += 1; }
                        Err(e) => {
                            eprintln!("[capture] Failed: {}", e);
                            errors += 1;
                        }
                    }
                }
            }

            let mut lines = vec![
                format!("\n✅ Captured: {} memories from {} file(s)", created, files.len()),
            ];
            if errors > 0 {
                lines.push(format!("⚠️  {} errors (see above)", errors));
            }
            lines.push("Run 'geniuz backfill' to enable semantic search.".to_string());
            Ok(lines.join("\n"))
        }

        Command::Remember { content, gist, parent, json } => {
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
                    "ok": true, "action": "remember", "uuid": short_uuid
                })).unwrap())
            } else {
                Ok(format!("✅ Remembered {}", short_uuid))
            }
        }

        Command::Recall { query, random, keyword, full, limit, json } => {
            if query.as_deref() == Some("help") {
                let mut cmd = Cli::build();
                let sub = cmd.find_subcommand_mut("recall").unwrap();
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
            } else if query.is_none() {
                // No query on recall → show recent as fallback
                db.recent(limit)?
            } else {
                let q = query.as_deref().unwrap();
                if keyword {
                    db.keyword_search(q, limit)?
                } else {
                    db.semantic_search(q, limit)?
                }
            };

            if full {
                for entry in &mut entries {
                    if let Ok(Some(content)) = db.get_full_content(&entry.memory_uuid) {
                        entry.content = Some(content);
                    }
                }
            }

            if json {
                let data: Vec<serde_json::Value> = entries.iter().map(|e| {
                    let mut v = serde_json::json!({
                        "uuid": &e.memory_uuid[..8],
                        "gist": e.gist,
                        "created_at": e.created_at,
                    });
                    if let Some(ref p) = e.parent_uuid { v["parent"] = serde_json::json!(&p[..8]); }
                    if let Some(s) = e.score { v["score"] = serde_json::json!(format!("{:.3}", s)); }
                    if let Some(ref c) = e.content { v["content"] = serde_json::json!(c); }
                    v
                }).collect();
                return Ok(serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true, "action": "recall", "count": data.len(), "memories": data
                })).unwrap());
            }

            if entries.is_empty() {
                return Ok("No memories found.".to_string());
            }

            let mut lines: Vec<String> = Vec::new();
            for e in &entries {
                let uuid_short = &e.memory_uuid[..8];
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

        Command::Recent { limit, full, json } => {
            let db = get_db()?;
            let mut entries = db.recent(limit)?;

            if full {
                for entry in &mut entries {
                    if let Ok(Some(content)) = db.get_full_content(&entry.memory_uuid) {
                        entry.content = Some(content);
                    }
                }
            }

            if json {
                let data: Vec<serde_json::Value> = entries.iter().map(|e| {
                    let mut v = serde_json::json!({
                        "uuid": &e.memory_uuid[..8],
                        "gist": e.gist,
                        "created_at": e.created_at,
                    });
                    if let Some(ref p) = e.parent_uuid { v["parent"] = serde_json::json!(&p[..8]); }
                    if let Some(ref c) = e.content { v["content"] = serde_json::json!(c); }
                    v
                }).collect();
                return Ok(serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true, "action": "recent", "count": data.len(), "memories": data
                })).unwrap());
            }

            if entries.is_empty() {
                return Ok("No memories yet.".to_string());
            }

            let mut lines: Vec<String> = Vec::new();
            for e in &entries {
                let uuid_short = &e.memory_uuid[..8];
                let ts = shorten_ts(&e.created_at);
                let mut suffix = String::new();
                if let Some(ref p) = e.parent_uuid {
                    suffix.push_str(&format!(" <- {}", &p[..8.min(p.len())]));
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

        Command::Watch { interval, since, exec, once, json } => {
            let db = get_db()?;

            let mut cursor = if let Some(ref uuid_prefix) = since {
                db.get_signal_timestamp(uuid_prefix)?
                    .ok_or_else(|| format!("Memory not found: {}", uuid_prefix))?
            } else {
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
            };

            if !once {
                eprintln!("[watch] Watching for new memories (every {}s). Ctrl+C to stop.", interval);
                if since.is_some() {
                    eprintln!("[watch] Starting after: {}", cursor);
                }
            }

            loop {
                let entries = db.since(&cursor, 100)?;

                if !entries.is_empty() {
                    cursor = entries.last().unwrap().created_at.clone();

                    for e in &entries {
                        if let Some(ref cmd_template) = exec {
                            let uuid_short = &e.memory_uuid[..8.min(e.memory_uuid.len())];
                            let parent_short = e.parent_uuid.as_deref()
                                .map(|p| &p[..8.min(p.len())])
                                .unwrap_or("");
                            let content = e.content.as_deref().unwrap_or("");
                            let signal_json = serde_json::json!({
                                "uuid": uuid_short,
                                "gist": &e.gist,
                                "content": content,
                                "created_at": &e.created_at,
                                "parent": parent_short,
                            });

                            let cmd = cmd_template
                                .replace("{uuid}", uuid_short)
                                .replace("{gist}", &e.gist)
                                .replace("{content}", content)
                                .replace("{created_at}", &e.created_at)
                                .replace("{parent}", parent_short)
                                .replace("{json}", &signal_json.to_string());

                            match std::process::Command::new("sh")
                                .arg("-c")
                                .arg(&cmd)
                                .status()
                            {
                                Ok(status) => {
                                    if !status.success() {
                                        eprintln!("[watch] exec exited with: {}", status);
                                    }
                                }
                                Err(e) => eprintln!("[watch] exec failed: {}", e),
                            }
                        } else if json {
                            let uuid_short = &e.memory_uuid[..8.min(e.memory_uuid.len())];
                            let parent_short = e.parent_uuid.as_deref()
                                .map(|p| &p[..8.min(p.len())]);
                            let mut v = serde_json::json!({
                                "uuid": uuid_short,
                                "gist": &e.gist,
                                "created_at": &e.created_at,
                            });
                            if let Some(p) = parent_short { v["parent"] = serde_json::json!(p); }
                            if let Some(ref c) = e.content { v["content"] = serde_json::json!(c); }
                            println!("{}", serde_json::to_string(&v).unwrap());
                        } else {
                            let uuid_short = &e.memory_uuid[..8.min(e.memory_uuid.len())];
                            let ts = shorten_ts(&e.created_at);
                            let mut suffix = String::new();
                            if let Some(ref p) = e.parent_uuid {
                                suffix.push_str(&format!(" <- {}", &p[..8.min(p.len())]));
                            }
                            println!("+ {} | {} | {}{}", uuid_short, ts, e.gist, suffix);
                        }
                    }
                }

                if once {
                    if entries.is_empty() {
                        return Ok("No new memories.".to_string());
                    } else {
                        return Ok(String::new());
                    }
                }

                std::thread::sleep(std::time::Duration::from_secs(interval));
            }
        }

        Command::Backfill => {
            let db = get_db()?;
            db.set_embedding_model(geniuz::embedding::model_id())?;

            let uncached = db.get_uncached_signals()?;
            if uncached.is_empty() {
                return Ok("All memories cached.".to_string());
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
                .or_else(|_| std::env::var("CLAWMARK_STATION"))
                .unwrap_or_else(|_| default_station_path().to_string_lossy().to_string());

            let mut lines = vec![
                format!("Folder: {}", path),
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

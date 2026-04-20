//! MCP server over stdio — exposes geniuz as remember/recall tools
//!
//! Claude Desktop connects via stdio transport. The agent gets three
//! human-friendly tools that wrap geniuz's signal/tune operations.
//!
//! Tool descriptions guide the model toward proactive memory use —
//! recalling on startup, remembering during conversation.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

use geniuz::db::{DatabaseManager, SignalEntry};
use geniuz::embedding::{self, EmbeddingBackend};

// =============================================================================
// MCP Protocol Types
// =============================================================================

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

fn success(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse { jsonrpc: "2.0".to_string(), id, result: Some(result), error: None }
}

fn error_response(id: Value, code: i64, message: &str) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(), id, result: None,
        error: Some(json!({ "code": code, "message": message })),
    }
}

fn tool_result(id: Value, text: &str, is_error: bool) -> JsonRpcResponse {
    success(id, json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error
    }))
}

// =============================================================================
// Tool Definitions
// =============================================================================

fn tool_definitions() -> Value {
    json!({
        "tools": [
            {
                "name": "remember",
                "description": "Save something worth remembering for future sessions. Use this when you learn something important about the user — their preferences, decisions, client details, project context, or anything they would not want to repeat. Your future self will find it by meaning, not keywords. Signal more than you think you should — storage is free, forgetting is expensive.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "The full detail. Write for a future you that knows nothing about this session. Include names, numbers, decisions, reasoning, context."
                        },
                        "gist": {
                            "type": "string",
                            "description": "A one-line summary for finding this later. Format: 'category: key insight'. Example: 'client: Maria — Q2 retention focus, $40K budget'"
                        },
                        "thread": {
                            "type": "string",
                            "description": "Optional. Short UUID of a previous memory to thread this to. Builds chains — prospect to client, draft to final, problem to solution."
                        }
                    },
                    "required": ["content", "gist"]
                }
            },
            {
                "name": "recall",
                "description": "Search your memories by meaning. Use this at the START of every conversation to check what you already know about the topic or the user. Also use it whenever you need context from previous sessions. The search finds related memories even when the words are different — 'budget priorities' finds memories about 'retention focus, $40K'. Use this proactively and often.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "What you are looking for. A topic, a name, a concept. Semantic search finds related memories even if the exact words differ."
                        },
                        "full": {
                            "type": "boolean",
                            "description": "If true, returns full content of each memory. Default false (gist summaries only)."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results. Default 10."
                        }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "recall_recent",
                "description": "Get the most recent memories. Use this at the START of every conversation to see what happened in recent sessions. This is how you orient yourself — what was the user working on? What decisions were made? What context matters right now?",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "limit": {
                            "type": "integer",
                            "description": "Number of recent memories to return. Default 5."
                        },
                        "full": {
                            "type": "boolean",
                            "description": "If true, returns full content. Default false (gist summaries only)."
                        }
                    }
                }
            }
        ]
    })
}

// =============================================================================
// Tool Execution
// =============================================================================

fn execute_remember(
    db: &DatabaseManager,
    backend: Option<&dyn EmbeddingBackend>,
    params: &Value,
) -> (String, bool) {
    let content = match params.get("content").and_then(|c| c.as_str()) {
        Some(c) => c,
        None => return ("Error: content is required".to_string(), true),
    };
    let gist = params.get("gist").and_then(|g| g.as_str());
    let thread = params.get("thread").and_then(|t| t.as_str());

    match db.signal_with_backend(content, gist, thread, None, backend) {
        Ok(uuid) => (format!("Remembered ({})", uuid), false),
        Err(e) => (format!("Error: {}", e), true),
    }
}

fn execute_recall(db: &DatabaseManager, params: &Value) -> (String, bool) {
    let query = match params.get("query").and_then(|q| q.as_str()) {
        Some(q) => q,
        None => return ("Error: query is required".to_string(), true),
    };
    let full = params.get("full").and_then(|f| f.as_bool()).unwrap_or(false);
    let limit = params.get("limit").and_then(|l| l.as_u64()).unwrap_or(10).min(100) as usize;

    // Semantic first, keyword fallback
    let results = match db.semantic_search(query, limit) {
        Ok(r) if !r.is_empty() => r,
        _ => match db.keyword_search(query, limit) {
            Ok(r) => r,
            Err(e) => return (format!("Error: {}", e), true),
        },
    };

    if results.is_empty() {
        return ("No memories found for that query.".to_string(), false);
    }

    (format_entries(&results, full, db), false)
}

fn execute_recall_recent(db: &DatabaseManager, params: &Value) -> (String, bool) {
    let limit = params.get("limit").and_then(|l| l.as_u64()).unwrap_or(5).min(100) as usize;
    let full = params.get("full").and_then(|f| f.as_bool()).unwrap_or(false);

    match db.recent(limit) {
        Ok(entries) if entries.is_empty() => {
            ("No memories yet. This is a fresh start.".to_string(), false)
        }
        Ok(entries) => (format_entries(&entries, full, db), false),
        Err(e) => (format!("Error: {}", e), true),
    }
}

fn format_entries(entries: &[SignalEntry], full: bool, db: &DatabaseManager) -> String {
    let mut lines = Vec::new();
    for e in entries {
        let ts = crate::shorten_ts(&e.created_at);
        let score_str = e.score.map(|s| format!(" ({:.2})", s)).unwrap_or_default();
        if full {
            let content = db.get_full_content(&e.memory_uuid)
                .ok().flatten().unwrap_or_default();
            lines.push(format!("{} | {} | {}{}\n  {}", &e.memory_uuid[..8], ts, e.gist, score_str, content));
        } else {
            lines.push(format!("{} | {} | {}{}", &e.memory_uuid[..8], ts, e.gist, score_str));
        }
    }
    lines.join("\n")
}

// =============================================================================
// MCP Server Loop
// =============================================================================

pub fn serve() {
    let db = match crate::get_db() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("[geniuz] Failed to open station: {}", e);
            std::process::exit(1);
        }
    };

    // Build the embedding backend once at startup and reuse it for every
    // remember call. Constructing a fresh ort::Session per call inside a
    // long-lived MCP subprocess produces wrong-dimension output on Windows;
    // reuse matches the CLI pattern in main.rs and also eliminates per-call
    // model-load overhead. Soft-fail to keyword-only if backend init fails.
    let backend = match embedding::create_backend() {
        Ok(b) => Some(b),
        Err(e) => {
            eprintln!("[geniuz] Embedding backend unavailable ({}); MCP remember will soft-fail to keyword-only.", e);
            None
        }
    };

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = error_response(Value::Null, -32700, &format!("Parse error: {}", e));
                let json_str = serde_json::to_string(&resp).unwrap_or_default();
                let _ = writeln!(stdout, "{}", json_str);
                let _ = stdout.flush();
                continue;
            }
        };

        let id = request.id.clone().unwrap_or(Value::Null);

        let response = match request.method.as_str() {
            "initialize" => {
                success(id, json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": {
                        "name": "Geniuz",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }))
            }

            "notifications/initialized" => continue,

            "tools/list" => success(id, tool_definitions()),

            "tools/call" => {
                let tool_name = request.params.get("name")
                    .and_then(|n| n.as_str()).unwrap_or("");
                let arguments = request.params.get("arguments")
                    .cloned().unwrap_or(json!({}));

                let (text, is_error) = match tool_name {
                    "remember" => execute_remember(&db, backend.as_deref(), &arguments),
                    "recall" => execute_recall(&db, &arguments),
                    "recall_recent" => execute_recall_recent(&db, &arguments),
                    _ => (format!("Unknown tool: {}", tool_name), true),
                };

                tool_result(id, &text, is_error)
            }

            _ => error_response(id, -32601, &format!("Method not found: {}", request.method)),
        };

        let json_str = serde_json::to_string(&response).unwrap_or_default();
        let _ = writeln!(stdout, "{}", json_str);
        let _ = stdout.flush();
    }
}

// =============================================================================
// Install / Status
// =============================================================================

/// All Claude Desktop config paths on this platform.
///
/// Most platforms have one path. Windows can have two because Claude Desktop
/// ships in two distribution flavors:
///   1. `.exe` download → reads from `%APPDATA%\Claude\` (standard, what
///      `dirs::config_dir()` returns).
///   2. Microsoft Store / MSIX package → reads from a sandboxed location
///      `%LOCALAPPDATA%\Packages\Claude_<hash>\LocalCache\Roaming\Claude\`.
///      The package directory only exists when the Store version is installed,
///      so we detect at runtime by scanning for any `Claude_*` package folder.
///
/// We always include the standard path (so first-time installs land somewhere
/// useful even if Claude isn't installed yet) and additionally include any
/// Store package path that exists right now. Writing to both is harmless —
/// each Claude variant only reads from its own path.
fn config_paths() -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();

    // Standard cross-platform location:
    //   macOS:   ~/Library/Application Support/Claude/claude_desktop_config.json
    //   Windows: %APPDATA%\Claude\claude_desktop_config.json (.exe Claude)
    //   Linux:   ~/.config/Claude/claude_desktop_config.json
    if let Some(base) = dirs::config_dir() {
        paths.push(base.join("Claude").join("claude_desktop_config.json"));
    }

    // Windows-only: detect Microsoft Store packaged Claude Desktop.
    #[cfg(target_os = "windows")]
    if let Some(local) = dirs::data_local_dir() {
        let packages = local.join("Packages");
        if packages.exists() {
            if let Ok(entries) = std::fs::read_dir(&packages) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    if name.to_string_lossy().starts_with("Claude_") {
                        paths.push(
                            entry.path()
                                .join("LocalCache")
                                .join("Roaming")
                                .join("Claude")
                                .join("claude_desktop_config.json"),
                        );
                    }
                }
            }
        }
    }

    if paths.is_empty() {
        // Last-resort fallback so install() always has *somewhere* to write
        paths.push(std::path::PathBuf::from(".")
            .join("Claude")
            .join("claude_desktop_config.json"));
    }
    paths
}

fn geniuz_binary_path() -> String {
    // Use the currently running binary's path
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "geniuz".to_string())
}

/// Parse `KEY=VALUE` strings into a JSON object suitable for the MCP
/// server entry's `env` block. Skips malformed entries with a warning.
fn parse_env_args(env_args: &[String]) -> serde_json::Map<String, serde_json::Value> {
    let mut env_map = serde_json::Map::new();
    for entry in env_args {
        if let Some((k, v)) = entry.split_once('=') {
            env_map.insert(k.to_string(), serde_json::Value::String(v.to_string()));
        } else {
            eprintln!("[geniuz mcp install] Ignoring malformed --env (expected KEY=VALUE): {}", entry);
        }
    }
    env_map
}

pub fn install(env_args: &[String]) -> Result<String, String> {
    let binary = geniuz_binary_path();
    let env_map = parse_env_args(env_args);
    let paths = config_paths();
    let mut written = Vec::new();
    let mut errors = Vec::new();

    for config_file in &paths {
        match install_to_path(config_file, &binary, &env_map) {
            Ok(()) => written.push(config_file.clone()),
            Err(e) => errors.push(format!("{}: {}", config_file.display(), e)),
        }
    }

    if written.is_empty() {
        return Err(format!(
            "Failed to install MCP config — no writable location:\n  {}",
            errors.join("\n  ")
        ));
    }

    let mut lines = vec![
        "✅ Geniuz installed in Claude Desktop.".to_string(),
        String::new(),
    ];
    if written.len() == 1 {
        lines.push(format!("  Config: {}", written[0].display()));
    } else {
        lines.push("  Config written to:".to_string());
        for p in &written {
            lines.push(format!("    {}", p.display()));
        }
    }
    lines.push(format!("  Binary: {}", binary));
    lines.push(String::new());
    lines.push("  Restart Claude Desktop to activate.".to_string());
    lines.push("  Your Claude will have: remember, recall, recall_recent".to_string());

    if !errors.is_empty() {
        lines.push(String::new());
        lines.push("  Note: some locations were not writable (this is usually fine):".to_string());
        for e in &errors {
            lines.push(format!("    {}", e));
        }
    }

    let station = crate::default_db_path();
    if station.exists() {
        if let Ok(db) = crate::get_db() {
            let count = db.count().unwrap_or(0);
            if count > 0 {
                lines.push(String::new());
                lines.push(format!("  Station has {} existing memories — Claude will find them.", count));
            }
        }
    }

    Ok(lines.join("\n"))
}

/// Read-modify-write a single Claude Desktop config file: load existing JSON
/// (or start fresh if absent), upsert the Geniuz MCP entry, write back.
fn install_to_path(
    config_file: &std::path::Path,
    binary: &str,
    env_map: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let mut config: serde_json::Value = if config_file.exists() {
        let content = std::fs::read_to_string(config_file)
            .map_err(|e| format!("read failed: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("parse failed: {}", e))?
    } else {
        if let Some(parent) = config_file.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir failed: {}", e))?;
        }
        serde_json::json!({})
    };

    if config.get("mcpServers").is_none() {
        config["mcpServers"] = serde_json::json!({});
    }

    let mut entry = serde_json::json!({
        "command": binary,
        "args": ["mcp", "serve"]
    });
    if !env_map.is_empty() {
        entry["env"] = serde_json::Value::Object(env_map.clone());
    }
    config["mcpServers"]["Geniuz"] = entry;

    let formatted = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("serialize failed: {}", e))?;
    std::fs::write(config_file, &formatted)
        .map_err(|e| format!("write failed: {}", e))?;
    Ok(())
}

pub fn status() -> Result<String, String> {
    let paths = config_paths();
    let mut lines = Vec::new();
    let mut any_installed = false;
    let mut any_present = false;

    for config_file in &paths {
        if !config_file.exists() {
            lines.push(format!("Config: {} (not present)", config_file.display()));
            continue;
        }
        any_present = true;

        let content = match std::fs::read_to_string(config_file) {
            Ok(c) => c,
            Err(e) => {
                lines.push(format!("Config: {} (read error: {})", config_file.display(), e));
                continue;
            }
        };
        let config: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                lines.push(format!("Config: {} (parse error: {})", config_file.display(), e));
                continue;
            }
        };

        let installed = config.get("mcpServers")
            .and_then(|s| s.get("Geniuz"))
            .is_some();
        if installed { any_installed = true; }

        lines.push(format!("Config: {}", config_file.display()));
        lines.push(format!("  Geniuz: {}", if installed { "installed" } else { "not installed" }));
        if installed {
            if let Some(cmd) = config["mcpServers"]["Geniuz"].get("command").and_then(|c| c.as_str()) {
                lines.push(format!("  Binary: {}", cmd));
            }
        }
    }

    if !any_present {
        return Ok(format!(
            "Claude Desktop config not found at any known location:\n  {}\n\nRun 'geniuz mcp install' first.",
            paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n  ")
        ));
    }

    // Station info
    let station = crate::default_db_path();
    if station.exists() {
        if let Ok(db) = crate::get_db() {
            let count = db.count().unwrap_or(0);
            let embeddings = db.embedding_count().unwrap_or(0);
            lines.push(format!("Station: {} ({} memories, {}/{} embedded)", station.display(), count, embeddings, count));
        }
    } else {
        lines.push("Station: not created yet (will be created on first remember)".to_string());
    }

    if !any_installed {
        lines.push(String::new());
        lines.push("Run 'geniuz mcp install' to add Geniuz to Claude Desktop.".to_string());
    }

    Ok(lines.join("\n"))
}

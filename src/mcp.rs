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
use geniuz::window::Window;

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
                            "description": "What you are looking for. A topic, a name, a concept. Semantic search finds related memories even if the exact words differ. Required unless thread is given."
                        },
                        "grep": {
                            "type": ["string", "array"],
                            "items": { "type": "string" },
                            "description": "Optional. Exact words to find, case-blind, in the FULL content: returns the matching lines, not whole memories. Use it for names, numbers and anything you know verbatim (semantic search does not find proper names reliably). A list means every term must appear in the memory. Combines with since/until and thread."
                        },
                        "thread": {
                            "type": "string",
                            "description": "Optional. The short UUID of any memory in a thread (the 8 characters before '|', or the one after '<-'). Returns that whole thread, root first, oldest to newest, instead of searching. Use it to read a conversation you found one piece of."
                        },
                        "full": {
                            "type": "boolean",
                            "description": "If true, returns full content of each memory. Default false (gist summaries only)."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results. Default 10."
                        },
                        "since": {
                            "type": "string",
                            "description": "Optional. Only memories from this time on: '24h', '7d', '2w', a date ('2026-09-23'), a local time ('2026-09-23 14:00') or RFC 3339. Use it whenever the question has a time in it ('last week', 'yesterday') — without it, older memories outrank recent ones."
                        },
                        "until": {
                            "type": "string",
                            "description": "Optional. Only memories before this time. Same forms as since; a date includes that whole day."
                        }
                    }
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
                        },
                        "since": {
                            "type": "string",
                            "description": "Optional. Only memories from this time on: '24h', '7d', '2w', a date ('2026-09-23'), a local time ('2026-09-23 14:00') or RFC 3339. Use it whenever the question has a time in it ('last week', 'yesterday') — without it, older memories outrank recent ones."
                        },
                        "until": {
                            "type": "string",
                            "description": "Optional. Only memories before this time. Same forms as since; a date includes that whole day."
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
        Ok(result) if result.embedded => (format!("Remembered ({})", result.uuid), false),
        // State, not instruction. This server exposes three tools and none of
        // them runs the CLI, so telling the caller to run `backfill` is a
        // command it may be unable to follow — while some hosts have a shell
        // and could. Report what is true and let the caller decide whether to
        // run it or relay it; the server cannot see the caller's capabilities.
        Ok(result) => (
            format!(
                "Remembered ({}) — saved without a semantic embedding; keyword-searchable until 'geniuz backfill' runs.",
                result.uuid
            ),
            false,
        ),
        Err(e) => (format!("Error: {}", e), true),
    }
}

fn execute_recall(db: &DatabaseManager, params: &Value) -> (String, bool) {
    let full = params.get("full").and_then(|f| f.as_bool()).unwrap_or(false);
    let window = match window_from(params) {
        Ok(w) => w,
        Err(e) => return (format!("Error: {}", e), true),
    };
    let thread = params.get("thread").and_then(|t| t.as_str()).map(str::trim).filter(|t| !t.is_empty());
    let grep: Vec<String> = match params.get("grep") {
        Some(Value::String(t)) => vec![t.clone()],
        Some(Value::Array(a)) => a.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    };
    if !grep.is_empty() {
        let terms = match geniuz::dig::Terms::parse(&grep) {
            Ok(t) => t,
            Err(e) => return (format!("Error: {}", e), true),
        };
        let limit = params.get("limit").and_then(|l| l.as_u64()).unwrap_or(50).min(500) as usize;
        return execute_grep(db, &terms, &window, thread, limit);
    }
    if let Some(id) = thread {
        let limit = params.get("limit").and_then(|l| l.as_u64()).unwrap_or(100).min(500) as usize;
        return execute_thread(db, id, &window, limit, full);
    }
    let query = match params.get("query").and_then(|q| q.as_str()) {
        Some(q) => q,
        None => return ("Error: query is required (or thread, to read a whole thread)".to_string(), true),
    };
    let limit = params.get("limit").and_then(|l| l.as_u64()).unwrap_or(10).min(100) as usize;

    // Semantic first, keyword fallback
    let mut mode = format!("semantic \"{query}\"");
    let results = match db.semantic_search_in(query, &window, limit) {
        Ok(r) if !r.is_empty() => r,
        _ => {
            mode = format!("keyword \"{query}\"");
            match db.keyword_search_in(query, &window, limit) {
                Ok(r) => r,
                Err(e) => return (format!("Error: {}", e), true),
            }
        }
    };
    let scope = scope(db, &window, &mode);

    if results.is_empty() {
        return (format!("No memories found for that query.\n{scope}"), false);
    }

    // An MCP caller never sees stderr, so a partial index has to be said in
    // the response or it isn't said at all — the whole point of the audit
    // finding: unembedded memories were invisible rather than absent.
    let mut out = format_entries(&results, full, db);
    if let Ok(n) = db.unembedded_count() {
        if n > 0 {
            out.push_str(&format!(
                "\n\n({} {} not embedded — ranked results are partial until 'geniuz backfill' runs.)",
                n, if n == 1 { "memory" } else { "memories" }
            ));
        }
    }
    out.push_str(&format!("\n{scope}"));
    (out, false)
}

fn execute_recall_recent(db: &DatabaseManager, params: &Value) -> (String, bool) {
    let limit = params.get("limit").and_then(|l| l.as_u64()).unwrap_or(5).min(100) as usize;
    let full = params.get("full").and_then(|f| f.as_bool()).unwrap_or(false);
    let window = match window_from(params) {
        Ok(w) => w,
        Err(e) => return (format!("Error: {}", e), true),
    };

    match db.recent_in(&window, limit) {
        Ok(entries) if entries.is_empty() && window.is_all() => {
            ("No memories yet. This is a fresh start.".to_string(), false)
        }
        Ok(entries) if entries.is_empty() => {
            (format!("No memories found.\n{}", scope(db, &window, "recent")), false)
        }
        Ok(entries) => (
            format!("{}\n{}", format_entries(&entries, full, db), scope(db, &window, "recent")),
            false,
        ),
        Err(e) => (format!("Error: {}", e), true),
    }
}

/// The `since` / `until` arguments of a recall call, parsed the CLI's way.
fn window_from(params: &Value) -> Result<Window, String> {
    Window::parse(
        params.get("since").and_then(|v| v.as_str()),
        params.get("until").and_then(|v| v.as_str()),
    )
}

/// Matching lines (search feature 2), each tagged `UUID · time ·`, closed
/// by the scope: how many lines in how many memories, from what pool.
fn execute_grep(
    db: &DatabaseManager, terms: &geniuz::dig::Terms, window: &Window, thread: Option<&str>, limit: usize,
) -> (String, bool) {
    let dig = match db.grep_in(terms, window, thread, limit) {
        Ok(d) => d,
        Err(e) => return (format!("Error: {}", e), true),
    };
    let (pool, count) = match thread {
        Some(id) => match db.thread_in(id, window, 1) {
            Ok(t) => (Some(format!("thread {} · root {}", id.to_uppercase(), &t.root[..8.min(t.root.len())])), Some(t.total)),
            Err(e) => return (format!("Error: {}", e), true),
        },
        None => (None, db.count_in(window).ok()),
    };
    let scope = match count {
        Some(n) => geniuz::window::scope_line(pool.as_deref(), n, window, &dig.mode(terms)),
        None => format!("searched: {}", dig.mode(terms)),
    };
    if dig.hits.is_empty() {
        return (format!("No lines found.\n{scope}"), false);
    }
    let mut lines: Vec<String> = dig.hits.iter().map(|h| format!(
        "{} · {} · {}", &h.memory_uuid[..8.min(h.memory_uuid.len())], crate::shorten_ts(&h.created_at), h.line,
    )).collect();
    lines.push(scope);
    (lines.join("\n"), false)
}

/// A whole thread, root first (search feature 6), closed by its scope.
fn execute_thread(db: &DatabaseManager, id: &str, window: &Window, limit: usize, full: bool) -> (String, bool) {
    let t = match db.thread_in(id, window, limit) {
        Ok(t) => t,
        Err(e) => return (format!("Error: {}", e), true),
    };
    let label = format!("thread {} · root {}", id.to_uppercase(), &t.root[..8.min(t.root.len())]);
    let shown = if t.entries.len() < t.total {
        format!("oldest {} shown (raise limit for more)", t.entries.len())
    } else {
        "oldest first".to_string()
    };
    let scope = geniuz::window::scope_line(Some(&label), t.total, window, &shown);
    if t.entries.is_empty() {
        return (format!("No memories in that thread inside the window.\n{scope}"), false);
    }
    (format!("{}\n{scope}", format_entries(&t.entries, full, db)), false)
}

/// The closing scope line (search feature 5): what was searched, so an empty
/// answer carries its reach. A count that cannot be read is left out.
fn scope(db: &DatabaseManager, window: &Window, mode: &str) -> String {
    match db.count_in(window) {
        Ok(n) => geniuz::window::scope_line(None, n, window, mode),
        Err(_) => format!("searched: {mode}"),
    }
}

fn format_entries(entries: &[SignalEntry], full: bool, db: &DatabaseManager) -> String {
    let mut lines = Vec::new();
    for e in entries {
        let head = entry_line(e);
        if full {
            let content = db.get_full_content(&e.memory_uuid)
                .ok().flatten().unwrap_or_default();
            lines.push(format!("{}\n  {}", head, content));
        } else {
            lines.push(head);
        }
    }
    lines.join("\n")
}

/// One result line, in the CLI's shape: `UUID | time | gist <- PARENT (score)`.
/// The parent arrow is what lets an MCP caller see the threads it builds with
/// `thread:` — without it, chains were stored but invisible through this door.
fn entry_line(e: &SignalEntry) -> String {
    let ts = crate::shorten_ts(&e.created_at);
    let mut suffix = String::new();
    if let Some(ref p) = e.parent_uuid {
        suffix.push_str(&format!(" <- {}", &p[..8.min(p.len())]));
    }
    if let Some(s) = e.score {
        suffix.push_str(&format!(" ({:.3})", s));
    }
    format!("{} | {} | {}{}", &e.memory_uuid[..8], ts, e.gist, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(parent: Option<&str>, score: Option<f32>) -> SignalEntry {
        SignalEntry {
            memory_uuid: "5B1A36D9-0000-0000-0000-000000000000".into(),
            gist: "test: child".into(),
            created_at: "2026-09-27 07:19:39".into(),
            parent_uuid: parent.map(String::from),
            content: None,
            score,
        }
    }

    #[test]
    fn line_shows_parent_before_score_like_the_cli() {
        let line = entry_line(&entry(Some("6B3F7B38-0000-0000-0000-000000000000"), Some(0.483)));
        assert!(line.starts_with("5B1A36D9 | "), "{line}");
        assert!(line.ends_with("test: child <- 6B3F7B38 (0.483)"), "{line}");
    }

    /// An empty answer inside a window must say what it searched, or "nothing
    /// found" reads as "nothing there" (search feature 5).
    #[test]
    fn an_empty_windowed_answer_carries_its_scope() {
        let dir = tempfile::tempdir().unwrap();
        let db = DatabaseManager::new(dir.path().join("memory.db").to_str().unwrap()).unwrap();
        db.signal("an old note", Some("old"), None, Some("2020-01-01 00:00:00")).unwrap();
        let (out, is_err) = execute_recall_recent(&db, &json!({"since": "7d"}));
        assert!(!is_err, "{out}");
        assert!(out.ends_with("searched: 0 memories · since 7d · recent"), "{out}");
        let (out, _) = execute_recall_recent(&db, &json!({}));
        assert!(out.ends_with("searched: 1 memory · all time · recent"), "{out}");
        let (out, is_err) = execute_recall_recent(&db, &json!({"since": "someday"}));
        assert!(is_err && out.contains("--since `someday`"), "{out}");
    }

    /// recall with `grep` returns lines, and the terms can be a string or a list.
    #[test]
    fn recall_with_grep_returns_the_matching_lines() {
        let dir = tempfile::tempdir().unwrap();
        let db = DatabaseManager::new(dir.path().join("memory.db").to_str().unwrap()).unwrap();
        db.signal("Roundup\nCubic reviewed the PR.\nDevin filed issues.", Some("reviews"), None, None).unwrap();
        let (out, is_err) = execute_recall(&db, &json!({"grep": "cubic"}));
        assert!(!is_err, "{out}");
        assert!(out.contains(" · Cubic reviewed the PR."), "{out}");
        assert!(!out.contains("Devin"), "only matching lines: {out}");
        assert!(out.ends_with("grep \"cubic\" · 1 line in 1 memory"), "{out}");
        let (out, _) = execute_recall(&db, &json!({"grep": ["cubic", "zebra"]}));
        assert!(out.starts_with("No lines found."), "{out}");
    }

    /// recall with `thread` reads the whole conversation from any member.
    #[test]
    fn recall_with_thread_reads_the_whole_conversation() {
        let dir = tempfile::tempdir().unwrap();
        let db = DatabaseManager::new(dir.path().join("memory.db").to_str().unwrap()).unwrap();
        let root = db.signal("the question", Some("question"), None, Some("2026-09-01 00:00:00")).unwrap().uuid;
        let reply = db.signal("the answer", Some("answer"), Some(&root), Some("2026-09-02 00:00:00")).unwrap().uuid;
        db.signal("elsewhere", Some("elsewhere"), None, Some("2026-09-03 00:00:00")).unwrap();
        let (out, is_err) = execute_recall(&db, &json!({"thread": reply}));
        assert!(!is_err, "{out}");
        let q = out.find("question").unwrap();
        let a = out.find(&format!("answer <- {root}")).unwrap();
        assert!(q < a, "root first: {out}");
        assert!(!out.contains("elsewhere"), "{out}");
        assert!(out.ends_with(&format!("searched: thread {reply} · root {root} · 2 memories · all time · oldest first")), "{out}");
        let (out, is_err) = execute_recall(&db, &json!({}));
        assert!(is_err && out.contains("or thread"), "{out}");
    }

    #[test]
    fn root_line_has_no_arrow() {
        let line = entry_line(&entry(None, None));
        assert!(line.ends_with("test: child"), "{line}");
        assert!(!line.contains("<-"), "{line}");
    }
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

//! MCP server over stdio — exposes geniuz as keep, recall and folders tools
//!
//! Claude Desktop connects via stdio transport. Tool descriptions guide the
//! model toward proactive memory use — recalling on startup, keeping during
//! conversation.
//!
//! The folder is resolved on every call, never held from startup: a long-lived
//! server must not keep writing to a path a folder has moved away from, since
//! opening that path would create an empty store.
//! `remember` and `recall_recent` still answer for older configurations, but
//! aren't listed.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

use geniuz::db::{DatabaseManager, MemoryEntry};
use geniuz::embedding::{self, EmbeddingBackend};
use geniuz::folders::{parse_address, search_across, Registry, Search};
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

/// What an agent is told when it connects. Tool descriptions alone weren't enough:
/// in earlier testing, agents with no other guidance used Geniuz in 0 of 8 sessions,
/// and with the imperative wording that preceded this one, in 4 of 4.
///
/// This wording, measured 9 Nov (fresh claude -p, only Geniuz, empty memory, 2-3 runs
/// each, old wording vs this): asked "I've just installed a new MCP called Geniuz.
/// Could you take a look at it?", both read folders and recall; the old wording then
/// went digging in the shell, this one stopped and told the user Geniuz "suggests
/// asking you once whether notes should be kept". Asked for garden advice, neither
/// called a tool; this one offered to keep notes in 1 of 3, the old one in 0 of 3.
///
/// Worded as explanation, not orders (Jack, 9 Nov): a model reads "do this, then that"
/// from a tool as a possible injection, and asks before keeping on the user's behalf.
const INSTRUCTIONS: &str = "Geniuz keeps notes that last beyond this chat, on this computer. \
Recall with no query shows the most recent notes, a quick way to see what's already known when a conversation starts; \
a search then finds what's relevant to the topic at hand. \
Many people like to be asked once whether notes should be kept. \
When they agree, keep saves what a later chat would need and couldn't get from the files: \
decisions and why, preferences, what to pick up next. \
In a scheduled or unattended run, with no one to ask, keeping what the next run will need is what Geniuz is for. \
Notes live in folders, each describing what belongs there; the folders tool lists them. \
A note can't be moved or removed afterwards.";

fn tool_definitions() -> Value {
    let mut tools = base_tool_definitions();
    let folder = json!({
        "type": "string",
        "description": "Which folder to read, by name or 8-character ID. Choose one with the folders tool. Left out, this reads the folder set for where you were started (a .geniuz-folder file or GENIUZ_DB), else My memory. An unknown name is an error, never a different folder."
    });
    for tool in tools["tools"].as_array_mut().into_iter().flatten() {
        if tool["name"] != "folders" {
            tool["inputSchema"]["properties"]["folder"] = folder.clone();
        }
        // A memory is written only where the writer says: keep needs its folder.
        if tool["name"] == "keep" {
            tool["inputSchema"]["properties"]["folder"]["description"] = json!(
                "Which folder to keep this in, by name or 8-character ID. Required: find it with the folders tool first. An unknown name is an error, never a different folder."
            );
            if let Some(req) = tool["inputSchema"]["required"].as_array_mut() {
                req.push(json!("folder"));
            }
        }
    }
    tools
}

fn base_tool_definitions() -> Value {
    json!({
        "tools": [
            {
                "name": "keep",
                "description": "Saves a note for later chats: a decision and why, a preference, project context, what to pick up next. It's found later by meaning, not exact words. Each note goes in a named folder, with a one-line gist.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "The full detail. Write for a future you that knows nothing about this session. Include names, numbers, decisions, reasoning, context."
                        },
                        "gist": {
                            "type": "string",
                            "description": "A one-line summary for finding this later. Format: 'category: key insight'. Example: 'decision: ship 5.0.9 Monday, skill header now stamped'"
                        },
                        "thread": {
                            "type": "string",
                            "description": "Optional. Short UUID or address of a memory in the same folder to thread this to. Builds chains: question to answer, draft to final, problem to solution. A memory in another folder can't be threaded to: keep this in that folder, or put its address in the content instead."
                        }
                    },
                    "required": ["content", "gist"]
                }
            },
            {
                "name": "recall",
                "description": "Search your memories by meaning. Use this at the START of every conversation to check what you already know about the topic or the user. Also use it whenever you need context from previous sessions. The search finds related memories even when the words are different: 'budget priorities' finds memories about 'retention focus, $40K'. Use this proactively and often.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "What you are looking for. A topic, a name, a concept. Semantic search finds related memories even if the exact words differ. Proper names (a Capitalized word after the first, or anything in \"quotes\") must match exactly, and memories holding them come first. An address like 13625F6B:3487B9B0 opens that memory, whichever folder it is in. Leave it out to get the most recent memories, which is how to orient yourself at the start of a conversation."
                        },
                        "all": {
                            "type": "boolean",
                            "description": "If true, searches every folder and shows each result's address. Use it when you don't know which folder holds something."
                        },
                        "grep": {
                            "type": ["string", "array"],
                            "items": { "type": "string" },
                            "description": "Optional. Exact words to find, case-blind, in the FULL content: returns the matching lines, not whole memories. Use it for names, numbers and anything you know verbatim (semantic search does not find proper names reliably). A list means every term must appear in the memory. Combines with since/until and thread."
                        },
                        "thread": {
                            "type": "string",
                            "description": "Optional. The short UUID or address of any memory in a thread (the 8 characters before '|', or the one after '<-'). Returns that whole thread, root first, oldest to newest, instead of searching. Use it to read a conversation you found one piece of."
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
                            "description": "Optional. Only memories from this time on: '24h', '7d', '2w', a date ('2026-09-23'), a local time ('2026-09-23 14:00') or RFC 3339. Use it whenever the question has a time in it ('last week', 'yesterday'). Without it, older memories outrank recent ones."
                        },
                        "until": {
                            "type": "string",
                            "description": "Optional. Only memories before this time. Same forms as since; a date includes that whole day."
                        }
                    }
                }
            },
            {
                "name": "folders",
                "description": "List the folders you can use: each is a separate store of memories with its own purpose. Check a folder's description before writing to it, since it may say what belongs there. Pass a name to read one folder's whole description. Pass create to make a new folder, when the person wants memories kept apart.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Optional. A folder's name or 8-character ID, to read it in full."
                        },
                        "filter": {
                            "type": "string",
                            "description": "Optional. A word from a folder's name or description. With many folders the list is a shortlist; this finds the others."
                        },
                        "create": {
                            "type": "string",
                            "description": "Optional. The name of a new folder to create, such as Work."
                        },
                        "description": {
                            "type": "string",
                            "description": "Optional, with create. What belongs in the folder, for whoever comes here next."
                        },
                        "face": {
                            "type": "string",
                            "description": "Optional, with name or create. One emoji shown beside the folder's name in the Geniuz window. A folder's own writer chooses it; an empty value clears it."
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

fn execute_keep(
    db: &DatabaseManager,
    backend: Option<&dyn EmbeddingBackend>,
    params: &Value,
) -> (String, bool) {
    let content = match params.get("content").and_then(|c| c.as_str()) {
        Some(c) => c,
        None => return ("Error: content is required".to_string(), true),
    };
    let gist = match params.get("gist").and_then(|g| g.as_str()) {
        Some(g) => g,
        None => return ("Error: gist is required: one line saying what this memory is".to_string(), true),
    };
    let thread = params.get("thread").and_then(|t| t.as_str());
    // A memory's time is when it was kept, never a time the caller gives (Jack, 8 Nov):
    // something about the past is said in the gist and content.
    match db.keep_with_backend(content, gist, thread, backend) {
        Ok(result) if result.embedded => (format!("Kept ({})", db.address(&result.uuid)), false),
        // State, not instruction. This server exposes three tools and none of
        // them runs the CLI, so telling the caller to run `backfill` is a
        // command it may be unable to follow — while some hosts have a shell
        // and could. Report what is true and let the caller decide whether to
        // run it or relay it; the server cannot see the caller's capabilities.
        Ok(result) => (
            format!(
                "Kept ({}), without an embedding for now: found by keywords until 'geniuz backfill' runs.",
                db.address(&result.uuid)
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
    let query = match params.get("query").and_then(|q| q.as_str()).map(str::trim).filter(|q| !q.is_empty()) {
        Some(q) => q,
        None => return execute_recall_recent(db, params),
    };
    let limit = params.get("limit").and_then(|l| l.as_u64()).unwrap_or(10).min(100) as usize;

    // Semantic first, keyword fallback
    let mut mode = geniuz::dig::semantic_mode(query);
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
                "\n\n({} {} not embedded, so results may be incomplete until 'geniuz backfill' runs.)",
                n, if n == 1 { "memory" } else { "memories" }
            ));
        }
    }
    let harvest = db.harvest(&results, query).lines();
    if !harvest.is_empty() {
        out.push_str(&format!("\n{}", harvest.join("\n")));
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
        Ok(entries) => {
            let mut out = format_entries(&entries, full, db);
            for line in db.harvest(&entries, "").lines() {
                out.push_str(&format!("\n{line}"));
            }
            (format!("{out}\n{}", scope(db, &window, "recent")), false)
        }
        Err(e) => (format!("Error: {}", e), true),
    }
}

/// The `since` / `until` arguments of a recall call, parsed the CLI's way.
/// One line per folder, clipped: the full list across many folders has to fit
/// comfortably in a conversation. A name returns that folder whole.
fn execute_folders(reg: &Registry, params: &Value) -> (String, bool) {
    // By length, not by line: a rule on a folder's second line is still a rule.
    const CLIPPED: usize = 300;
    if let Some(name) = params.get("create").and_then(|n| n.as_str()) {
        let description = params.get("description").and_then(|d| d.as_str()).unwrap_or("");
        let face = params.get("face").and_then(|f| f.as_str());
        // Checked first, so a face that won't do doesn't leave a folder behind.
        if let Some(Err(e)) = face.map(geniuz::folders::validate_face) {
            return (format!("Error: {}", e), true);
        }
        return match reg.create(name, description, None).and_then(|f| match face {
            Some(face) => reg.set_face(&f.uuid, face),
            None => Ok(f),
        }) {
            Ok(f) => (format!("Created {} ({}). To keep a memory there, pass folder: \"{}\".", f.name, f.short_id(), f.name), false),
            Err(e) => (format!("Error: {}", e), true),
        };
    }
    if let Some(q) = params.get("name").and_then(|n| n.as_str()) {
        let found = match params.get("face").and_then(|f| f.as_str()) {
            Some(face) => reg.set_face(q, face),
            None => reg.find(q),
        };
        return match found {
            Ok(f) => {
                let (count, last) = f.stats().unwrap_or((0, None));
                (
                    format!(
                        "{}{} ({})\n{} memories, last written {}\n\n{}",
                        if f.face.is_empty() { String::new() } else { format!("{} ", f.face) },
                        f.name,
                        f.short_id(),
                        count,
                        last.as_deref().map(crate::shorten_ts).unwrap_or_else(|| "never".to_string()),
                        f.description
                    ),
                    false,
                )
            }
            Err(e) => (format!("Error: {}", e), true),
        };
    }
    let scan = reg.scan();
    let current = crate::resolve_db_path().ok();
    // Many folders (hundreds, where many agents share a machine) would cost every agent
    // thousands of tokens per call. Past SHORTLIST_FROM, list the folder memories go to,
    // the pinned ones and the week's newest, and say how to find the rest.
    const SHORTLIST_FROM: usize = 20;
    const RECENT: usize = 10;
    const MATCHES: usize = 30;
    let filter = params.get("filter").and_then(|f| f.as_str()).map(|f| f.trim().to_lowercase()).filter(|f| !f.is_empty());
    let stats: Vec<(i64, Option<String>)> = scan.folders.iter().map(|f| f.stats().unwrap_or((0, None))).collect();
    let mut chosen: Vec<usize> = (0..scan.folders.len()).collect();
    let mut tail: Option<String> = None;
    if let Some(q) = &filter {
        let hits: Vec<usize> = chosen.into_iter().filter(|&i| {
            let f = &scan.folders[i];
            f.name.to_lowercase().contains(q.as_str()) || f.description.to_lowercase().contains(q.as_str()) || f.short_id().to_lowercase().starts_with(q.as_str())
        }).collect();
        if hits.is_empty() {
            return (format!("No folder's name or description mentions \"{}\".", q), false);
        }
        if hits.len() > MATCHES {
            tail = Some(format!("…and {} more matches. Narrow the filter.", hits.len() - MATCHES));
        }
        chosen = hits.into_iter().take(MATCHES).collect();
    } else if scan.folders.len() > SHORTLIST_FROM {
        let pins = reg.pinned();
        let week_ago = (chrono::Utc::now() - chrono::Duration::days(7)).format("%Y-%m-%d %H:%M:%S").to_string();
        let mut recent: Vec<usize> = (0..scan.folders.len())
            .filter(|&i| stats[i].1.as_deref().is_some_and(|w| w > week_ago.as_str()))
            .collect();
        recent.sort_by(|&a, &b| stats[b].1.cmp(&stats[a].1));
        let mut picked: Vec<usize> = Vec::new();
        let canon = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let here = current.as_deref().map(canon);
        let in_use = (0..scan.folders.len()).filter(|&i| here.as_deref() == Some(canon(&scan.folders[i].db_path).as_path()));
        let pinned = (0..scan.folders.len()).filter(|&i| pins.iter().any(|p| p.eq_ignore_ascii_case(&scan.folders[i].uuid)));
        for i in in_use.chain(pinned).chain(recent.into_iter().take(RECENT)) {
            if !picked.contains(&i) { picked.push(i); }
        }
        let homes = scan.folders.iter().filter(|f| in_agents_tree(&f.db_path)).count();
        let rest = scan.folders.len() - picked.len();
        if rest > 0 { tail = Some(format!(
            "{} more folders not shown ({} total{}). Pass filter with a word from a name or description to find one, e.g. filter: \"work\".",
            rest,
            scan.folders.len(),
            if homes > 0 { format!(", {} of them agents' homes", homes) } else { String::new() }
        )); }
        chosen = picked;
    }
    let mut lines = Vec::new();
    for &i in &chosen {
        let f = &scan.folders[i];
        let (count, last) = stats[i].clone();
        let flat = f.description.split_whitespace().collect::<Vec<_>>().join(" ");
        let summary: String = flat.chars().take(CLIPPED).collect();
        let more = if flat.chars().count() > CLIPPED { "…" } else { "" };
        let mark = if current.as_ref() == Some(&f.db_path) { "* " } else { "" };
        let noun = if count == 1 { "memory" } else { "memories" };
        let mut line = format!(
            "{}{} | {} | {} {} · {}",
            mark, f.short_id(), f.name, count, noun,
            last.as_deref().map(crate::shorten_ts).unwrap_or_else(|| "never".to_string())
        );
        if !summary.is_empty() {
            line.push_str(&format!(" · {}{}", summary, more));
        }
        lines.push(line);
    }
    if let Some(t) = tail {
        lines.push(t);
    }
    if !scan.blocked.is_empty() {
        lines.push(format!("{} blocked as copies of each other, until one is moved.", scan.blocked.len()));
    }
    (lines.join("\n"), false)
}

/// A folder whose directory sits in one named `agents` is an agent's home.
fn in_agents_tree(db_path: &std::path::Path) -> bool {
    db_path.parent().and_then(|d| d.parent()).and_then(|p| p.file_name())
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("agents"))
}

/// The store for this call: the named folder, or My memory.
fn open_for_call(reg: &Registry, params: &Value) -> Result<DatabaseManager, String> {
    let named = params.get("folder").and_then(|f| f.as_str()).map(str::trim).filter(|f| !f.is_empty());
    match named {
        // State, not instruction: this caller has the folders tool, not a shell.
        Some(q) => crate::open_resolved(|| {
            reg.find(q)
                .map(|f| f.db_path)
                .map_err(|e| if e.starts_with("No folder") { format!("No folder called \"{}\". The folders tool lists them.", q) } else { e })
        }),
        None => crate::get_db(),
    }
}

/// A query or thread given as an address names its own folder. `None` means
/// the arguments hold no address.
fn recall_by_address(reg: &Registry, args: &Value) -> Option<(String, bool)> {
    let field = |key: &str| args.get(key).and_then(|v| v.as_str()).and_then(parse_address);
    let opened = field("query");
    let thread = field("thread");
    let (folder, _) = opened.as_ref().or(thread.as_ref())?;
    let folder = folder.clone();
    let open = || -> Result<DatabaseManager, String> {
        crate::open_resolved(|| {
            reg.find(&folder)
                .map(|f| f.db_path)
                .map_err(|_| format!("No folder with the ID {}. The folders tool lists them.", folder))
        })
    };
    let db = match open() {
        Ok(db) => db,
        Err(e) => return Some((format!("Error: {}", e), true)),
    };
    if let Some((_, memory)) = opened {
        return Some(match db.get_by_uuid_prefix(&memory) {
            Ok(Some(entry)) => (format_entries(&[entry], true, &db), false),
            Ok(None) => (format!("Error: No memory {} in folder {}.", memory, folder), true),
            Err(e) => (format!("Error: {}", e), true),
        });
    }
    let (_, memory) = thread?;
    let mut rewritten = args.clone();
    rewritten["thread"] = json!(memory);
    Some(execute_recall(&db, &rewritten))
}

/// Exact words across every folder: the matching lines, newest first, each with
/// the address that opens its memory.
fn execute_grep_all(reg: &Registry, grep: &[String], window: &Window, args: &Value) -> (String, bool) {
    let terms = match geniuz::dig::Terms::parse(grep) {
        Ok(t) => t,
        Err(e) => return (format!("Error: {}", e), true),
    };
    let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(50).min(500) as usize;
    let (mut hits, mut searched, mut skipped) = (Vec::new(), 0, Vec::new());
    for f in reg.scan().folders {
        let found = DatabaseManager::open_existing(&f.db_path.to_string_lossy())
            .and_then(|db| db.grep_in(&terms, window, None, limit));
        match found {
            Ok(dig) => {
                searched += 1;
                for h in dig.hits {
                    let address = format!("{}:{}", f.short_id(), h.memory_uuid.chars().take(8).collect::<String>().to_uppercase());
                    hits.push((h.created_at, address, f.name.clone(), h.line));
                }
            }
            Err(_) => skipped.push(f.name),
        }
    }
    hits.sort_by(|a, b| b.0.cmp(&a.0));
    hits.truncate(limit);
    let mut lines: Vec<String> = hits
        .iter()
        .map(|(at, address, folder, line)| format!("{} | {} | {} | {}", address, folder, crate::shorten_ts(at), line))
        .collect();
    if !skipped.is_empty() {
        lines.push(format!("Couldn't read: {}", skipped.join(", ")));
    }
    lines.push(format!("searched: {} · grep \"{}\"", geniuz::dig::count(searched, "folder", "folders"), grep.join(" ")));
    (lines.join("\n"), false)
}

fn execute_recall_all(reg: &Registry, backend: Option<&dyn EmbeddingBackend>, args: &Value) -> (String, bool) {
    let window = match window_from(args) {
        Ok(w) => w,
        Err(e) => return (format!("Error: {}", e), true),
    };
    if args.get("thread").and_then(|t| t.as_str()).is_some_and(|t| !t.trim().is_empty()) {
        return ("Error: a thread lives in one folder, so all can't be combined with thread.".to_string(), true);
    }
    let grep: Vec<String> = match args.get("grep") {
        Some(Value::String(t)) => vec![t.clone()],
        Some(Value::Array(a)) => a.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    };
    if !grep.is_empty() {
        return execute_grep_all(reg, &grep, &window, args);
    }
    let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(10).min(100) as usize;
    let full = args.get("full").and_then(|f| f.as_bool()).unwrap_or(false);
    let query = args.get("query").and_then(|q| q.as_str()).map(str::trim).filter(|q| !q.is_empty());
    let search = match query {
        None => Search::Recent,
        Some(q) if backend.is_some() => Search::Semantic(q, backend),
        Some(q) => Search::Keyword(q),
    };
    let found = search_across(reg, &search, &window, limit);
    let mut lines: Vec<String> = found
        .hits
        .iter()
        .map(|h| {
            let score = h.entry.score.map(|s| format!(" ({:.3})", s)).unwrap_or_default();
            let line = format!("{} | {} | {} | {}{}", h.address(), h.folder.name, crate::shorten_ts(&h.entry.created_at), h.entry.gist, score);
            let content = full
                .then(|| DatabaseManager::open_existing(&h.folder.db_path.to_string_lossy()).ok()?.get_full_content(&h.entry.memory_uuid).ok().flatten())
                .flatten();
            match content {
                Some(c) => format!("{}\n{}\n", line, c),
                None => line,
            }
        })
        .collect();
    if !found.skipped.is_empty() {
        lines.push(format!("Couldn't read: {}", found.skipped.join(", ")));
    }
    lines.push(format!("searched: {} · {}", geniuz::dig::count(found.searched, "folder", "folders"), geniuz::dig::count(found.memories, "memory", "memories")));
    (lines.join("\n"), false)
}

fn call_tool(
    reg: &Registry,
    backend: Option<&dyn EmbeddingBackend>,
    name: &str,
    args: &Value,
) -> (String, bool) {
    if name == "folders" {
        return execute_folders(reg, args);
    }
    if !matches!(name, "keep" | "remember" | "recall" | "recall_recent") {
        return (format!("Unknown tool: {}", name), true);
    }
    if name == "recall" {
        // An address names one memory, so it wins over searching everything.
        if let Some(found) = recall_by_address(reg, args) {
            return found;
        }
        if args.get("all").and_then(|a| a.as_bool()).unwrap_or(false) {
            return execute_recall_all(reg, backend, args);
        }
    }
    // Writing never falls back to a folder nobody named.
    if matches!(name, "keep" | "remember")
        && args.get("folder").and_then(|f| f.as_str()).map(str::trim).is_none_or(str::is_empty)
    {
        return ("Error: folder is required: name the folder to keep this in. The folders tool lists them.".to_string(), true);
    }
    let db = match open_for_call(reg, args) {
        Ok(db) => db,
        Err(e) => return (format!("Error: {}", e), true),
    };
    match name {
        "keep" | "remember" => {
            let (text, is_error) = execute_keep(&db, backend, args);
            if is_error {
                return (text, is_error);
            }
            // Say where it went: a memory can't be moved, so this is worth knowing.
            let here = std::fs::canonicalize(&db.db_path).ok();
            let folder = reg.scan().folders.into_iter().find(|f| std::fs::canonicalize(&f.db_path).ok() == here);
            (folder.map(|f| format!("{} In folder: {}.", text, f.name)).unwrap_or(text), false)
        }
        "recall" | "recall_recent" => {
            let (text, is_error) = if name == "recall" {
                execute_recall(&db, args)
            } else {
                execute_recall_recent(&db, args)
            };
            // "Nothing here" mustn't read as "nothing anywhere".
            if !is_error && db.count().unwrap_or(1) == 0 {
                let here = std::fs::canonicalize(&db.db_path).ok();
                let elsewhere = reg
                    .scan()
                    .folders
                    .iter()
                    .filter(|f| std::fs::canonicalize(&f.db_path).ok() != here)
                    .filter_map(|f| {
                        let (count, last) = f.stats().ok()?;
                        (count > 0).then(|| (last, format!("{} ({})", f.name, count)))
                    })
                    .collect::<Vec<_>>();
                // Newest first, so the few named are the live ones.
                let mut elsewhere = elsewhere;
                elsewhere.sort_by(|a, b| b.0.cmp(&a.0));
                let elsewhere: Vec<String> = elsewhere.into_iter().map(|(_, s)| s).collect();
                if !elsewhere.is_empty() {
                    // Name a few, not hundreds: the folders tool finds the rest.
                    const NAMED: usize = 8;
                    let shown = elsewhere.iter().take(NAMED).cloned().collect::<Vec<_>>().join(", ");
                    let more = if elsewhere.len() > NAMED {
                        format!(", and {} more (the folders tool finds them)", elsewhere.len() - NAMED)
                    } else {
                        String::new()
                    };
                    return (format!("This folder is empty. Other folders hold memories: {}{}.", shown, more), false);
                }
            }
            (text, is_error)
        }
        _ => (format!("Unknown tool: {}", name), true),
    }
}

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
    let found: Vec<&str> = dig.hits.iter().map(|h| h.line.as_str()).collect();
    let own: Vec<String> = dig.hits.iter().map(|h| h.memory_uuid.clone()).collect();
    lines.extend(geniuz::dig::Harvest::of(&found, &terms.describe(), &own).resolving(db).lines());
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
    let mut out = format_entries(&t.entries, full, db);
    for line in db.harvest(&t.entries, "").lines() {
        out.push_str(&format!("\n{line}"));
    }
    (format!("{out}\n{scope}"), false)
}

/// The closing scope line (search feature 5): what was searched, so an empty
/// answer carries its reach. A count that cannot be read is left out.
fn scope(db: &DatabaseManager, window: &Window, mode: &str) -> String {
    match db.count_in(window) {
        Ok(n) => geniuz::window::scope_line(None, n, window, mode),
        Err(_) => format!("searched: {mode}"),
    }
}

fn format_entries(entries: &[MemoryEntry], full: bool, db: &DatabaseManager) -> String {
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
fn entry_line(e: &MemoryEntry) -> String {
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

    /// A memory with a fixed time, written straight to the table: only a test may
    /// choose a memory's time, and the library offers no way to (no backdating).
    /// Returns the short ID, as `keep` does; `parent` is a short ID too.
    fn keep_dated(db: &DatabaseManager, content: &str, gist: &str, parent: Option<&str>, at: &str) -> String {
        let uuid = uuid::Uuid::new_v4().to_string().to_uppercase();
        let payload = json!({ "content": content, "gist": gist }).to_string();
        let conn = rusqlite::Connection::open(&db.db_path).unwrap();
        let parent: Option<String> = parent.map(|p| {
            conn.query_row("SELECT memory_uuid FROM memories WHERE memory_uuid LIKE ?1 || '%'", [p], |r| r.get(0)).unwrap()
        });
        conn.execute(
            "INSERT INTO memories (memory_uuid, payload, created_at, parent_uuid) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![uuid, payload, at, parent],
        )
        .unwrap();
        uuid[..8].to_string()
    }

    fn entry(parent: Option<&str>, score: Option<f32>) -> MemoryEntry {
        MemoryEntry {
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
        keep_dated(&db, "an old note", "old", None, "2020-01-01 00:00:00");
        let (out, is_err) = execute_recall_recent(&db, &json!({"since": "7d"}));
        assert!(!is_err, "{out}");
        assert!(out.ends_with("searched: 0 memories · since 7d · recent"), "{out}");
        let (out, _) = execute_recall_recent(&db, &json!({}));
        assert!(out.ends_with("searched: 1 memory · all time · recent"), "{out}");
        let (out, is_err) = execute_recall_recent(&db, &json!({"since": "someday"}));
        assert!(is_err && out.contains("--since `someday`"), "{out}");
    }

    /// Every answer ends with where to go next: recurring names not asked
    /// for, and the pointers the hits cite, just before the scope.
    #[test]
    fn an_answer_ends_with_its_harvest_then_its_scope() {
        let dir = tempfile::tempdir().unwrap();
        let db = DatabaseManager::new(dir.path().join("memory.db").to_str().unwrap()).unwrap();
        db.keep("Review notes\nThe PR went to Devin, see EEEEEEEE:87C1FADC.", "reviews", None).unwrap();
        db.keep("More review\nAsked Devin again.", "reviews again", None).unwrap();
        let (out, _) = execute_recall_recent(&db, &json!({}));
        let lines: Vec<&str> = out.lines().collect();
        let n = lines.len();
        assert_eq!(lines[n - 3], "names seen: Devin (2)", "{out}");
        assert_eq!(lines[n - 2], "pointers not found here: EEEEEEEE:87C1FADC", "{out}");
        assert!(lines[n - 1].starts_with("searched: "), "{out}");
    }

    #[test]
    fn a_pointer_to_a_memory_here_is_offered_as_live() {
        let dir = tempfile::tempdir().unwrap();
        let db = DatabaseManager::new(dir.path().join("memory.db").to_str().unwrap()).unwrap();
        let first = db.keep("the original", "first", None).unwrap().uuid;
        let id = first[..8].to_uppercase();
        db.keep(&format!("see {id}"), "second", None).unwrap();
        let (out, _) = execute_recall(&db, &json!({ "query": "second", "limit": 1 }));
        assert!(!out.contains("not found here"), "{out}");
    }

    /// recall with `grep` returns lines, and the terms can be a string or a list.
    #[test]
    fn recall_with_grep_returns_the_matching_lines() {
        let dir = tempfile::tempdir().unwrap();
        let db = DatabaseManager::new(dir.path().join("memory.db").to_str().unwrap()).unwrap();
        db.keep("Roundup\nCubic reviewed the PR.\nDevin filed issues.", "reviews", None).unwrap();
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
        let root = keep_dated(&db, "the question", "question", None, "2026-09-01 00:00:00");
        let reply = keep_dated(&db, "the answer", "answer", Some(&root), "2026-09-02 00:00:00");
        keep_dated(&db, "elsewhere", "elsewhere", None, "2026-09-03 00:00:00");
        let (out, is_err) = execute_recall(&db, &json!({"thread": reply}));
        assert!(!is_err, "{out}");
        let q = out.find("question").unwrap();
        let a = out.find(&format!("answer <- {root}")).unwrap();
        assert!(q < a, "root first: {out}");
        assert!(!out.contains("elsewhere"), "{out}");
        assert!(out.ends_with(&format!("searched: thread {reply} · root {root} · 2 memories · all time · oldest first")), "{out}");
        let (out, is_err) = execute_recall(&db, &json!({}));
        assert!(!is_err && out.contains("elsewhere"), "no query means recent: {out}");
    }

    #[test]
    fn keep_needs_a_gist_and_spaces_alone_dont_count() {
        let dir = tempfile::tempdir().unwrap();
        let reg = Registry::new(dir.path());
        reg.ensure_default().unwrap();
        for args in [json!({ "folder": "My memory", "content": "x" }), json!({ "folder": "My memory", "content": "x", "gist": "   " })] {
            let (out, is_error) = call_tool(&reg, None, "keep", &args);
            assert!(is_error && out.contains("gist"), "{out}");
        }
        assert_eq!(reg.find("My memory").unwrap().stats().unwrap().0, 0, "nothing kept");
    }

    #[test]
    fn keep_needs_a_named_folder_and_never_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let reg = Registry::new(dir.path());
        reg.ensure_default().unwrap();
        for args in [json!({ "content": "x", "gist": "g" }), json!({ "content": "x", "gist": "g", "folder": "  " })] {
            let (out, is_error) = call_tool(&reg, None, "keep", &args);
            assert!(is_error && out.contains("folder is required"), "{out}");
        }
        assert_eq!(reg.find("My memory").unwrap().stats().unwrap().0, 0, "nothing kept by fallback");
        let keep = tool_definitions()["tools"].as_array().unwrap().iter().find(|t| t["name"] == "keep").unwrap().clone();
        assert!(keep["inputSchema"]["required"].as_array().unwrap().contains(&json!("folder")), "{keep}");
    }

    #[test]
    fn the_listed_tools_are_keep_recall_and_folders() {
        let defs = tool_definitions();
        let names: Vec<_> = defs["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["keep", "recall", "folders"]);
        assert!(defs["tools"][0]["inputSchema"]["properties"]["folder"].is_object());
        assert!(defs["tools"][2]["inputSchema"]["properties"]["folder"].is_null());
    }

    #[test]
    fn a_named_folder_is_used_and_a_wrong_name_is_an_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let reg = Registry::new(&home);
        let work = reg.create("Work", "", None).unwrap();

        let args = json!({ "folder": "work", "content": "in work", "gist": "g" });
        let (text, is_error) = call_tool(&reg, None, "keep", &args);
        assert!(!is_error, "{}", text);
        assert_eq!(work.stats().unwrap().0, 1);

        let (_, is_error) = call_tool(&reg, None, "remember", &json!({ "folder": "work", "content": "alias", "gist": "g" }));
        assert!(!is_error);
        assert_eq!(work.stats().unwrap().0, 2);

        let (text, is_error) = call_tool(&reg, None, "keep", &json!({ "folder": "wrok", "content": "x", "gist": "g" }));
        assert!(is_error);
        assert!(text.contains("No folder"), "{}", text);
        assert_eq!(work.stats().unwrap().0, 2);

        let (text, _) = call_tool(&reg, None, "recall", &json!({ "folder": "Work" }));
        assert!(text.contains("alias") || text.contains("g"), "{}", text);
        let (text, _) = call_tool(&reg, None, "folders", &json!({}));
        assert!(text.contains("Work"), "{}", text);
    }

    #[test]
    fn the_newest_memory_comes_first_within_the_same_second() {
        let dir = tempfile::tempdir().unwrap();
        let db = DatabaseManager::new(dir.path().join("memory.db").to_str().unwrap()).unwrap();
        for gist in ["one", "two", "three", "four", "five"] {
            keep_dated(&db, "body", gist, None, "2026-09-01 00:00:00");
        }
        let (out, _) = execute_recall(&db, &json!({ "limit": 1 }));
        assert!(out.contains("five"), "{out}");
    }

    #[test]
    fn folders_can_create_a_folder() {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let reg = Registry::new(&home);
        let (out, is_err) = call_tool(&reg, None, "folders", &json!({ "create": "Work", "description": "Notes for work" }));
        assert!(!is_err && out.contains("Created Work"), "{out}");
        assert_eq!(reg.find("Work").unwrap().description, "Notes for work");
        let (out, is_err) = call_tool(&reg, None, "folders", &json!({ "create": "Work" }));
        assert!(is_err, "a second Work is refused: {out}");
    }

    /// A folder's writer chooses its face from a chat: at creation, or later by name.
    #[test]
    fn folders_can_choose_and_clear_a_face() {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let reg = Registry::new(&home);
        let (out, is_err) = call_tool(&reg, None, "folders", &json!({ "create": "Piste", "face": "⛷️" }));
        assert!(!is_err, "{out}");
        assert_eq!(reg.find("Piste").unwrap().face, "⛷️");
        let (out, is_err) = call_tool(&reg, None, "folders", &json!({ "name": "Piste", "face": "🎿" }));
        assert!(!is_err && out.starts_with("🎿 Piste ("), "{out}");
        let (out, is_err) = call_tool(&reg, None, "folders", &json!({ "name": "Piste", "face": "a word" }));
        assert!(is_err && out.contains("one emoji"), "{out}");
        let (out, is_err) = call_tool(&reg, None, "folders", &json!({ "name": "Piste", "face": "P" }));
        assert!(is_err && out.contains("no letters"), "{out}");
        assert_eq!(reg.find("Piste").unwrap().face, "🎿", "a refused face changes nothing");
        let (out, _) = call_tool(&reg, None, "folders", &json!({ "name": "Piste", "face": "" }));
        assert!(out.starts_with("Piste ("), "{out}");
        // A face that won't do stops the folder being made at all.
        let (out, is_err) = call_tool(&reg, None, "folders", &json!({ "create": "Finn", "face": "not a face" }));
        assert!(is_err, "{out}");
        assert!(reg.find("Finn").is_err(), "no folder is left behind");
    }

    #[test]
    fn recall_all_with_full_returns_the_whole_memory() {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let reg = Registry::new(&home);
        let work = reg.create("Work", "", None).unwrap();
        let db = DatabaseManager::new(&work.db_path.to_string_lossy()).unwrap();
        db.keep("the whole body", "the gist", None).unwrap();
        let (out, _) = call_tool(&reg, None, "recall", &json!({ "all": true, "full": true }));
        assert!(out.contains("the gist") && out.contains("the whole body"), "{out}");
        let (out, _) = call_tool(&reg, None, "recall", &json!({ "all": true }));
        assert!(out.contains("the gist") && !out.contains("the whole body"), "{out}");
    }

    #[test]
    fn install_replaces_any_spelling_of_the_entry() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config = tmp.path().join("claude_desktop_config.json");
        std::fs::write(&config, r#"{"mcpServers":{"geniuz":{"command":"/gone"},"other":{"command":"x"}}}"#).unwrap();
        install_to_path(&config, "/Applications/Geniuz.app/geniuz", &serde_json::Map::new()).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
        let servers = v["mcpServers"].as_object().unwrap();
        assert_eq!(servers.len(), 2, "{servers:?}");
        assert!(servers.contains_key("other"));
        assert_eq!(servers["Geniuz"]["command"], "/Applications/Geniuz.app/geniuz");
    }

    #[test]
    fn a_temporary_location_is_recognized() {
        assert!(is_temporary_location("/private/var/folders/h6/T/AppTranslocation/ABC/d/Geniuz.app/Contents/Resources/geniuz"));
        assert!(is_temporary_location("/Volumes/Geniuz/Geniuz.app/Contents/Resources/geniuz"));
        assert!(!is_temporary_location("/Applications/Geniuz.app/Contents/Resources/geniuz"));
        assert!(!is_temporary_location("/usr/local/bin/geniuz"));
    }

    #[test]
    fn an_address_opens_its_own_folder_from_anywhere() {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let reg = Registry::new(&home);
        let work = reg.create("Work", "", None).unwrap();
        let db = DatabaseManager::new(&work.db_path.to_string_lossy()).unwrap();
        let id = db.keep("the whole body", "the gist", None).unwrap().uuid;
        let address = format!("{}:{}", work.short_id(), &id[..8]);

        let (out, is_err) = call_tool(&reg, None, "recall", &json!({ "query": address }));
        assert!(!is_err && out.contains("the whole body"), "{out}");

        let (out, is_err) = call_tool(&reg, None, "recall", &json!({ "thread": address }));
        assert!(!is_err && out.contains("the gist"), "{out}");

        let wrong = format!("{}:00000000", work.short_id());
        let (out, is_err) = call_tool(&reg, None, "recall", &json!({ "query": wrong }));
        assert!(is_err && out.contains("No memory"), "{out}");

        let (out, is_err) = call_tool(&reg, None, "recall", &json!({ "all": true, "query": "body" }));
        assert!(!is_err && out.contains(&address), "{out}");
    }

    fn registry_with_work() -> (tempfile::TempDir, Registry, geniuz::folders::Folder) {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let reg = Registry::new(&home);
        let work = reg.create("Work", "Client work.\nNo passwords here.", None).unwrap();
        (tmp, reg, work)
    }

    #[test]
    fn folders_shows_a_rule_on_a_descriptions_second_line() {
        let (_tmp, reg, _work) = registry_with_work();
        let (text, _) = call_tool(&reg, None, "folders", &json!({}));
        assert!(text.contains("Client work. No passwords here."), "{text}");
    }

    #[test]
    fn grep_across_folders_searches_and_shows_addresses() {
        let (_tmp, reg, work) = registry_with_work();
        let db = DatabaseManager::new(&work.db_path.to_string_lossy()).unwrap();
        let id = db.keep("first line\nThe Cubic review passed.", "g", None).unwrap().uuid;

        let (text, is_err) = call_tool(&reg, None, "recall", &json!({ "all": true, "grep": "cubic" }));
        assert!(!is_err, "{text}");
        assert!(text.contains("The Cubic review passed."), "{text}");
        assert!(text.contains(&format!("{}:{}", work.short_id(), &id[..8])), "{text}");
        assert!(!text.contains("first line"), "only the matching line: {text}");

        let (text, is_err) = call_tool(&reg, None, "recall", &json!({ "all": true, "thread": &id[..8] }));
        assert!(is_err, "{text}");
    }

    #[test]
    fn an_address_opens_even_when_all_is_set() {
        let (_tmp, reg, work) = registry_with_work();
        let db = DatabaseManager::new(&work.db_path.to_string_lossy()).unwrap();
        let id = db.keep("the whole body", "g", None).unwrap().uuid;
        let address = format!("{}:{}", work.short_id(), &id[..8]);
        let (text, _) = call_tool(&reg, None, "recall", &json!({ "query": address, "all": true }));
        assert!(text.contains("the whole body"), "{text}");
    }

    #[test]
    fn an_empty_folder_says_where_the_memories_are() {
        let (_tmp, reg, work) = registry_with_work();
        let empty = reg.create("Empty", "", None).unwrap();
        DatabaseManager::new(&work.db_path.to_string_lossy()).unwrap().keep("x", "g", None).unwrap();
        let (text, _) = call_tool(&reg, None, "recall", &json!({ "folder": empty.name }));
        assert!(text.contains("Work (1)"), "{text}");
    }

    #[test]
    fn keep_says_which_folder_it_went_to() {
        let (_tmp, reg, _work) = registry_with_work();
        let (text, _) = call_tool(&reg, None, "keep", &json!({ "folder": "Work", "content": "x", "gist": "y" }));
        assert!(text.contains("In folder: Work."), "{text}");
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
    let reg = Registry::from_env();
    // Fail early on a broken setup, but hold nothing: each call resolves afresh.
    if let Err(e) = crate::get_db() {
        eprintln!("[geniuz] Couldn't open your folder: {}", e);
    }

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
                    },
                    "instructions": INSTRUCTIONS
                }))
            }

            "notifications/initialized" => continue,

            "tools/list" => success(id, tool_definitions()),

            "tools/call" => {
                let tool_name = request.params.get("name")
                    .and_then(|n| n.as_str()).unwrap_or("");
                let arguments = request.params.get("arguments")
                    .cloned().unwrap_or(json!({}));

                let (text, is_error) = call_tool(&reg, backend.as_deref(), tool_name, &arguments);

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

/// macOS runs a quarantined app from a random path that disappears when it quits,
/// and a disk image disappears when it's ejected. A connection saved there breaks.
fn is_temporary_location(path: &str) -> bool {
    path.contains("/AppTranslocation/") || path.starts_with("/Volumes/")
}

/// The one entry Geniuz keeps in a Claude Desktop config. Older versions and the
/// menubar app wrote other spellings, so match them all when reading or replacing.
const SERVER_KEY: &str = "Geniuz";

fn is_server_key(key: &str) -> bool {
    key.eq_ignore_ascii_case(SERVER_KEY)
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
    if is_temporary_location(&binary) {
        return Err("Geniuz is running from a temporary location, so a connection saved now would break. \
Drag Geniuz to Applications, open it from there, and run this again.".to_string());
    }
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
            "Couldn't install the MCP config: no writable location:\n  {}",
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
    lines.push("  Your Claude will have: keep, recall, folders".to_string());

    if !errors.is_empty() {
        lines.push(String::new());
        lines.push("  Note: some locations were not writable (this is usually fine):".to_string());
        for e in &errors {
            lines.push(format!("    {}", e));
        }
    }

    // The folder Claude will use when it names none. Claude starts Geniuz with no
    // -f, no GENIUZ_DB and not from this directory, so only the default applies:
    // resolved the same way here, not as this command would.
    if let Ok(path) = geniuz::folders::Registry::from_env().resolve(None, None, None) {
        if path.exists() {
            if let Ok(db) = geniuz::db::DatabaseManager::open_existing(&path.to_string_lossy()) {
                let count = db.count().unwrap_or(0);
                if count > 0 {
                    let name = geniuz::folders::identity_at(&path)
                        .map(|f| format!("\"{}\"", f.name))
                        .unwrap_or_else(|| "Your folder".to_string());
                    lines.push(String::new());
                    lines.push(format!("  {} has {} memories, and Claude will find them.", name, count));
                }
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
    if let Some(servers) = config["mcpServers"].as_object_mut() {
        servers.retain(|key, _| !is_server_key(key));
    }
    config["mcpServers"][SERVER_KEY] = entry;

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

        let entry = config.get("mcpServers")
            .and_then(|s| s.as_object())
            .and_then(|s| s.iter().find(|(key, _)| is_server_key(key)))
            .map(|(_, v)| v);
        let command = entry.and_then(|e| e.get("command")).and_then(|c| c.as_str());
        // A path that's gone means Claude can't start Geniuz, whatever the config says.
        let missing = command.is_some_and(|c| c.contains('/') && !std::path::Path::new(c).exists());
        let installed = entry.is_some() && !missing;
        if installed { any_installed = true; }

        lines.push(format!("Config: {}", config_file.display()));
        let state = if installed {
            "installed"
        } else if missing {
            "broken: its program is gone. Run 'geniuz mcp install' to repair it"
        } else {
            "not installed"
        };
        lines.push(format!("  Geniuz: {}", state));
        if let Some(cmd) = command {
            lines.push(format!("  Binary: {}", cmd));
        }
    }

    if !any_present {
        return Ok(format!(
            "Claude Desktop config not found at any known location:\n  {}\n\nRun 'geniuz mcp install' first.",
            paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n  ")
        ));
    }

    // The folder a memory goes to when it names none: the one resolve picks.
    match crate::resolve_db_path() {
        Err(e) => lines.push(format!("Folder: {}", e)),
        Ok(path) if !path.exists() => {
            lines.push("Folder: not created yet (will be created on first keep)".to_string());
        }
        Ok(path) => {
            // Named from the database itself: a status check changes nothing on disk.
            let name = geniuz::folders::identity_at(&path)
                .map(|f| format!("{} ({})", f.name, f.short_id()))
                .unwrap_or_else(|| path.display().to_string());
            match crate::get_db() {
                Ok(db) => {
                    let count = db.count().unwrap_or(0);
                    let embeddings = db.embedding_count().unwrap_or(0);
                    lines.push(format!("Folder: {} ({} memories, {}/{} embedded)", name, count, embeddings, count));
                }
                Err(e) => lines.push(format!("Folder: {} (can't be opened: {})", name, e)),
            }
        }
    }

    if !any_installed {
        lines.push(String::new());
        lines.push("Run 'geniuz mcp install' to add Geniuz to Claude Desktop.".to_string());
    }

    Ok(lines.join("\n"))
}

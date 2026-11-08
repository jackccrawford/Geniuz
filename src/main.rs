mod adapter;
mod cli;
mod folder_cli;
mod mcp;
mod tui;

use clap::FromArgMatches;
use cli::{Cli, Command};
use geniuz::db;
use std::path::PathBuf;

/// The command `watch --exec` runs for one memory, and the variables it runs
/// with. A memory's words are data: they reach the shell as environment
/// variables, never as part of the command it parses, so an apostrophe cannot
/// break the command and shell syntax in a memory cannot run. Every byte
/// arrives intact.
fn watch_exec_command(template: &str, e: &geniuz::db::SignalEntry) -> (String, Vec<(&'static str, String)>) {
    let uuid_short = &e.memory_uuid[..8.min(e.memory_uuid.len())];
    let parent_short = e.parent_uuid.as_deref().map(|p| &p[..8.min(p.len())]).unwrap_or("");
    let content = e.content.as_deref().unwrap_or("");
    let json = serde_json::json!({
        "uuid": uuid_short,
        "gist": &e.gist,
        "content": content,
        "created_at": &e.created_at,
        "parent": parent_short,
    });
    let cmd = template
        .replace("{uuid}", uuid_short)
        .replace("{created_at}", &e.created_at)
        .replace("{parent}", parent_short)
        .replace("{gist}", "${GENIUZ_GIST}")
        .replace("{content}", "${GENIUZ_CONTENT}")
        .replace("{json}", "${GENIUZ_JSON}");
    let vars = vec![
        ("GENIUZ_GIST", e.gist.clone()),
        ("GENIUZ_CONTENT", content.to_string()),
        ("GENIUZ_JSON", json.to_string()),
    ];
    (cmd, vars)
}

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

use geniuz::data_dir;

fn default_db_path() -> PathBuf {
    data_dir().join("memory.db")
}


/// Locate and launch the Geniuz Dashboard binary.
///
/// On macOS, prefers the `geniuz://` URL scheme (registered by the dashboard
/// .app's Info.plist) which the OS routes via Launch Services. Falls back to
/// known install paths and a dev fallback for cargo builds.
///
/// On Linux and Windows, searches known install paths for the dashboard
/// binary and spawns it as a subprocess. Returns an error string if the
/// dashboard isn't installed.
fn launch_dashboard() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // Try the URL scheme first — works regardless of install location.
        let status = std::process::Command::new("open")
            .arg("geniuz://open")
            .status();
        if let Ok(s) = status {
            if s.success() {
                return Ok(());
            }
        }
        // Fallback: open the .app directly from known locations.
        for path in [
            "/Applications/Geniuz.app/Contents/Resources/Geniuz.app",
            "/Applications/Geniuz.app", // if the menubar host bundle is the dashboard
        ] {
            if std::path::Path::new(path).exists() {
                let _ = std::process::Command::new("open").arg(path).spawn()
                    .map_err(|e| format!("Failed to launch dashboard: {e}"))?;
                return Ok(());
            }
        }
        return Err("Geniuz Dashboard not found. Install Geniuz.dmg from https://geniuz.life".into());
    }
    #[cfg(target_os = "linux")]
    {
        let candidates = [
            "/usr/bin/geniuz-dashboard",
            "/usr/local/bin/geniuz-dashboard",
        ];
        for path in candidates {
            if std::path::Path::new(path).exists() {
                std::process::Command::new(path).spawn()
                    .map_err(|e| format!("Failed to launch dashboard: {e}"))?;
                return Ok(());
            }
        }
        return Err("Geniuz Dashboard not found. Install the .deb/.rpm/.AppImage from https://geniuz.life".into());
    }
    #[cfg(target_os = "windows")]
    {
        // Look in standard install dirs. The NSIS installer places the
        // dashboard under %ProgramFiles% (system install) or %LOCALAPPDATA%
        // (per-user install).
        let candidates = [
            std::env::var("ProgramFiles").ok().map(|p| format!("{p}\\Geniuz\\geniuz-dashboard.exe")),
            std::env::var("LOCALAPPDATA").ok().map(|p| format!("{p}\\Programs\\Geniuz\\geniuz-dashboard.exe")),
        ];
        for path in candidates.into_iter().flatten() {
            if std::path::Path::new(&path).exists() {
                std::process::Command::new(&path).spawn()
                    .map_err(|e| format!("Failed to launch dashboard: {e}"))?;
                return Ok(());
            }
        }
        return Err("Geniuz Dashboard not found. Install Geniuz-Setup.exe from https://geniuz.life".into());
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err("Geniuz Dashboard is not available on this platform".into())
    }
}

/// The `--in` flag, set once when the command line is parsed.
static IN_FOLDER: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// Which database this command uses. One resolver for everything in this binary.
/// One specific memory file. `GENIUZ_STATION` is its older name, still honored
/// because older scripts may set it.
fn explicit_db_env() -> Option<String> {
    std::env::var("GENIUZ_DB").or_else(|_| std::env::var("GENIUZ_STATION")).ok()
}

pub fn resolve_db_path() -> Result<PathBuf, String> {
    let flag = IN_FOLDER.get().and_then(|f| f.as_deref());
    let env_station = explicit_db_env();
    let cwd = std::env::current_dir().ok();
    geniuz::folders::Registry::from_env().resolve(flag, env_station.as_deref(), cwd.as_deref())
}

pub fn get_db() -> Result<db::DatabaseManager, String> {
    // An explicit file path may name a store to be created, as it always could.
    if explicit_db_env().is_some_and(|s| !s.is_empty()) && IN_FOLDER.get().and_then(|f| f.as_deref()).is_none() {
        return db::DatabaseManager::new(&resolve_db_path()?.to_string_lossy());
    }
    open_resolved(resolve_db_path)
}

/// Open the folder `find` names. If it moved in between, ask once more: it will
/// be at its new path. Never create a store as a side effect of choosing a folder.
pub fn open_resolved(find: impl Fn() -> Result<PathBuf, String>) -> Result<db::DatabaseManager, String> {
    match db::DatabaseManager::open_existing(&find()?.to_string_lossy()) {
        Ok(db) => Ok(db),
        Err(_) => db::DatabaseManager::open_existing(&find()?.to_string_lossy()),
    }
}

/// Convert a UTC timestamp string from SQLite into a human-readable
/// local-time string. SQLite stores everything as UTC ("2026-04-18 22:34:01"),
/// but customers live in their own timezone — showing raw UTC to a Pacific
/// user makes a 6 PM memory look like it was saved at 2 AM next day.
///
/// Input: SQLite timestamp string like "2026-04-18 22:34:01" (UTC, space-separated).
/// Output: "YYYY-MM-DD HH:MM" in the host machine's local timezone.
/// If parsing fails (malformed timestamp), falls back to the original truncation.
/// `recall --grep TERM…` (search feature 2): the matching lines, newest
/// memory first, each tagged with the memory it came from.
fn recall_grep(
    db: &db::DatabaseManager, terms: &geniuz::dig::Terms, window: &geniuz::window::Window,
    thread: Option<&str>, limit: usize, json: bool,
) -> Result<String, String> {
    let dig = db.grep_in(terms, window, thread, limit)?;
    // The pool the dig drew from: the thread, or the whole store.
    let (pool, count) = match thread {
        Some(id) => {
            let t = db.thread_in(id, window, 1)?;
            let label = format!("thread {} · root {}", id.trim().to_uppercase(), &t.root[..8.min(t.root.len())]);
            (Some(label), Some(t.total))
        }
        None => (None, db.count_in(window).ok()),
    };
    let scope = match count {
        Some(n) => geniuz::window::scope_line(pool.as_deref(), n, window, &dig.mode(terms)),
        None => format!("searched: {}", dig.mode(terms)),
    };
    if json {
        let hits: Vec<serde_json::Value> = dig.hits.iter().map(|h| serde_json::json!({
            "uuid": &h.memory_uuid[..8.min(h.memory_uuid.len())], "created_at": h.created_at, "line": h.line,
        })).collect();
        return Ok(serde_json::to_string_pretty(&serde_json::json!({
            "ok": true, "action": "grep", "lines": dig.lines, "memories": dig.memories,
            "count": hits.len(), "hits": hits, "searched": scope,
        })).unwrap());
    }
    if dig.hits.is_empty() {
        return Ok(format!("No lines found.\n{scope}"));
    }
    let mut lines: Vec<String> = dig.hits.iter().map(|h| format!(
        "{} · {} · {}", &h.memory_uuid[..8.min(h.memory_uuid.len())], shorten_ts(&h.created_at), h.line,
    )).collect();
    let found: Vec<&str> = dig.hits.iter().map(|h| h.line.as_str()).collect();
    let own: Vec<String> = dig.hits.iter().map(|h| h.memory_uuid.clone()).collect();
    lines.extend(geniuz::dig::Harvest::of(&found, &terms.describe(), &own).resolving(db).lines());
    lines.push(scope);
    Ok(lines.join("\n"))
}

/// `recall --thread ID` (search feature 6): the whole thread, oldest first,
/// in the same line shape as every other answer, closed by its scope.
fn recall_thread(
    db: &db::DatabaseManager, id: &str, window: &geniuz::window::Window,
    limit: usize, full: bool, json: bool,
) -> Result<String, String> {
    let mut thread = db.thread_in(id, window, limit)?;
    if full {
        for e in &mut thread.entries {
            e.content = db.get_full_content(&e.memory_uuid).ok().flatten();
        }
    }
    let label = format!("thread {} · root {}", id.trim().to_uppercase(), &thread.root[..8.min(thread.root.len())]);
    let shown = if thread.entries.len() < thread.total {
        format!("oldest {} shown", thread.entries.len())
    } else {
        "oldest first".to_string()
    };
    let scope = geniuz::window::scope_line(Some(&label), thread.total, window, &shown);

    if json {
        let data: Vec<serde_json::Value> = thread.entries.iter().map(|e| {
            let mut v = serde_json::json!({ "uuid": &e.memory_uuid[..8], "gist": e.gist, "created_at": e.created_at });
            if let Some(ref p) = e.parent_uuid { v["parent"] = serde_json::json!(&p[..8.min(p.len())]); }
            if let Some(ref c) = e.content { v["content"] = serde_json::json!(c); }
            v
        }).collect();
        return Ok(serde_json::to_string_pretty(&serde_json::json!({
            "ok": true, "action": "thread", "root": &thread.root[..8.min(thread.root.len())],
            "total": thread.total, "count": data.len(), "memories": data,
            "searched": { "thread": id, "since": window.since_label, "until": window.until_label },
        })).unwrap());
    }

    if thread.entries.is_empty() {
        return Ok(format!("No memories in that thread inside the window.\n{scope}"));
    }
    let mut lines = Vec::new();
    for e in &thread.entries {
        let parent = e.parent_uuid.as_deref()
            .map(|p| format!(" <- {}", &p[..8.min(p.len())])).unwrap_or_default();
        lines.push(format!("{} | {} | {}{}", &e.memory_uuid[..8], shorten_ts(&e.created_at), e.gist, parent));
        if let Some(ref content) = e.content {
            for line in content.lines() {
                lines.push(format!("           {}", line));
            }
            lines.push(String::new());
        }
    }
    lines.extend(db.harvest(&thread.entries, "").lines());
    lines.push(scope);
    Ok(lines.join("\n"))
}

/// The scope line for a CLI answer (search feature 5). A count that cannot
/// be read is left out rather than guessed.
fn scope_text(db: &db::DatabaseManager, window: &geniuz::window::Window, mode: &str) -> String {
    match db.count_in(window) {
        Ok(n) => geniuz::window::scope_line(None, n, window, mode),
        Err(_) => format!("searched: {} · {mode}",
            if window.is_all() { "all time".to_string() } else { window.describe() }),
    }
}

fn scope_json(db: &db::DatabaseManager, window: &geniuz::window::Window, mode: &str) -> serde_json::Value {
    serde_json::json!({
        "memories": db.count_in(window).ok(),
        "since": window.since_label, "until": window.until_label,
        "mode": mode,
    })
}

/// `recall --all`: every folder, merged, each result labelled with its address.
fn recall_all(
    query: Option<&str>, keyword: bool, full: bool,
    window: &geniuz::window::Window, limit: usize, json: bool,
) -> Result<String, String> {
    use geniuz::folders::{search_across, Registry, Search};
    let reg = Registry::from_env();
    let backend = match query {
        Some(_) if !keyword => geniuz::embedding::create_backend().ok(),
        _ => None,
    };
    let (search, mode) = match query {
        None => (Search::Recent, "recent".to_string()),
        Some(q) if keyword || backend.is_none() => (Search::Keyword(q), format!("keyword \"{q}\"")),
        Some(q) => (Search::Semantic(q, backend.as_deref()), geniuz::dig::semantic_mode(q)),
    };
    let found = search_across(&reg, &search, window, limit);
    let content = |hit: &geniuz::folders::Hit| -> Option<String> {
        if !full { return None; }
        db::DatabaseManager::open_existing(&hit.folder.db_path.to_string_lossy()).ok()?
            .get_full_content(&hit.entry.memory_uuid).ok().flatten()
    };

    if json {
        let data: Vec<_> = found.hits.iter().map(|h| {
            let mut v = serde_json::json!({
                "address": h.address(),
                "folder": h.folder.name,
                "gist": h.entry.gist,
                "created_at": h.entry.created_at,
            });
            if let Some(s) = h.entry.score { v["score"] = serde_json::json!(format!("{:.3}", s)); }
            if let Some(c) = content(h) { v["content"] = serde_json::json!(c); }
            v
        }).collect();
        return serde_json::to_string_pretty(&serde_json::json!({
            "ok": true, "action": "recall", "count": data.len(), "memories": data,
            "searched": { "folders": found.searched, "memories": found.memories, "mode": mode },
            "skipped": found.skipped,
        })).map_err(|e| e.to_string());
    }

    let mut lines = Vec::new();
    for h in &found.hits {
        let score = h.entry.score.map(|s| format!(" ({:.3})", s)).unwrap_or_default();
        lines.push(format!("{} | {} | {} | {}{}", h.address(), h.folder.name, shorten_ts(&h.entry.created_at), h.entry.gist, score));
        if let Some(c) = content(h) {
            lines.extend(c.lines().map(|l| format!("           {}", l)));
            lines.push(String::new());
        }
    }
    if found.hits.is_empty() {
        lines.push("No memories found.".to_string());
    }
    if !found.skipped.is_empty() {
        lines.push(format!("Couldn't read: {}", found.skipped.join(", ")));
    }
    lines.push(format!("searched: {} · {} · {}", geniuz::dig::count(found.searched, "folder", "folders"), geniuz::dig::count(found.memories, "memory", "memories"), mode));
    Ok(lines.join("\n"))
}

pub fn shorten_ts(ts: &str) -> String {
    use chrono::{DateTime, NaiveDateTime, Utc, Local};

    // The store stamps UTC as "2026-04-18 22:34:01" (db::NOW_UTC).
    // NaiveDateTime::parse_from_str treats it as unzoned, so we attach UTC
    // explicitly before converting to Local.
    if let Ok(naive) = NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S") {
        let utc: DateTime<Utc> = DateTime::from_naive_utc_and_offset(naive, Utc);
        return utc.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string();
    }
    // Fallback: no timezone conversion possible; keep the old truncation behavior.
    if ts.len() >= 16 { ts[..16].to_string() } else { ts.to_string() }
}

fn run(cli: Cli) -> Result<String, String> {
    let _ = IN_FOLDER.set(cli.in_folder.clone());
    match cli.command {
        Command::Skill => {
            Ok(include_str!("../skills/SKILL.md").to_string())
        }

        Command::Folder(cmd) => folder_cli::run(cmd),

        Command::Tui => {
            tui::run()?;
            Ok(String::new())
        }

        Command::Dashboard => {
            launch_dashboard()?;
            Ok(String::new())
        }

        Command::Capture { paths, split, gist_prefix, dry_run, force } => {
            // General mode: capture files and directories
            if paths.is_empty() {
                return Err("No files specified. Use 'geniuz capture <files...>'.".to_string());
            }

            // Prose-format allowlist. Non-prose files (CSS, HTML, JS, binaries) produce
            // noisy embeddings that pollute semantic recall across the whole store.
            // The embedding model is trained on paraphrase similarity for natural language,
            // not content-type classification — so our only honest gate is extension-based.
            // --force bypasses the check for users who know what they're doing.
            const PROSE_EXTENSIONS: &[&str] = &["md", "markdown", "txt", "rst", "org"];
            fn is_prose_ext(path: &std::path::Path) -> bool {
                path.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| PROSE_EXTENSIONS.iter().any(|p| p.eq_ignore_ascii_case(e)))
                    .unwrap_or(false)
            }

            let mut files: Vec<PathBuf> = Vec::new();
            let mut rejected: Vec<PathBuf> = Vec::new();
            // A directory is read one level deep: memories can't be removed, so a whole
            // tree is never taken in by accident. Subfolders are named, not passed over,
            // except hidden ones (.git, .obsidian), which are tool state, not documents.
            let mut subdirs: Vec<PathBuf> = Vec::new();
            for p in &paths {
                let path = PathBuf::from(p);
                if path.is_dir() {
                    match std::fs::read_dir(&path) {
                        Ok(entries) => {
                            for entry in entries.flatten() {
                                let ep = entry.path();
                                if ep.is_dir() {
                                    if !entry.file_name().to_string_lossy().starts_with('.') {
                                        subdirs.push(ep);
                                    }
                                } else if ep.is_file() {
                                    if force || is_prose_ext(&ep) {
                                        files.push(ep);
                                    } else {
                                        rejected.push(ep);
                                    }
                                }
                            }
                        }
                        Err(e) => eprintln!("[capture] Failed to read directory {}: {}", path.display(), e),
                    }
                } else if path.is_file() {
                    if force || is_prose_ext(&path) {
                        files.push(path);
                    } else {
                        rejected.push(path);
                    }
                } else {
                    eprintln!("[capture] Not found: {}", path.display());
                }
            }
            files.sort();
            subdirs.sort();

            if !subdirs.is_empty() {
                eprintln!("[capture] {} subfolder(s) not read (capture reads one level; name a subfolder to include it):", subdirs.len());
                for d in &subdirs {
                    eprintln!("  not read: {}/", d.display());
                }
            }

            if !rejected.is_empty() {
                eprintln!("[capture] {} file(s) refused (non-prose extensions; embeddings would be noisy):", rejected.len());
                for r in &rejected {
                    eprintln!("  skipped: {}", r.display());
                }
                eprintln!("  Accepted: {}", PROSE_EXTENSIONS.iter().map(|e| format!(".{}", e)).collect::<Vec<_>>().join(", "));
                eprintln!("  Use --force to capture them anyway.");
            }

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
                        Ok(result) => {
                            created += 1;
                            result.uuid
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

        Command::Keep { content, gist, parent, at, json } => {
            let at = at.as_deref().map(geniuz::window::moment).transpose()?;
            // Resolve content: @file or stdin
            let resolved = if content == "-" {
                use std::io::{IsTerminal, Read};
                // Fail fast if stdin is an interactive terminal — otherwise read_to_string
                // blocks indefinitely waiting for input, which is the silent-hang case when
                // someone types `geniuz remember -c -` without piping or redirecting stdin.
                if std::io::stdin().is_terminal() {
                    return Err(
                        "No content on stdin.\n  \
                         Use one of:\n    \
                           -c \"inline content\"\n    \
                           -c @path/to/file\n    \
                           echo \"content\" | geniuz remember -c - -g \"...\""
                            .to_string()
                    );
                }
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
            let result = db.signal(&resolved, gist.as_deref(), parent.as_deref(), at.as_deref())?;

            let address = db.address(&result.uuid);
            if json {
                Ok(serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true, "action": "remember", "uuid": result.uuid, "address": address,
                    "embedded": result.embedded
                })).unwrap())
            } else if result.embedded {
                Ok(format!("✅ Kept {}", address))
            } else {
                Ok(format!(
                    "✅ Kept {} (keyword search only — embedding unavailable, run 'geniuz backfill' later)",
                    address
                ))
            }
        }

        Command::Recall { query, all, thread, grep, random, keyword, full, limit, since, until, json } => {
            if query.as_deref() == Some("help") {
                let mut cmd = Cli::build();
                let sub = cmd.find_subcommand_mut("recall").unwrap();
                sub.print_help().ok();
                println!();
                return Ok(String::new());
            }

            let window = geniuz::window::Window::parse(since.as_deref(), until.as_deref())?;

            if all {
                if IN_FOLDER.get().and_then(|f| f.as_deref()).is_some() {
                    return Err("--all searches every folder, so it can't be combined with --in.".to_string());
                }
                return recall_all(query.as_deref(), keyword, full, &window, limit.unwrap_or(20), json);
            }

            // An address names its own folder, wherever this command would
            // otherwise have looked.
            let opened = query.as_deref().and_then(geniuz::folders::parse_address);
            let thread_address = thread.as_deref().and_then(geniuz::folders::parse_address);
            let db = match opened.as_ref().or(thread_address.as_ref()) {
                Some((folder, _)) => {
                    let reg = geniuz::folders::Registry::from_env();
                    open_resolved(|| Ok(reg.find(folder)?.db_path))?
                }
                None => get_db()?,
            };
            let thread = thread_address.map(|(_, memory)| memory).or(thread);
            if let Some((folder, memory)) = &opened {
                if db.get_by_uuid_prefix(memory)?.is_none() {
                    return Err(format!("No memory {} in folder {}.", memory, folder));
                }
            }
            let full = full || opened.is_some();
            let keyword = keyword && opened.is_none();
            let query = opened.map(|(_, memory)| memory).or(query);

            if !grep.is_empty() {
                let terms = geniuz::dig::Terms::parse(&grep)?;
                return recall_grep(&db, &terms, &window, thread.as_deref(), limit.unwrap_or(50), json);
            }
            if let Some(id) = thread {
                return recall_thread(&db, &id, &window, limit.unwrap_or(100), full, json);
            }
            let limit = limit.unwrap_or(20);

            // UUID-shaped queries are a lookup key, not a semantic search target.
            // A bare 8-char or 36-char hex-ish string embeds to noise; treating it as
            // direct retrieval surfaces the matching memory instead of burying it
            // under unrelated results. Bypassed by --keyword (explicit keyword mode).
            fn looks_like_uuid(q: &str) -> bool {
                let s = q.trim();
                match s.len() {
                    8 => s.chars().all(|c| c.is_ascii_hexdigit()),
                    36 => {
                        let b = s.as_bytes();
                        b[8] == b'-' && b[13] == b'-' && b[18] == b'-' && b[23] == b'-'
                            && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
                    }
                    _ => false,
                }
            }

            // What kind of search this is, for the scope line.
            let semantic_ready = db.embedding_count().map(|n| n > 0).unwrap_or(false);
            let mut mode = match query.as_deref() {
                _ if random => "random".to_string(),
                None => "recent".to_string(),
                Some(q) if !keyword && looks_like_uuid(q) => format!("id {}", q.trim()),
                Some(q) if keyword || !semantic_ready => format!("keyword \"{q}\""),
                Some(q) => geniuz::dig::semantic_mode(q),
            };
            let mut entries = if random {
                match db.random_in(&window)? {
                    Some(e) => vec![e],
                    None => vec![],
                }
            } else if query.is_none() {
                // No query on recall → show recent as fallback
                db.recent_in(&window, limit)?
            } else {
                let q = query.as_deref().unwrap();
                if !keyword && looks_like_uuid(q) {
                    match db.get_by_uuid_prefix(q)? {
                        Some(entry) => vec![entry],
                        None => {
                            eprintln!("[geniuz] No memory found matching UUID {} — falling back to semantic search.", q);
                            mode = format!("semantic \"{q}\" (no memory has that id)");
                            db.semantic_search_in(q, &window, limit)?
                        }
                    }
                } else if keyword {
                    db.keyword_search_in(q, &window, limit)?
                } else {
                    db.semantic_search_in(q, &window, limit)?
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
                let mut out = serde_json::json!({
                    "ok": true, "action": "recall", "count": data.len(), "memories": data,
                    "searched": scope_json(&db, &window, &mode),
                });
                // Report a partial index rather than presenting these results
                // as the whole corpus.
                if let Ok(n) = db.unembedded_count() {
                    if n > 0 { out["unembedded"] = serde_json::json!(n); }
                }
                return Ok(serde_json::to_string_pretty(&out).unwrap());
            }

            let scope = scope_text(&db, &window, &mode);
            if entries.is_empty() {
                return Ok(format!("No memories found.\n{scope}"));
            }
            let harvest = db.harvest(&entries, query.as_deref().unwrap_or(""));

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
            // Unembedded memories are keyword-findable but invisible to
            // ranking. Say so, so the gap reads as absent rather than as
            // nothing-was-there.
            if let Ok(n) = db.unembedded_count() {
                if n > 0 {
                    lines.push(format!(
                        "\n({} {} not embedded — ranked results are partial. Run 'geniuz backfill'.)",
                        n, if n == 1 { "memory" } else { "memories" }
                    ));
                }
            }
            lines.extend(harvest.lines());
            lines.push(scope);
            Ok(lines.join("\n"))
        }

        Command::Recent { limit, full, since, until, json } => {
            let window = geniuz::window::Window::parse(since.as_deref(), until.as_deref())?;
            let db = get_db()?;
            let mut entries = db.recent_in(&window, limit)?;

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
                    "ok": true, "action": "recent", "count": data.len(), "memories": data,
                    "searched": scope_json(&db, &window, "recent"),
                })).unwrap());
            }

            let scope = scope_text(&db, &window, "recent");
            if entries.is_empty() && window.is_all() {
                return Ok("No memories yet.".to_string());
            }
            if entries.is_empty() {
                return Ok(format!("No memories found.\n{scope}"));
            }
            let harvest = db.harvest(&entries, "");

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
            lines.extend(harvest.lines());
            lines.push(scope);
            Ok(lines.join("\n"))
        }

        Command::Watch { interval, since, exec, once, json } => {
            // An address names its own folder: watch that one, as recall opens it,
            // whatever the default. With -f too, they must agree.
            let db = match since.as_deref().and_then(geniuz::folders::parse_address) {
                Some((folder, _)) => {
                    let reg = geniuz::folders::Registry::from_env();
                    let named = reg.find(&folder)?;
                    if let Some(flag) = IN_FOLDER.get().and_then(|f| f.as_deref()) {
                        if reg.find(flag)?.uuid != named.uuid {
                            return Err(format!("--since {} is in folder {}, but -f names {}", since.as_deref().unwrap_or_default(), named.name, flag));
                        }
                    }
                    open_resolved(|| Ok(named.db_path.clone()))?
                }
                None => get_db()?,
            };

            if let Some(t) = exec.as_deref().filter(|t| ["'{gist}'", "'{content}'", "'{json}'"].iter().any(|p| t.contains(p))) {
                eprintln!("[watch] Memory text arrives as a shell variable, which single quotes keep from expanding. Use double quotes in: {t}");
            }

            // The cursor is a position in the order memories were written, so every
            // memory written since is seen, whatever its date. Between checks it is held
            // as the last memory's ID and looked up again, because another tool's VACUUM
            // can renumber rows (it keeps their order).
            let mut cursor = if let Some(ref id) = since {
                db.position_of(id)?.ok_or_else(|| format!("Memory not found: {}", id))?
            } else if once {
                // Starting from now, a single check could never find anything.
                return Err("--once needs --since <memory>: the last one you saw (a UUID or an address)".to_string());
            } else {
                // From now: after the newest memory, whatever the clock says.
                db.last_position()?.unwrap_or_else(|| (String::new(), 0))
            };

            if !once {
                eprintln!("[watch] Watching for new memories (every {}s). Ctrl+C to stop.", interval);
                if since.is_some() {
                    eprintln!("[watch] Starting after: {}", cursor.0);
                }
            }

            let mut last_seen: Option<String> = since.clone();
            loop {
                if let Some(id) = last_seen.as_deref() {
                    if let Some(pos) = db.position_of(id)? {
                        cursor = pos;
                    }
                }
                let found = db.after(&cursor, 100)?;
                if let Some((pos, e)) = found.last() {
                    cursor = pos.clone();
                    last_seen = Some(e.memory_uuid.clone());
                }
                let entries: Vec<_> = found.into_iter().map(|(_, e)| e).collect();

                if !entries.is_empty() {
                    for e in &entries {
                        if let Some(ref cmd_template) = exec {
                            let (cmd, vars) = watch_exec_command(cmd_template, e);
                            match std::process::Command::new("sh")
                                .arg("-c")
                                .arg(&cmd)
                                .envs(vars)
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
                    // The help promises 0 = new memories, 1 = none, so a script
                    // or cron can wake only when there is something to read.
                    // Returning it as Err sends the note to stderr and exits 1;
                    // it's an answer, not a failure.
                    if entries.is_empty() {
                        return Err("No new memories.".to_string());
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
            let path = resolve_db_path()?.to_string_lossy().to_string();

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
                McpCommand::Install { env } => {
                    mcp::install(&env)
                }
                McpCommand::Status => {
                    mcp::status()
                }
            }
        }

        Command::Settings(settings_cmd) => {
            use cli::SettingsCommand;
            use geniuz::settings::Settings;
            match settings_cmd {
                SettingsCommand::List { json } => {
                    let s = Settings::load_strict()?;
                    if json {
                        serde_json::to_string_pretty(&s).map_err(|e| format!("serialize: {}", e))
                    } else {
                        Ok(s.list())
                    }
                }
                SettingsCommand::Get { key } => {
                    Settings::load_strict()?.get(&key)
                }
                SettingsCommand::Set { key, value } => {
                    let mut s = Settings::load_strict()?;
                    let stored = s.set(&key, &value)?;
                    s.save()?;
                    Ok(format!("{} = {}", key, stored))
                }
                SettingsCommand::Path => {
                    Ok(Settings::path().display().to_string())
                }
            }
        }
    }
}

#[cfg(test)]
mod watch_exec_tests {
    use super::watch_exec_command;

    fn run(template: &str, gist: &str, content: &str) -> (String, bool) {
        let e = geniuz::db::SignalEntry {
            memory_uuid: "ABCDEF12-0000-0000-0000-000000000000".into(),
            gist: gist.into(),
            created_at: "2026-11-02 00:00:00".into(),
            parent_uuid: None,
            content: Some(content.into()),
            score: None,
        };
        let (cmd, vars) = watch_exec_command(template, &e);
        let out = std::process::Command::new("sh").arg("-c").arg(&cmd).envs(vars).output().unwrap();
        (String::from_utf8_lossy(&out.stdout).trim_end().to_string(), out.status.success())
    }

    /// A memory's words are data: shell syntax in them does not run, in any
    /// placeholder, and the bytes arrive intact.
    #[test]
    fn shell_syntax_in_a_memory_does_not_run() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("ran");
        let attack = format!("x$(touch {0})`touch {0}`\"; touch {0}; \"", marker.display());
        for template in ["printf %s \"{gist}\"", "printf %s \"{content}\"", "printf %s \"{json}\"", "echo {gist}"] {
            run(template, &attack, &attack);
            assert!(!marker.exists(), "memory text ran through: {template}");
        }
        let (out, ok) = run("printf %s \"{gist}\"", &attack, "");
        assert!(ok);
        assert_eq!(out, attack, "every byte arrives");
    }

    /// The everyday failure the old pasting caused: an apostrophe or a dollar
    /// sign in a memory broke or mangled the command.
    #[test]
    fn ordinary_punctuation_survives() {
        assert_eq!(run("printf %s \"{gist}\"", "don't pay $5", "").0, "don't pay $5");
        let (out, ok) = run("printf %s \"{json}\"", "don't", "it's");
        assert!(ok);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["content"], "it's");
        assert_eq!(run("echo {uuid}", "g", "c").0, "ABCDEF12");
    }
}

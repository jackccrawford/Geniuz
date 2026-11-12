// Geniuz dashboard — Tauri commands.
//
// All data access goes through the canonical geniuz library
// (`geniuz::db::DatabaseManager`). The dashboard does NOT open memory.db
// directly or maintain its own data layer. One source of truth for schema
// and query logic, used by the CLI, the MCP server, and this dashboard alike.

use geniuz::db::{DatabaseManager, MemoryEntry};
use geniuz::folders::{Folder, Registry};
use geniuz::settings::Settings;
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Emitter, Manager};

/// The folder this window works in. Resolved every time, because a folder can
/// move while the window is open.
fn db_path() -> Result<String, String> {
    let reg = Registry::from_env();
    let shown = SHOWN.lock().map_err(|_| "The app is in a bad state.".to_string())?.clone();
    match shown {
        // The path, afresh: the folder may have moved, but it's still the one on screen.
        Shown::Folder(uuid) => reg.find(&uuid).map(|f| f.db_path.to_string_lossy().into_owned()),
        Shown::Failed(why) => Err(why),
        Shown::NotYet => reg.resolve(None, None, None).map(|p| p.to_string_lossy().into_owned()),
    }
}

/// The folder this window is showing, set when the page loads. Every read and
/// write goes to it, so if something else changes the current folder, a draft
/// can't land in a folder that isn't the one on screen.
static SHOWN: std::sync::Mutex<Shown> = std::sync::Mutex::new(Shown::NotYet);

/// The folder the person chose to look at in this window: the window's own state,
/// not the machine's. Keep here starts on the folder on screen and writes where its
/// pulldown says. Kept across page reloads; a new launch opens on My memory.
static VIEW: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Where a memory goes when no folder is named (My memory), as its UUID.
fn first_uuid(reg: &Registry) -> Option<String> {
    let using = reg.resolve(None, None, None).ok().map(|p| std::fs::canonicalize(&p).unwrap_or(p))?;
    reg.scan().folders.iter()
        .find(|f| std::fs::canonicalize(&f.db_path).unwrap_or_else(|_| f.db_path.clone()) == using)
        .map(|f| f.uuid.clone())
}

/// The folder this window shows: the one chosen here, else My memory.
fn shown_uuid(reg: &Registry) -> Option<String> {
    if let Ok(slot) = SHOWN.lock() {
        if let Shown::Folder(u) = &*slot { return Some(u.clone()); }
    }
    first_uuid(reg)
}

#[derive(Clone)]
enum Shown {
    NotYet,
    Folder(String),
    /// Nothing may quietly follow the current folder after this.
    Failed(String),
}

fn open_db() -> Result<DatabaseManager, String> {
    DatabaseManager::open_existing(&db_path()?).or_else(|_| DatabaseManager::open_existing(&db_path()?))
}

// -------------------------------------------------------------------------
// Data shapes returned to the frontend
// -------------------------------------------------------------------------

#[derive(Serialize, Default)]
struct FolderStats {
    total_memories: usize,
    this_week: usize,
    storage_bytes: u64,
    daily_average_recent: f64,
    last_write_iso: Option<String>,
}

#[derive(Serialize)]
struct RecentMemory {
    uuid: String,
    gist: String,
    content: Option<String>,
    created_at: String,
    parent_uuid: Option<String>,
    /// Which folder it came from, when a search covered more than the one on screen.
    folder: Option<String>,
    folder_name: Option<String>,
    /// How many images it shows, so a list can say so without fetching them.
    images: usize,
    /// Their short IDs, in order, so a list row can show them small.
    image_ids: Vec<String>,
}

/// One matching line from an Exact search.
#[derive(Serialize)]
struct FindLine {
    uuid: String,
    created_at: String,
    line: String,
    folder: String,
    folder_name: String,
}

/// Where a pointer in the results can be opened.
#[derive(Serialize)]
struct PointerTarget {
    folder: String,
    uuid: String,
}

/// What Find shows: memories (Meaning) or lines (Exact), then where to go
/// next and what was searched (search features 4 and 5).
#[derive(Serialize)]
struct FindResult {
    memories: Vec<RecentMemory>,
    lines: Vec<FindLine>,
    names: Vec<String>,
    pointers: Vec<String>,
    /// The pointers a click can open, in the folders searched or by address in
    /// any folder. The rest may live elsewhere.
    pointer_targets: std::collections::HashMap<String, PointerTarget>,
    scope: String,
}

#[derive(Serialize)]
struct DailyCount {
    date: String,
    count: usize,
}

#[derive(Serialize)]
struct MemoryDetail {
    uuid: String,
    gist: String,
    content: Option<String>,
    created_at: String,
    parent_uuid: Option<String>,
    /// The images it shows, in order. Their bytes are asked for one at a time.
    images: Vec<ImageRef>,
}

#[derive(Serialize)]
struct ImageRef {
    id: String,
    width: u32,
    height: u32,
    bytes: u64,
    /// When the folder's owner took it back or put another in its place, if they did.
    unavailable: bool,
    replaced_at: Option<String>,
}

#[derive(Serialize)]
struct StatusReport {
    memory_count: usize,
    embedding_count: usize,
    indexed_pct: f64,
    embedding_model: Option<String>,
    data_dir: String,
    data_dir_bytes: u64,
    claude_desktop_configured: bool,
    claude_desktop_has_geniuz: bool,
    claude_desktop_config_path: Option<String>,
}

#[derive(Deserialize)]
struct SettingsPatch {
    settings: Settings,
}

// -------------------------------------------------------------------------
// Commands
// -------------------------------------------------------------------------

#[tauri::command]
fn get_app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// A newer release, if the last check found one. The Mac menu bar (or, on
/// Windows and Linux, this app's tray) asks GitHub and writes `update.json` in
/// the data directory; the pages only read it, so every surface agrees.
#[derive(serde::Serialize)]
struct UpdateInfo {
    version: String,
    url: String,
}

/// Numeric, part by part ("5.0.10" > "5.0.9"); a pre-release suffix is ignored.
fn version_newer(candidate: &str, current: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.trim().trim_start_matches(['v', 'V']).split('-').next().unwrap_or("")
            .split('.').map(|p| p.parse().unwrap_or(0)).collect()
    };
    let (a, b) = (parts(candidate), parts(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

/// What the last check found: when it ran, and a newer release if there was one.
#[derive(serde::Serialize)]
struct UpdateStatus {
    checked_at: String,
    newer: Option<UpdateInfo>,
}

#[tauri::command]
fn get_update() -> Option<UpdateStatus> {
    let raw = std::fs::read_to_string(geniuz::data_dir().join("update.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let checked_at = v.get("checked_at")?.as_str()?.to_string();
    let newer = (|| {
        let latest = v.get("latest")?.as_str()?;
        let url = v.get("url")?.as_str()?;
        // Only a link to the project's own releases opens from here.
        if !url.starts_with("https://github.com/jackccrawford/Geniuz/releases/") {
            return None;
        }
        // The page of the latest release, never the address the check recorded:
        // that one names a single release, and a newer one may be out by now.
        version_newer(latest, env!("CARGO_PKG_VERSION"))
            .then(|| UpdateInfo { version: latest.to_string(), url: format!("{}latest", geniuz::update::RELEASES_PREFIX) })
    })();
    Some(UpdateStatus { checked_at, newer })
}

/// A page asked for from outside the window: the Mac menu bar app opens the
/// dashboard with `geniuz://settings`. Kept until the page reads it, because on a
/// cold start the link arrives before the page is listening.
static PENDING_NAV: std::sync::Mutex<Option<&'static str>> = std::sync::Mutex::new(None);

/// Whether the page has started listening. Before that, a link is held for it;
/// after, it is delivered and not kept, so a later reload of the page doesn't
/// jump back to wherever the last link pointed.
static PAGE_LISTENING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The page a `geniuz://` link names, as the id the page's navigation takes. Only
/// these four: a link can't make the window do anything else.
fn nav_for_link(url: &str) -> Option<&'static str> {
    let rest = url.strip_prefix("geniuz://")?;
    // Query and fragment off first, then the trailing slash: "settings/?x=1" is Settings.
    match rest.split(['?', '#']).next()?.trim_end_matches('/') {
        "settings" => Some("tray_settings"),
        "status" => Some("tray_status"),
        "find" => Some("tray_find"),
        "memories" => Some("tray_recent"),
        _ => None,
    }
}

/// The page asked for before the window was ready, once. The page calls this
/// when it has started listening, and from then on links reach it directly.
#[tauri::command]
fn take_startup_nav() -> Option<&'static str> {
    PAGE_LISTENING.store(true, std::sync::atomic::Ordering::SeqCst);
    PENDING_NAV.lock().ok()?.take()
}

/// Ask now whether a newer Geniuz is out, whatever the schedule says, and report
/// what the check found. Off the main thread: it waits on GitHub.
#[tauri::command(async)]
fn check_update_now() -> Result<Option<UpdateStatus>, String> {
    geniuz::update::check()?;
    Ok(get_update())
}

#[cfg(test)]
mod update_tests {
    use super::version_newer;
    #[test]
    fn compares_versions_by_number() {
        assert!(version_newer("5.0.10", "5.0.9"));
        assert!(version_newer("v5.1", "5.0.3"));
        assert!(!version_newer("5.0.3", "5.0.3"));
        assert!(!version_newer("5.0.2", "5.0.3"));
        assert!(!version_newer("5.0.3-rc.1", "5.0.3"));
    }
}

/// Persist a new memory. `gist` is required: the library refuses one that is
/// missing or blank, and stores it trimmed. Returns the short (8-char) UUID
/// prefix of the new memory.
///
/// Off the main thread (`async`): keeping embeds the words and prepares any
/// images, which takes long enough to freeze the window if it ran there, so the
/// Keep button could never show that it had been pressed.
#[tauri::command(async)]
fn remember_memory(
    gist: Option<String>,
    content: String,
    folder: Option<String>,
    parent: Option<String>,
    images: Option<Vec<String>>,
) -> Result<String, String> {
    // The folder chosen on the page, or the one on screen.
    let db = match folder {
        Some(uuid) => DatabaseManager::open_existing(&Registry::from_env().find(&uuid)?.db_path.to_string_lossy())?,
        None => open_db()?,
    };
    // A reply goes under a memory in this same folder. db.keep refuses a parent
    // it can't find; looking it up here first gives the dashboard's own, plainer
    // message for that.
    let parent = match parent.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => Some(
            db.get_by_uuid_prefix(p)?
                .ok_or_else(|| format!("No memory {} in this folder to reply to", &p[..p.len().min(8)]))?
                .memory_uuid,
        ),
        None => None,
    };
    // Pasted or dropped images arrive as `data:` addresses. Each is prepared by the
    // same code the command line uses, before anything is kept.
    let images = images.unwrap_or_default();
    // Counted before any is decoded: a long list is refused without preparing one.
    if images.len() > geniuz::pictures::MAX_PER_MEMORY {
        return Err(format!(
            "A memory carries at most {} images; this one has {}. Keep the rest in a reply.",
            geniuz::pictures::MAX_PER_MEMORY, images.len()
        ));
    }
    let mut attach = Vec::new();
    for (n, url) in images.iter().enumerate() {
        let picture = geniuz::pictures::from_data_url(url)
            .and_then(|bytes| geniuz::pictures::prepare(&bytes))
            .map_err(|e| format!("Image {}: {}", n + 1, e))?;
        attach.push(geniuz::db::Attach::New(picture));
    }
    db.keep_with_images(content.trim(), gist.as_deref().unwrap_or(""), parent.as_deref(), &attach, None)
        .map(|r| r.uuid)
}

#[tauri::command]
fn get_folder_stats() -> Result<FolderStats, String> {
    let db = open_db()?;
    let total_memories = db.count()?;
    let this_week = db.count_since_days(7)?;
    let recent_30 = db.count_since_days(30)?;
    let daily_average_recent = (recent_30 as f64) / 30.0;
    let last_write_iso = db.last_write_timestamp()?;
    let storage_bytes = std::fs::metadata(db_path()?).map(|m| m.len()).unwrap_or(0);

    Ok(FolderStats {
        total_memories,
        this_week,
        storage_bytes,
        daily_average_recent,
        last_write_iso,
    })
}

#[tauri::command]
fn get_recent_memories(
    limit: Option<u32>,
    offset: Option<u32>,
    oldest_first: Option<bool>,
) -> Result<Vec<RecentMemory>, String> {
    let db = open_db()?;
    let mut entries = db.page(
        oldest_first.unwrap_or(false),
        offset.unwrap_or(0) as usize,
        limit.unwrap_or(24) as usize,
    )?;
    // recent() returns gists only; the stream reads each memory in place, so
    // fetch the words for the ones it shows (as get_thread_chain does).
    for e in &mut entries {
        e.content = db.get_full_content(&e.memory_uuid).ok().flatten();
    }
    db.mark_images(&mut entries);
    Ok(entries.into_iter().map(map_recent).collect())
}

#[tauri::command]
fn get_activity(days: Option<u32>) -> Result<Vec<DailyCount>, String> {
    let db = open_db()?;
    let buckets = db.daily_activity(days.unwrap_or(14))?;
    Ok(buckets
        .into_iter()
        .map(|(date, count)| DailyCount { date, count })
        .collect())
}

#[tauri::command]
fn semantic_search(query: String, limit: Option<u32>) -> Result<Vec<RecentMemory>, String> {
    let db = open_db()?;
    let mut entries = db.semantic_search(&query, limit.unwrap_or(20) as usize)?;
    db.mark_images(&mut entries);
    Ok(entries.into_iter().map(map_recent).collect())
}

#[tauri::command]
fn keyword_search(query: String, limit: Option<u32>) -> Result<Vec<RecentMemory>, String> {
    let db = open_db()?;
    let mut entries = db.keyword_search(&query, limit.unwrap_or(20) as usize)?;
    db.mark_images(&mut entries);
    Ok(entries.into_iter().map(map_recent).collect())
}

/// Find, the dashboard's door to search: `mode` is "meaning" or "exact";
/// `since` is any window the CLI takes (24h, 7d, a date), or none.
#[tauri::command]
fn find(
    query: String,
    mode: String,
    since: Option<String>,
    limit: Option<u32>,
    folders: Option<Vec<String>>,
) -> Result<FindResult, String> {
    use geniuz::{dig, window};
    let reg = Registry::from_env();
    let window = window::Window::parse(since.as_deref(), None)?;
    let limit = limit.unwrap_or(if mode == "exact" { 100 } else { 40 }) as usize;

    let targets: Vec<Folder> = match folders.filter(|f| !f.is_empty()) {
        Some(ids) => ids.iter().map(|id| reg.find(id)).collect::<Result<_, _>>()?,
        None => {
            let shown = std::fs::canonicalize(db_path()?).map_err(|e| e.to_string())?;
            reg.scan()
                .folders
                .into_iter()
                .filter(|f| std::fs::canonicalize(&f.db_path).ok().as_deref() == Some(shown.as_path()))
                .collect()
        }
    };
    let open = |f: &Folder| DatabaseManager::open_existing(&f.db_path.to_string_lossy());

    let mut pool = 0;
    let mut names: Vec<String> = Vec::new();
    let mut pointers: Vec<String> = Vec::new();
    let mut add_harvest = |h: &dig::Harvest| {
        for (n, _) in &h.names {
            if !names.contains(n) {
                names.push(n.clone());
            }
        }
        for p in &h.pointers {
            if !pointers.contains(p) {
                pointers.push(p.clone());
            }
        }
    };
    let mut memories: Vec<(MemoryEntry, &Folder)> = Vec::new();
    let mut lines: Vec<FindLine> = Vec::new();
    let mode_text;

    if mode == "exact" {
        let terms = dig::Terms::parse(&dig::split_terms(&query))?;
        let mut text = None;
        for f in &targets {
            let db = open(f)?;
            pool += db.count_in(&window).unwrap_or(0);
            let d = db.grep_in(&terms, &window, None, limit)?;
            text.get_or_insert_with(|| d.mode(&terms));
            let found: Vec<&str> = d.hits.iter().map(|h| h.line.as_str()).collect();
            let own: Vec<String> = d.hits.iter().map(|h| h.memory_uuid.clone()).collect();
            add_harvest(&dig::Harvest::of(&found, &query, &own));
            lines.extend(d.hits.iter().map(|h| FindLine {
                uuid: h.memory_uuid.clone(),
                created_at: h.created_at.clone(),
                line: h.line.clone(),
                folder: f.uuid.clone(),
                folder_name: f.name.clone(),
            }));
        }
        lines.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        lines.truncate(limit);
        mode_text = text.unwrap_or_else(|| "exact".to_string());
    } else {
        // One loaded model for every folder.
        let backend = geniuz::embedding::create_backend().ok();
        for f in &targets {
            let db = open(f)?;
            pool += db.count_in(&window).unwrap_or(0);
            let entries = db
                .semantic_search_with(&query, &window, limit, backend.as_deref())
                .or_else(|_| db.keyword_search_in(&query, &window, limit))?;
            add_harvest(&db.harvest(&entries, &query));
            let mut entries = entries;
            db.mark_images(&mut entries);
            memories.extend(entries.into_iter().map(|e| (e, f)));
        }
        memories.sort_by(|a, b| {
            b.0.score
                .unwrap_or(f32::MIN)
                .total_cmp(&a.0.score.unwrap_or(f32::MIN))
                .then_with(|| b.0.created_at.cmp(&a.0.created_at))
        });
        memories.truncate(limit);
        mode_text = dig::semantic_mode(&query);
    }

    // A bare ID opens in a folder searched. An address opens in its own folder.
    let mut pointer_targets = std::collections::HashMap::new();
    for p in &pointers {
        let found = match geniuz::folders::parse_address(p) {
            Some((folder, memory)) => reg.find(&folder).ok().and_then(|f| {
                let here = open(&f).ok()?.get_by_uuid_prefix(&memory).ok().flatten()?;
                Some(PointerTarget { folder: f.uuid, uuid: here.memory_uuid })
            }),
            None => targets.iter().find_map(|f| {
                let here = open(f).ok()?.get_by_uuid_prefix(p).ok().flatten()?;
                Some(PointerTarget { folder: f.uuid.clone(), uuid: here.memory_uuid })
            }),
        };
        if let Some(target) = found {
            pointer_targets.insert(p.clone(), target);
        }
    }

    let many = (targets.len() > 1).then(|| format!("{} folders", targets.len()));
    let scope = window::scope_line(many.as_deref(), pool, &window, &mode_text);
    Ok(FindResult {
        memories: memories
            .into_iter()
            .map(|(e, f)| RecentMemory {
                folder: Some(f.uuid.clone()),
                folder_name: Some(f.name.clone()),
                ..map_recent(e)
            })
            .collect(),
        lines,
        names,
        pointers,
        pointer_targets,
        scope,
    })
}

#[tauri::command]
fn get_memory_detail(uuid: String, folder: Option<String>) -> Result<Option<MemoryDetail>, String> {
    let db = match folder {
        Some(id) => DatabaseManager::open_existing(&Registry::from_env().find(&id)?.db_path.to_string_lossy())?,
        None => open_db()?,
    };
    let entry = db.get_by_uuid_prefix(&uuid)?;
    Ok(entry.map(|e| {
        let images = db
            .images_of(&e.memory_uuid)
            .unwrap_or_default()
            .into_iter()
            .map(|i| ImageRef {
                id: i.image_uuid[..8].to_string(), width: i.width, height: i.height, bytes: i.bytes,
                unavailable: i.unavailable, replaced_at: i.replaced_at,
            })
            .collect();
        MemoryDetail { images, ..map_detail(e) }
    }))
}

/// One image, as a `data:` address the page can show. The bytes never leave the
/// folder's file except into the window.
#[tauri::command]
fn get_image(id: String, folder: Option<String>) -> Result<String, String> {
    let db = match folder {
        Some(f) => DatabaseManager::open_existing(&Registry::from_env().find(&f)?.db_path.to_string_lossy())?,
        None => open_db()?,
    };
    let (info, data) = db.image(&id)?.ok_or_else(|| format!("No image {} in this folder.", id.to_uppercase()))?;
    if info.unavailable {
        return Err("This image is unavailable.".to_string());
    }
    Ok(geniuz::pictures::data_url(&info.media_type, &data))
}

#[tauri::command]
fn get_thread_chain(uuid: String, limit: Option<u32>) -> Result<Vec<RecentMemory>, String> {
    let db = open_db()?;
    // thread_in, not the memory_chains view: the view loses a thread whose
    // root names a parent that is not in this store.
    let mut thread = db.thread_in(&uuid, &geniuz::window::Window::all(), limit.unwrap_or(100) as usize)?;
    for e in &mut thread.entries {
        e.content = db.get_full_content(&e.memory_uuid).ok().flatten();
    }
    db.mark_images(&mut thread.entries);
    Ok(thread.entries.into_iter().map(map_recent).collect())
}

#[tauri::command]
fn get_status() -> Result<StatusReport, String> {
    let db = open_db()?;
    let memory_count = db.count()?;
    let embedding_count = db.embedding_count()?;
    let indexed_pct = if memory_count == 0 {
        0.0
    } else {
        (embedding_count as f64 / memory_count as f64) * 100.0
    };
    let embedding_model = db.get_embedding_model().ok().flatten();
    let data_dir = geniuz::data_dir().to_string_lossy().into_owned();
    let data_dir_bytes = dir_size(&geniuz::data_dir()).unwrap_or(0);

    // Claude Desktop check
    let config_path = geniuz::claude_desktop_config_path();
    let (claude_desktop_configured, claude_desktop_has_geniuz, claude_desktop_config_path) =
        match &config_path {
            Some(p) => {
                let exists = p.exists();
                let has_geniuz = if exists {
                    std::fs::read_to_string(p)
                        .ok()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                        .and_then(|v| {
                            v.get("mcpServers")
                                .and_then(|m| m.as_object())
                                .map(|obj| obj.keys().any(|k| k.to_lowercase().contains("geniuz")))
                        })
                        .unwrap_or(false)
                } else {
                    false
                };
                (exists, has_geniuz, Some(p.to_string_lossy().into_owned()))
            }
            None => (false, false, None),
        };

    Ok(StatusReport {
        memory_count,
        embedding_count,
        indexed_pct,
        embedding_model,
        data_dir,
        data_dir_bytes,
        claude_desktop_configured,
        claude_desktop_has_geniuz,
        claude_desktop_config_path,
    })
}

#[tauri::command]
fn get_settings() -> Settings {
    Settings::load()
}

#[tauri::command]
fn update_settings(patch: SettingsPatch) -> Result<Settings, String> {
    // A settings.json that exists but can't be read is not saved over, as the command
    // line, the tray and the menu bar also refuse: the page showed defaults, not it.
    Settings::load_strict()?;
    patch.settings.save()?;
    Ok(Settings::load())
}

/// Export one folder as a single complete file: the folder named, else the one
/// this window shows.
#[tauri::command]
fn export_memory_db_to(target_path: String, folder: Option<String>) -> Result<u64, String> {
    let source = match folder.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
        Some(q) => Registry::from_env().find(q)?.db_path,
        None => std::path::PathBuf::from(db_path()?),
    };
    geniuz::folders::backup_to(&source, std::path::Path::new(&target_path))
}

/// A folder's size on disk: its memory.db and the -wal beside it.
#[tauri::command]
fn folder_disk_bytes(folder: String) -> Result<u64, String> {
    let db = Registry::from_env().find(&folder)?.db_path;
    let size = |p: std::path::PathBuf| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    Ok(size(db.clone()) + size(db.with_extension("db-wal")))
}

#[derive(Serialize)]
struct FolderRow {
    id: String,
    uuid: String,
    name: String,
    description: String,
    /// One emoji its writer chose, or empty.
    face: String,
    memories: i64,
    last_written: Option<String>,
    location: String,
    /// The folder this window shows.
    current: bool,
    /// The first folder lives beside the others, so it stays where it is.
    fixed: bool,
    pinned: bool,
    /// "agents" when the folder sits in a directory named agents (each agent's
    /// own home), "folders" otherwise. Where a folder lives says what it is;
    /// nothing in it is read to decide.
    tree: &'static str,
}

#[derive(Serialize)]
struct FolderList {
    folders: Vec<FolderRow>,
    archived: Vec<FolderRow>,
    blocked: usize,
    unavailable: usize,
}

fn folder_row(f: &Folder, current: Option<&str>, pinned: bool) -> FolderRow {
    let canonical = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (memories, last_written) = f.stats().unwrap_or((0, None));
    let dir = f.db_path.parent().unwrap_or(&f.db_path);
    FolderRow {
        id: f.short_id(),
        uuid: f.uuid.clone(),
        name: f.name.clone(),
        description: f.description.clone(),
        face: f.face.clone(),
        memories,
        last_written,
        location: dir.to_string_lossy().into_owned(),
        current: current.is_some_and(|c| c.eq_ignore_ascii_case(&f.uuid)),
        fixed: canonical(dir) == canonical(&geniuz::data_dir()),
        pinned,
        tree: tree_of(dir),
    }
}

/// Which tree a folder's directory belongs to (see FolderRow::tree).
fn tree_of(dir: &std::path::Path) -> &'static str {
    let parent = dir.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_lowercase());
    if parent.as_deref() == Some("agents") { "agents" } else { "folders" }
}

#[tauri::command]
fn list_folders() -> FolderList {
    let reg = Registry::from_env();
    let shown = shown_uuid(&reg);
    folder_list(&reg, shown.as_deref())
}

/// The list, with `current` marking exactly this folder.
fn folder_list(reg: &Registry, current: Option<&str>) -> FolderList {
    let scan = reg.scan();
    let pins = reg.pinned();
    FolderList {
        folders: scan
            .folders
            .iter()
            .map(|f| folder_row(f, current, pins.iter().any(|p| p.eq_ignore_ascii_case(&f.uuid))))
            .collect(),
        archived: reg.archived().iter().map(|f| folder_row(f, None, false)).collect(),
        blocked: scan.blocked.len(),
        unavailable: scan.unavailable.len() + scan.pending_moves.len(),
    }
}

/// Called once per page load: show the folder chosen in this window, else My
/// memory, and stay with it.
#[tauri::command]
fn bind_current_folder() -> Result<FolderList, String> {
    let reg = Registry::from_env();
    let viewed = VIEW.lock().ok().and_then(|v| v.clone()).and_then(|u| reg.find(&u).ok()).map(|f| f.uuid);
    if let Some(uuid) = viewed {
        let mut slot = SHOWN.lock().map_err(|_| "The app is in a bad state.".to_string())?;
        *slot = Shown::Folder(uuid.clone());
        return Ok(folder_list(&reg, Some(&uuid)));
    }
    // One decision: what's bound is exactly what the page is told it's showing.
    let resolved = reg.resolve(None, None, None);
    let chosen = resolved.and_then(|p| {
        let path = std::fs::canonicalize(&p).unwrap_or(p);
        let scan = reg.scan();
        let found = scan.folders.iter().find(|f| {
            std::fs::canonicalize(&f.db_path).unwrap_or_else(|_| f.db_path.clone()) == path
        });
        found.map(|f| f.uuid.clone()).ok_or_else(|| "My memory isn't available.".to_string())
    });
    let mut slot = SHOWN.lock().map_err(|_| "The app is in a bad state.".to_string())?;
    match chosen {
        Ok(uuid) => {
            *slot = Shown::Folder(uuid.clone());
            // Replace, not keep: a viewed folder that vanished must not pull the window
            // back when it returns.
            if let Ok(mut v) = VIEW.lock() { *v = Some(uuid.clone()); }
            Ok(folder_list(&reg, Some(&uuid)))
        }
        Err(why) => {
            *slot = Shown::Failed(why.clone());
            Err(why)
        }
    }
}

/// Show this folder in this window. Changes nothing anywhere else.
#[tauri::command]
fn view_folder(uuid: String) -> Result<(), String> {
    let f = Registry::from_env().find(&uuid)?;
    *VIEW.lock().map_err(|_| "The app is in a bad state.".to_string())? = Some(f.uuid);
    Ok(())
}

/// A new folder opens in this window.
#[tauri::command]
fn create_folder(name: String, description: String) -> Result<(), String> {
    let reg = Registry::from_env();
    let f = reg.create(&name, &description, None)?;
    *VIEW.lock().map_err(|_| "The app is in a bad state.".to_string())? = Some(f.uuid);
    Ok(())
}

#[tauri::command]
fn update_folder(uuid: String, name: String, description: String) -> Result<(), String> {
    let reg = Registry::from_env();
    let f = reg.find(&uuid)?;
    if f.name != name.trim() {
        reg.rename(&uuid, &name)?;
    }
    if f.description != description.trim() {
        reg.describe(&uuid, &description)?;
    }
    Ok(())
}

#[tauri::command]
fn add_folders(path: String) -> Result<usize, String> {
    Registry::from_env()
        .add(std::path::Path::new(&path))
        .map(|found| found.len())
}

#[tauri::command]
fn move_folder(uuid: String, to: String) -> Result<Option<String>, String> {
    Registry::from_env()
        .relocate(&uuid, std::path::Path::new(&to))
        .map(|m| m.left_behind.map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
fn archive_folder(uuid: String) -> Result<String, String> {
    Registry::from_env()
        .archive(&uuid)
        .map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
fn pin_folder(uuid: String, on: bool) -> Result<(), String> {
    Registry::from_env().set_pinned(&uuid, on).map(|_| ())
}

/// Where an address points, so a click on one can open it from anywhere.
#[tauri::command]
fn resolve_address(address: String) -> Option<PointerTarget> {
    let (folder, memory) = geniuz::folders::parse_address(&address)?;
    let f = Registry::from_env().find(&folder).ok()?;
    let db = DatabaseManager::open_existing(&f.db_path.to_string_lossy()).ok()?;
    let here = db.get_by_uuid_prefix(&memory).ok().flatten()?;
    Some(PointerTarget { folder: f.uuid, uuid: here.memory_uuid })
}

// ---- Keyboard shortcuts ----
//
// Their own file, not settings.json: the menubar app rewrites that file from a
// fixed list of fields, so anything else in it would be lost.

/// Every action with a shortcut: menu ID, what it's called, and its default.
const SHORTCUTS: [(&str, &str, &str); 12] = [
    ("menu_keep", "Keep", "CmdOrCtrl+N"),
    ("menu_find", "Find", "CmdOrCtrl+K"),
    ("menu_recent", "Memories", "CmdOrCtrl+0"),
    ("menu_new_folder", "New folder", "CmdOrCtrl+Shift+N"),
    ("menu_sidebar", "Toggle sidebar", "CmdOrCtrl+B"),
    ("menu_status", "Status", "CmdOrCtrl+Shift+S"),
    ("menu_settings", "Settings", "CmdOrCtrl+,"),
    ("menu_export", "Export this folder", "CmdOrCtrl+Shift+E"),
    ("menu_refresh", "Refresh", "CmdOrCtrl+R"),
    ("menu_text_bigger", "Bigger text", "CmdOrCtrl+="),
    ("menu_text_smaller", "Smaller text", "CmdOrCtrl+-"),
    // Not CmdOrCtrl+0, which already opens Memories.
    ("menu_text_default", "Default text size", "CmdOrCtrl+Shift+0"),
];

// ---- Text size ---------------------------------------------------------------
// The whole window is scaled, as a browser's zoom does, so every piece of text
// grows together. The choice is kept in its own file, for the reason above.

const TEXT_SIZES: [f64; 9] = [0.8, 0.9, 1.0, 1.1, 1.25, 1.4, 1.6, 1.8, 2.0];

fn text_size_path() -> std::path::PathBuf {
    geniuz::data_dir().join("text-size")
}

/// The saved scale, or 1.0. Anything unreadable or out of range counts as 1.0.
fn saved_text_size() -> f64 {
    std::fs::read_to_string(text_size_path())
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|z| (TEXT_SIZES[0]..=TEXT_SIZES[TEXT_SIZES.len() - 1]).contains(z))
        .unwrap_or(1.0)
}

/// The next size up or down from `now`, stopping at the ends.
fn step_text_size(now: f64, bigger: bool) -> f64 {
    let at = TEXT_SIZES.iter().position(|z| (z - now).abs() < 0.001);
    match (at, bigger) {
        (Some(i), true) => TEXT_SIZES[(i + 1).min(TEXT_SIZES.len() - 1)],
        (Some(i), false) => TEXT_SIZES[i.saturating_sub(1)],
        // A saved value that isn't one of the steps: move to the nearest step that way.
        (None, true) => TEXT_SIZES.iter().copied().find(|z| *z > now).unwrap_or(now),
        (None, false) => TEXT_SIZES.iter().rev().copied().find(|z| *z < now).unwrap_or(now),
    }
}

/// Apply a menu choice to the window and remember it.
fn change_text_size(app: &tauri::AppHandle, id: &str) {
    use tauri::Manager;
    let now = saved_text_size();
    let next = match id {
        "menu_text_bigger" => step_text_size(now, true),
        "menu_text_smaller" => step_text_size(now, false),
        _ => 1.0,
    };
    if let Some(window) = app.get_webview_window("main") {
        if let Err(e) = window.set_zoom(next) {
            eprintln!("[geniuz] text size not changed: {e}");
            return;
        }
    }
    if let Err(e) = std::fs::write(text_size_path(), next.to_string()) {
        eprintln!("[geniuz] text size not saved: {e}");
    }
}

fn shortcuts_path() -> std::path::PathBuf {
    geniuz::data_dir().join("shortcuts.json")
}

fn saved_shortcuts() -> std::collections::HashMap<String, String> {
    std::fs::read_to_string(shortcuts_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// The keys for this action, or none if the person cleared them.
fn shortcut_for(saved: &std::collections::HashMap<String, String>, id: &str) -> Option<String> {
    let default = SHORTCUTS.iter().find(|s| s.0 == id).map(|s| s.2.to_string());
    match saved.get(id) {
        Some(keys) if keys.is_empty() => None,
        Some(keys) => Some(keys.clone()),
        None => default,
    }
}

#[derive(Serialize)]
struct ShortcutRow {
    id: String,
    label: String,
    keys: String,
    default_keys: String,
}

#[tauri::command]
fn get_shortcuts() -> Vec<ShortcutRow> {
    let saved = saved_shortcuts();
    SHORTCUTS
        .iter()
        .map(|(id, label, default)| ShortcutRow {
            id: id.to_string(),
            label: label.to_string(),
            keys: shortcut_for(&saved, id).unwrap_or_default(),
            default_keys: default.to_string(),
        })
        .collect()
}

/// Save one action's keys, and show them in the menu at once. Empty clears them.
/// Refused, with nothing changed, if another action has them or they aren't keys
/// the menu can use.
#[tauri::command]
fn set_shortcut(app: tauri::AppHandle, id: String, keys: String) -> Result<(), String> {
    if !SHORTCUTS.iter().any(|s| s.0 == id) {
        return Err("That isn't an action with a shortcut.".to_string());
    }
    let mut saved = saved_shortcuts();
    if !keys.is_empty() {
        let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
        for (other, label, _) in SHORTCUTS {
            if other != id && shortcut_for(&saved, other).is_some_and(|k| same(&k, &keys)) {
                return Err(format!("{} already uses those keys.", label));
            }
        }
        if (1..=9).any(|n| same(&keys, &format!("CmdOrCtrl+{}", n))) {
            return Err("Those keys switch to your first nine folders.".to_string());
        }
    }
    let before = saved.clone();
    saved.insert(id, keys);
    let write = |map: &std::collections::HashMap<String, String>| -> Result<(), String> {
        let body = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
        std::fs::write(shortcuts_path(), body).map_err(|e| format!("Couldn't save: {}", e))
    };
    write(&saved)?;
    match build_menu(&app, true) {
        Ok(menu) => app.set_menu(menu).map(|_| ()).map_err(|e| e.to_string()),
        Err(_) => {
            let _ = write(&before);
            Err("The menu can't use those keys. Try another combination.".to_string())
        }
    }
}

/// Rebuild the menu, for a change in the folders. While recording a new shortcut
/// the menu goes without any, or it would act on the keys being recorded.
#[tauri::command]
fn refresh_menu(app: tauri::AppHandle, shortcuts: bool) -> Result<(), String> {
    let menu = build_menu(&app, shortcuts).map_err(|e| e.to_string())?;
    app.set_menu(menu).map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
fn restore_folder(uuid: String) -> Result<(), String> {
    Registry::from_env().restore(&uuid).map(|_| ())
}

#[tauri::command]
fn get_data_dir() -> String {
    geniuz::data_dir().to_string_lossy().into_owned()
}

// -------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------

// No category is read out of a gist. Meaning is the writer's, in the words,
// and the app shows the gist whole.

fn map_recent(e: MemoryEntry) -> RecentMemory {
    RecentMemory {
        uuid: e.memory_uuid,
        gist: e.gist,
        content: e.content,
        created_at: e.created_at,
        parent_uuid: e.parent_uuid,
        folder: None,
        folder_name: None,
        images: e.images.len(),
        image_ids: e.images,
    }
}

fn map_detail(e: MemoryEntry) -> MemoryDetail {
    MemoryDetail {
        uuid: e.memory_uuid,
        gist: e.gist,
        content: e.content,
        created_at: e.created_at,
        parent_uuid: e.parent_uuid,
        images: Vec::new(),
    }
}

fn dir_size(path: &std::path::Path) -> std::io::Result<u64> {
    let mut total = 0;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_file() {
            total += meta.len();
        } else if meta.is_dir() {
            total += dir_size(&entry.path()).unwrap_or(0);
        }
    }
    Ok(total)
}

// Build the native macOS / Windows / Linux menu bar.
// Menu items with `id` emit menu events that the frontend listens for via
// `tauri::Emitter`-relayed window events ("menu:<id>"). Predefined items
// (undo/redo/cut/copy/paste/etc.) work without any wiring.
fn build_menu(app: &tauri::AppHandle, shortcuts: bool) -> tauri::Result<Menu<tauri::Wry>> {
    let saved = saved_shortcuts();
    let keys = |id: &str| if shortcuts { shortcut_for(&saved, id) } else { None };
    let item = |id: &str, label: &str| MenuItem::with_id(app, id, label, true, keys(id));

    let mi_keep = item("menu_keep", "Keep…")?;
    let mi_find = item("menu_find", "Find…")?;
    let mi_recent = item("menu_recent", "Memories")?;
    let mi_status = item("menu_status", "Status")?;
    let mi_settings = item("menu_settings", "Settings…")?;
    let mi_export = item("menu_export", "Export This Folder…")?;

    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &mi_keep,
            &mi_find,
            &mi_recent,
            &mi_status,
            &PredefinedMenuItem::separator(app)?,
            &mi_settings,
            &mi_export,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, Some("Close"))?,
        ],
    )?;

    // Folders: the same order as the sidebar, with the first nine a keystroke away.
    let list = list_folders();
    let folders_menu = Submenu::new(app, "Folders", true)?;
    folders_menu.append(&item("menu_new_folder", "New Folder…")?)?;
    folders_menu.append(&MenuItem::with_id(app, "menu_archived", "Archived", true, None::<&str>)?)?;
    folders_menu.append(&PredefinedMenuItem::separator(app)?)?;
    // With many folders a menu of all of them is a wall: show the one on screen,
    // the pinned ones and the week's (newest first, up to ten), then All Folders….
    const MENU_ALL_UP_TO: usize = 20;
    let shown: Vec<&FolderRow> = if list.folders.len() <= MENU_ALL_UP_TO {
        list.folders.iter().collect()
    } else {
        let week_ago = (chrono::Utc::now() - chrono::Duration::days(7)).format("%Y-%m-%d %H:%M:%S").to_string();
        let mut recent: Vec<&FolderRow> = list.folders.iter()
            .filter(|f| f.last_written.as_deref().is_some_and(|w| w > week_ago.as_str()))
            .collect();
        recent.sort_by(|a, b| b.last_written.cmp(&a.last_written));
        let mut out: Vec<&FolderRow> = Vec::new();
        // The folder on screen first (what this window shows), then pinned, then recent.
        for f in list.folders.iter().filter(|f| f.current)
            .chain(list.folders.iter().filter(|f| f.pinned))
            .chain(recent.into_iter().take(10))
        {
            if !out.iter().any(|o| o.uuid == f.uuid) { out.push(f); }
        }
        out
    };
    for (i, f) in shown.iter().enumerate() {
        let accel = (shortcuts && i < 9).then(|| format!("CmdOrCtrl+{}", i + 1));
        let label = if f.current { format!("✓ {}", f.name) } else { f.name.clone() };
        folders_menu.append(&MenuItem::with_id(app, format!("menu_folder:{}", f.uuid), label, true, accel)?)?;
    }
    if shown.len() < list.folders.len() {
        folders_menu.append(&PredefinedMenuItem::separator(app)?)?;
        folders_menu.append(&MenuItem::with_id(app, "menu_all_folders", format!("All {} Folders…", list.folders.len()), true, None::<&str>)?)?;
    }

    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    let view_menu = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &item("menu_sidebar", "Toggle Sidebar")?,
            &item("menu_refresh", "Refresh")?,
            &PredefinedMenuItem::separator(app)?,
            &item("menu_text_default", "Default Text Size")?,
            &item("menu_text_bigger", "Bigger")?,
            &item("menu_text_smaller", "Smaller")?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;

    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, Some("Close Window"))?,
        ],
    )?;

    let help_menu = Submenu::with_items(
        app,
        "Help",
        true,
        &[
            &MenuItem::with_id(app, "menu_shortcuts", "Keyboard Shortcuts", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "menu_about", "About Geniuz", true, None::<&str>)?,
            &MenuItem::with_id(app, "menu_website", "Visit geniuz.life", true, None::<&str>)?,
        ],
    )?;

    Menu::with_items(
        app,
        &[&file_menu, &edit_menu, &folders_menu, &view_menu, &window_menu, &help_menu],
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Open a link from a memory in the viewer's browser. Only web addresses:
/// a memory is words someone wrote, and a click must never run anything
/// local.
#[tauri::command]
fn open_link(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let u = url.trim();
    if !(u.starts_with("https://") || u.starts_with("http://")) {
        return Err("Only web links open from a memory.".into());
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(u, None::<&str>).map_err(|e| e.to_string())
}

/// Download, from the Status page: the same as the tray's. On Windows that is
/// the installer itself when it answers, and the latest release's page when it
/// doesn't; elsewhere the page. Off the caller's thread, since it asks GitHub.
#[tauri::command]
fn open_update() {
    std::thread::spawn(|| {
        let link = geniuz::update::link_to_open(&geniuz::update::download_link());
        if let Err(e) = geniuz::update::open(&link) {
            eprintln!("[geniuz] opening the update failed: {e}");
        }
    });
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Re-launch attempt: focus the existing window instead of
            // spawning a new process. Fixes the "taskbar click opens
            // another dashboard window" bug on Windows.
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_deep_link::init())
        .invoke_handler(tauri::generate_handler![
            open_link,
            open_update,
            check_update_now,
            take_startup_nav,
            get_image,
            get_app_version,
            get_update,
            view_folder,
            remember_memory,
            get_folder_stats,
            get_recent_memories,
            get_activity,
            semantic_search,
            keyword_search,
            find,
            get_memory_detail,
            get_thread_chain,
            get_status,
            get_settings,
            update_settings,
            export_memory_db_to,
            folder_disk_bytes,
            get_data_dir,
            list_folders,
            bind_current_folder,
            create_folder,
            update_folder,
            add_folders,
            move_folder,
            archive_folder,
            restore_folder,
            pin_folder,
            resolve_address,
            get_shortcuts,
            set_shortcut,
            refresh_menu,
        ])
        .setup(|app| {
            let handle = app.handle();
            let menu = build_menu(handle, true)?;
            app.set_menu(menu)?;

            // A link naming a page: go there, now if the page is up, else when it asks.
            {
                use tauri::{Emitter, Manager};
                use tauri_plugin_deep_link::DeepLinkExt;
                let go = handle.clone();
                let arrive = move |url: &str| {
                    let Some(id) = nav_for_link(url) else { return };
                    let listening = PAGE_LISTENING.load(std::sync::atomic::Ordering::SeqCst);
                    if !listening {
                        if let Ok(mut pending) = PENDING_NAV.lock() {
                            *pending = Some(id);
                        }
                    }
                    if let Some(window) = go.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                        if listening {
                            let _ = window.emit("tray-nav", id);
                        }
                    }
                };
                if let Ok(Some(urls)) = app.deep_link().get_current() {
                    urls.iter().for_each(|u| arrive(u.as_str()));
                }
                app.deep_link().on_open_url(move |event| event.urls().iter().for_each(|u| arrive(u.as_str())));
            }

            // The text size chosen last time.
            let size = saved_text_size();
            if (size - 1.0).abs() > 0.001 {
                use tauri::Manager;
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.set_zoom(size);
                }
            }

            // Native vibrancy — the window is `transparent: true` in
            // tauri.conf.json; here we ask the OS to fill the chrome with
            // its own translucent material. The dashboard then sits inside
            // the system aesthetic instead of asserting a solid colour.
            //
            // Best-effort: if the OS API isn't available (older macOS,
            // unsupported Windows build), we trace the error and continue —
            // the dashboard still works, it just paints with the body
            // background colour the frontend specifies.
            if let Some(window) = app.get_webview_window("main") {
                #[cfg(target_os = "macos")]
                {
                    use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
                    // Sidebar material — denser than HudWindow (which read as
                    // too transparent), lighter than Menu/Popover. Sits
                    // "between menubar glass and full-translucent window" —
                    // what Finder and Mail use for their navigation panes.
                    if let Err(e) = apply_vibrancy(
                        &window,
                        NSVisualEffectMaterial::Sidebar,
                        Some(NSVisualEffectState::Active),
                        None,
                    ) {
                        eprintln!("[geniuz] vibrancy not applied: {e}");
                    }
                }
                #[cfg(target_os = "windows")]
                {
                    use window_vibrancy::apply_acrylic;
                    if let Err(e) = apply_acrylic(&window, Some((18, 18, 18, 125))) {
                        eprintln!("[geniuz] acrylic not applied: {e}");
                    }
                }
                #[cfg(target_os = "linux")]
                {
                    // No native vibrancy backend on Linux. Paint a
                    // medium-light gray (~Mac sidebar material solid
                    // equivalent) so the alpha-blended surfaces and
                    // hairlines the design relies on have something to
                    // contrast against. Pure white would make 4%-white
                    // surfaces and 8%-black hairlines invisible.
                    use tauri::webview::Color;
                    if let Err(e) = window.set_background_color(Some(Color(228, 228, 232, 255))) {
                        eprintln!("[geniuz] linux background not applied: {e}");
                    }
                    // Tag <html> with `os-linux` once the page is loaded
                    // so CSS can apply solid-color overrides for the
                    // surfaces and rows that lose contrast on a solid
                    // backdrop.
                    let _ = window.eval(
                        "document.documentElement.classList.add('os-linux');"
                    );
                }
            }

            // System tray. Disabled on macOS because the SwiftUI menubar app
            // (desktop/Geniuz/) owns the menubar surface; running a Tauri
            // tray on Mac would double the status item. On Windows + Linux
            // there is no menubar app, so this Tauri tray IS the always-on
            // surface — left-click brings the dashboard window forward,
            // right-click shows the menu.
            #[cfg(not(target_os = "macos"))]
            {
                use tauri::tray::TrayIconBuilder;

                let tray_open = MenuItem::with_id(handle, "tray_open", "Open Dashboard", true, None::<&str>)?;
                let tray_recent = MenuItem::with_id(handle, "tray_recent", "Memories", true, None::<&str>)?;
                let tray_find = MenuItem::with_id(handle, "tray_find", "Find…", true, None::<&str>)?;
                let tray_status = MenuItem::with_id(handle, "tray_status", "Status", true, None::<&str>)?;
                let tray_settings = MenuItem::with_id(handle, "tray_settings", "Settings…", true, None::<&str>)?;
                // A plain item, not PredefinedMenuItem::quit: the menu library has no
                // built-in Quit on Linux and drops it without a word, so the Linux tray
                // had no way out (seen on lunar, 11 Nov 2026). Handled below.
                let tray_quit = MenuItem::with_id(handle, "tray_quit", "Quit Geniuz", true, None::<&str>)?;

                let tray_menu = Menu::with_items(
                    handle,
                    &[
                        &tray_open,
                        &PredefinedMenuItem::separator(handle)?,
                        &tray_recent,
                        &tray_find,
                        &tray_status,
                        &tray_settings,
                        &PredefinedMenuItem::separator(handle)?,
                        &tray_quit,
                    ],
                )?;

                // "Update available": on Windows and Linux this tray does what the
                // Mac menu bar does. Every 5 minutes it asks whether a check is due
                // (daily or weekly, per settings; never when updates are off or set
                // to manual), and when a newer release is known, a line at the top
                // of the menu opens it in the browser. Nothing downloads or installs.
                {
                    let menu = tray_menu.clone();
                    let handle = handle.clone();
                    std::thread::spawn(move || {
                        let mut shown = false;
                        loop {
                            if let Ok(s) = geniuz::settings::Settings::load_strict() {
                                if geniuz::update::due(&s, chrono::Utc::now()) {
                                    let _ = geniuz::update::check();
                                }
                            }
                            if !shown {
                                if let Some(found) = geniuz::update::available() {
                                    // On Windows the link is the installer itself, so say "Download".
                                    let label = format!("Download Geniuz {} (update available)", found.version);
                                    let menu = menu.clone();
                                    let h = handle.clone();
                                    // Menus are changed on the main thread.
                                    let _ = handle.run_on_main_thread(move || {
                                        if let Ok(item) = MenuItem::with_id(&h, "tray_update", label, true, None::<&str>) {
                                            let _ = menu.insert(&item, 0);
                                            if let Ok(sep) = PredefinedMenuItem::separator(&h) {
                                                let _ = menu.insert(&sep, 1);
                                            }
                                        }
                                    });
                                    shown = true;
                                }
                            }
                            std::thread::sleep(std::time::Duration::from_secs(300));
                        }
                    });
                }

                let _tray = TrayIconBuilder::new()
                    .icon(app.default_window_icon().cloned().expect("default window icon"))
                    .tooltip("Geniuz")
                    .menu(&tray_menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| {
                        let id = event.id().0.as_str();
                        if id == "tray_update" {
                            if let Some(found) = geniuz::update::available() {
                                // Checked off the menu's thread: it asks GitHub whether the installer is there.
                                std::thread::spawn(move || {
                                    let _ = geniuz::update::open(&geniuz::update::link_to_open(&found.url));
                                });
                            }
                            return;
                        }
                        if id == "tray_quit" {
                            app.exit(0);
                            return;
                        }
                        let Some(window) = app.get_webview_window("main") else { return };
                        match id {
                            "tray_open" => {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                            "tray_recent" | "tray_find" | "tray_status" | "tray_settings" => {
                                let _ = window.show();
                                let _ = window.set_focus();
                                let _ = window.emit("tray-nav", id);
                            }
                            _ => {}
                        }
                    })
                    .on_tray_icon_event(|tray, event| {
                        use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    })
                    .build(app)?;
            }

            #[cfg(target_os = "macos")]
            let _ = handle; // silence unused warning when tray is disabled on Mac

            Ok(())
        })
        .on_menu_event(|app, event| {
            // Predefined items (undo/redo/cut/copy/paste) are handled by the
            // webview natively and never reach this handler.
            let id = event.id().0.as_str();
            match id {
                // Help items act here, not in JS. The webview route did
                // nothing: window.alert() is not shown by the Tauri webview,
                // and opening the site went through open_path, which the
                // capability file scopes to $HOME paths, so it was denied.
                "menu_about" => {
                    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
                    app.dialog()
                        .message(format!(
                            "Geniuz {}\n\nPersistent memory for AI agents.\nManaged Ventures LLC\nhttps://geniuz.life",
                            env!("CARGO_PKG_VERSION")
                        ))
                        .title("About Geniuz")
                        .kind(MessageDialogKind::Info)
                        .show(|_| {});
                }
                "menu_website" => {
                    use tauri_plugin_opener::OpenerExt;
                    if let Err(e) = app.opener().open_url("https://geniuz.life", None::<&str>) {
                        eprintln!("[geniuz] open website failed: {e}");
                    }
                }
                "menu_text_bigger" | "menu_text_smaller" | "menu_text_default" => change_text_size(app, id),
                // Everything else switches surfaces in the frontend.
                _ => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.emit("menu", id);
                    }
                }
            }
        })
        .on_window_event(|window, event| {
            // On Windows and Linux the tray lives in this app, so closing the
            // window hides it instead of quitting: the tray, and its update
            // check, stay. "Quit Geniuz" in the tray quits; opening Geniuz again
            // brings the window back (see single_instance above).
            #[cfg(not(target_os = "macos"))]
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            #[cfg(target_os = "macos")]
            let _ = (window, event);
        })
        .run(tauri::generate_context!())
        .expect("error while running geniuz-dashboard");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_names_one_of_four_pages_and_nothing_else() {
        assert_eq!(nav_for_link("geniuz://settings"), Some("tray_settings"));
        assert_eq!(nav_for_link("geniuz://settings/"), Some("tray_settings"));
        assert_eq!(nav_for_link("geniuz://status?x=1"), Some("tray_status"));
        assert_eq!(nav_for_link("geniuz://settings/?x=1"), Some("tray_settings"));
        assert_eq!(nav_for_link("geniuz://find/#top"), Some("tray_find"));
        assert_eq!(nav_for_link("geniuz://memories"), Some("tray_recent"));
        for other in ["geniuz://", "geniuz://delete", "geniuz://settings/../x", "https://settings", "settings"] {
            assert_eq!(nav_for_link(other), None, "{other}");
        }
    }

    #[test]
    fn text_size_steps_up_and_down_and_stops_at_the_ends() {
        assert_eq!(step_text_size(1.0, true), 1.1);
        assert_eq!(step_text_size(1.0, false), 0.9);
        assert_eq!(step_text_size(2.0, true), 2.0, "no bigger than the biggest");
        assert_eq!(step_text_size(0.8, false), 0.8, "no smaller than the smallest");
        // A value between steps moves to the next step in that direction.
        assert_eq!(step_text_size(1.3, true), 1.4);
        assert_eq!(step_text_size(1.3, false), 1.25);
        // Nine clicks up from the smallest reach the biggest, never past it.
        let mut z = 0.8;
        for _ in 0..12 { z = step_text_size(z, true); }
        assert_eq!(z, 2.0);
    }

    #[test]
    fn every_shortcut_default_is_different() {
        let mut seen = std::collections::HashSet::new();
        for (id, _, keys) in SHORTCUTS {
            assert!(seen.insert(keys.to_lowercase()), "{id} reuses {keys}");
        }
    }

    /// Export copies the folder picked on the page, not whichever the window shows.
    /// Shares GENIUZ_HOME with the other test here, so both run in one test.
    fn export_takes_the_folder_picked(dir: &std::path::Path) {
        let reg = Registry::from_env();
        let work = reg.create("Work", "", None).unwrap();
        DatabaseManager::open_existing(&work.db_path.to_string_lossy()).unwrap().keep("in work", "w", None).unwrap();
        let out = dir.join("export.db");
        export_memory_db_to(out.to_string_lossy().into_owned(), Some(work.short_id())).unwrap();
        let copy = geniuz::folders::identity_at(&out).unwrap();
        assert_eq!(copy.uuid, work.uuid, "the export is the folder picked");
        assert!(folder_disk_bytes(work.uuid.clone()).unwrap() > 0);
    }

    /// Find's Exact mode returns lines, the harvest and the scope, from the
    /// data folder GENIUZ_HOME names (a scratch one here, never a real one).
    #[test]
    fn find_exact_returns_lines_harvest_and_scope() {
        let dir = std::env::temp_dir().join(format!("geniuz-find-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("GENIUZ_HOME", &dir);
        let db = open_db().unwrap();
        db.keep("Roundup\nCubic reviewed the PR; see 13625F6B:D824ADA7.\nDevin filed issues.", "reviews", None).unwrap();
        db.keep("Asked Devin to retest.", "follow-up", None).unwrap();

        let r = find("Devin".into(), "exact".into(), Some("24h".into()), None, None).unwrap();
        let lines: Vec<&str> = r.lines.iter().map(|l| l.line.as_str()).collect();
        assert_eq!(lines, ["Asked Devin to retest.", "Devin filed issues."]);
        assert!(r.memories.is_empty());
        assert_eq!(r.scope, "searched: 2 memories · since 24h · grep \"Devin\" · 2 lines in 2 memories");

        let r = find("\"the PR\" Cubic".into(), "exact".into(), None, None, None).unwrap();
        assert_eq!(r.lines.len(), 1);
        assert_eq!(r.pointers, ["13625F6B:D824ADA7"]);
        assert!(find("x".into(), "exact".into(), Some("someday".into()), None, None).is_err());
        export_takes_the_folder_picked(&dir);
        std::fs::remove_dir_all(&dir).ok();
    }
}

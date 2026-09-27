// Geniuz dashboard — Tauri commands.
//
// All data access goes through the canonical geniuz library
// (`geniuz::db::DatabaseManager`). The dashboard does NOT open memory.db
// directly or maintain its own data layer. One source of truth for schema
// and query logic, used by the CLI, the MCP server, and this dashboard alike.

use geniuz::db::{DatabaseManager, SignalEntry};
use geniuz::settings::Settings;
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Emitter, Manager};

fn db_path() -> String {
    geniuz::data_dir()
        .join("memory.db")
        .to_string_lossy()
        .into_owned()
}

fn open_db() -> Result<DatabaseManager, String> {
    DatabaseManager::new(&db_path())
}

// -------------------------------------------------------------------------
// Data shapes returned to the frontend
// -------------------------------------------------------------------------

#[derive(Serialize, Default)]
struct StationStats {
    total_memories: usize,
    this_week: usize,
    conversations: usize,
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
    category: Option<String>,
    parent_uuid: Option<String>,
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
    category: Option<String>,
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

/// Persist a new memory. `gist` is optional — if blank/None, the chassis
/// auto-derives one from the first 200 chars of content. Returns the short
/// (8-char) UUID prefix of the new memory.
#[tauri::command]
fn remember_memory(gist: Option<String>, content: String) -> Result<String, String> {
    let db = open_db()?;
    let gist_trimmed = gist.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty());
    db.signal(content.trim(), gist_trimmed, None, None)
        .map(|r| r.uuid)
}

#[tauri::command]
fn get_station_stats() -> Result<StationStats, String> {
    let db = open_db()?;
    let total_memories = db.count()?;
    let this_week = db.count_since_days(7)?;
    let conversations = db.thread_count()?;
    let recent_30 = db.count_since_days(30)?;
    let daily_average_recent = (recent_30 as f64) / 30.0;
    let last_write_iso = db.last_write_timestamp()?;
    let storage_bytes = std::fs::metadata(db_path()).map(|m| m.len()).unwrap_or(0);

    Ok(StationStats {
        total_memories,
        this_week,
        conversations,
        storage_bytes,
        daily_average_recent,
        last_write_iso,
    })
}

#[tauri::command]
fn get_recent_memories(limit: Option<u32>) -> Result<Vec<RecentMemory>, String> {
    let db = open_db()?;
    let entries = db.recent(limit.unwrap_or(24) as usize)?;
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
    let entries = db.semantic_search(&query, limit.unwrap_or(20) as usize)?;
    Ok(entries.into_iter().map(map_recent).collect())
}

#[tauri::command]
fn keyword_search(query: String, limit: Option<u32>) -> Result<Vec<RecentMemory>, String> {
    let db = open_db()?;
    let entries = db.keyword_search(&query, limit.unwrap_or(20) as usize)?;
    Ok(entries.into_iter().map(map_recent).collect())
}

#[tauri::command]
fn get_memory_detail(uuid: String) -> Result<Option<MemoryDetail>, String> {
    let db = open_db()?;
    let entry = db.get_by_uuid_prefix(&uuid)?;
    Ok(entry.map(map_detail))
}

#[tauri::command]
fn get_thread_chain(uuid: String, limit: Option<u32>) -> Result<Vec<RecentMemory>, String> {
    let db = open_db()?;
    let entries = db.thread_for(&uuid, limit.unwrap_or(100) as usize)?;
    Ok(entries.into_iter().map(map_recent).collect())
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
    patch.settings.save()?;
    Ok(Settings::load())
}

#[tauri::command]
fn export_memory_db_to(target_path: String) -> Result<u64, String> {
    let source = db_path();
    std::fs::copy(&source, &target_path)
        .map_err(|e| format!("Copy failed: {}", e))
}

#[tauri::command]
fn get_data_dir() -> String {
    geniuz::data_dir().to_string_lossy().into_owned()
}

// -------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------

/// Extract a category from a gist by splitting on the earliest separator
/// (`|`, `;`, or `:`). Returns `None` when no separator is present, the
/// prefix is empty, or the prefix is too long to be a shelf label (>= 40
/// chars). Matches the TUI's `split_gist` discipline so the dashboard chip
/// and the TUI category render the same way.
fn extract_category(gist: &str) -> Option<String> {
    let positions = [gist.find('|'), gist.find(';'), gist.find(':')];
    let earliest = positions.into_iter().flatten().min()?;
    let cat = gist[..earliest].trim().to_string();
    if cat.is_empty() || cat.len() >= 40 {
        return None;
    }
    Some(cat)
}

fn map_recent(e: SignalEntry) -> RecentMemory {
    let category = extract_category(&e.gist);
    RecentMemory {
        uuid: e.memory_uuid,
        gist: e.gist,
        content: e.content,
        created_at: e.created_at,
        category,
        parent_uuid: e.parent_uuid,
    }
}

fn map_detail(e: SignalEntry) -> MemoryDetail {
    let category = extract_category(&e.gist);
    MemoryDetail {
        uuid: e.memory_uuid,
        gist: e.gist,
        content: e.content,
        created_at: e.created_at,
        parent_uuid: e.parent_uuid,
        category,
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
fn build_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    // File menu — surface-level navigation accelerators
    let mi_find = MenuItem::with_id(app, "menu_find", "Find…", true, Some("CmdOrCtrl+K"))?;
    let mi_recent = MenuItem::with_id(app, "menu_recent", "Show Recent", true, Some("CmdOrCtrl+1"))?;
    let mi_status = MenuItem::with_id(app, "menu_status", "Show Status", true, Some("CmdOrCtrl+2"))?;
    let mi_settings = MenuItem::with_id(app, "menu_settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let mi_export = MenuItem::with_id(app, "menu_export", "Export memory.db…", true, Some("CmdOrCtrl+Shift+E"))?;

    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
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
            &MenuItem::with_id(app, "menu_refresh", "Refresh", true, Some("CmdOrCtrl+R"))?,
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
            &MenuItem::with_id(app, "menu_about", "About Geniuz", true, None::<&str>)?,
            &MenuItem::with_id(app, "menu_website", "Visit geniuz.life", true, None::<&str>)?,
        ],
    )?;

    Menu::with_items(
        app,
        &[&file_menu, &edit_menu, &view_menu, &window_menu, &help_menu],
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
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
            get_app_version,
            remember_memory,
            get_station_stats,
            get_recent_memories,
            get_activity,
            semantic_search,
            keyword_search,
            get_memory_detail,
            get_thread_chain,
            get_status,
            get_settings,
            update_settings,
            export_memory_db_to,
            get_data_dir,
        ])
        .setup(|app| {
            let handle = app.handle();
            let menu = build_menu(handle)?;
            app.set_menu(menu)?;

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
                let tray_quit = PredefinedMenuItem::quit(handle, Some("Quit Geniuz"))?;

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

                let _tray = TrayIconBuilder::new()
                    .icon(app.default_window_icon().cloned().expect("default window icon"))
                    .tooltip("Geniuz")
                    .menu(&tray_menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| {
                        let id = event.id().0.as_str();
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
                // Everything else switches surfaces in the frontend.
                _ => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.emit("menu", id);
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running geniuz-dashboard");
}

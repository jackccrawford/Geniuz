//! Geniuz tray — Windows ambient presence.
//!
//! Design commitment (per Jack, April 18 2026 late evening):
//!   - Tooltip carries live "is it working?" status — memory count + Claude
//!     Desktop connection state.
//!   - Right-click menu carries only what a tooltip can't: identity, recent
//!     memories list, context-sensitive actions (Configure when unconfigured,
//!     Restart-reminder when restart pending), Quit.
//!   - No status window, no About dialog — tooltip + menu is the whole surface.
//!
//! Mirrors Claude Desktop's two-item tray restraint while carrying more
//! status because Geniuz has more to report.

// Don't allocate a console window on Windows — GUI app subsystem.
#![windows_subsystem = "windows"]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tray_icon::{
    menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu},
    TrayIcon, TrayIconBuilder,
};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

// =============================================================================
// Status — snapshot read from disk/config on demand
// =============================================================================

#[derive(Clone, Default)]
struct Status {
    memory_count: usize,
    station_exists: bool,
    recent_gists: Vec<String>,
    claude_configured: bool,
    restart_required: bool,
}

impl Status {
    fn fetch(restart_required: bool) -> Self {
        let station_path = geniuz_memory_db_path();
        let (memory_count, recent_gists, station_exists) = if station_path.exists() {
            match geniuz::db::DatabaseManager::new(&station_path.to_string_lossy()) {
                Ok(db) => {
                    let count = db.count().unwrap_or(0);
                    let gists = db
                        .recent(5)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|e| e.gist)
                        .collect();
                    (count, gists, true)
                }
                Err(_) => (0, Vec::new(), false),
            }
        } else {
            (0, Vec::new(), false)
        };

        Status {
            memory_count,
            station_exists,
            recent_gists,
            claude_configured: check_claude_configured(),
            restart_required,
        }
    }

    fn tooltip(&self) -> String {
        let mem = if self.memory_count == 1 {
            "1 memory".to_string()
        } else {
            format!("{} memories", self.memory_count)
        };
        let claude = if self.claude_configured {
            "Claude Desktop connected"
        } else {
            "Claude Desktop not configured"
        };
        format!("Geniuz — {} · {}", mem, claude)
    }
}

fn geniuz_memory_db_path() -> PathBuf {
    if let Ok(home) = std::env::var("GENIUZ_HOME") {
        return PathBuf::from(home).join("memory.db");
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    PathBuf::from(home).join(".geniuz").join("memory.db")
}

fn claude_config_path() -> PathBuf {
    // %APPDATA%\Claude\claude_desktop_config.json
    let appdata = std::env::var("APPDATA").unwrap_or_default();
    PathBuf::from(appdata)
        .join("Claude")
        .join("claude_desktop_config.json")
}

fn check_claude_configured() -> bool {
    let path = claude_config_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    let Some(servers) = json.get("mcpServers").and_then(|v| v.as_object()) else {
        return false;
    };
    servers
        .keys()
        .any(|k| k.eq_ignore_ascii_case("geniuz"))
}

/// Write the MCP config entry pointing at the bundled geniuz.exe.
/// Returns Ok on success, Err with a human-readable message on failure.
fn configure_claude() -> Result<(), String> {
    let path = claude_config_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("create Claude config dir: {}", e))?;
    }

    let mut config: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    // Resolve geniuz.exe path — it's installed alongside geniuz-tray.exe.
    let exe = std::env::current_exe().map_err(|e| format!("locate tray exe: {}", e))?;
    let app_dir = exe
        .parent()
        .ok_or_else(|| "tray exe has no parent dir".to_string())?;
    let geniuz_exe = app_dir.join("geniuz.exe");

    let servers = config
        .as_object_mut()
        .ok_or_else(|| "config root is not an object".to_string())?
        .entry("mcpServers".to_string())
        .or_insert_with(|| serde_json::json!({}));
    let servers = servers
        .as_object_mut()
        .ok_or_else(|| "mcpServers is not an object".to_string())?;

    servers.insert(
        "Geniuz".to_string(),
        serde_json::json!({
            "command": geniuz_exe.to_string_lossy(),
            "args": ["mcp", "serve"]
        }),
    );

    let pretty = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("serialize config: {}", e))?;
    std::fs::write(&path, pretty).map_err(|e| format!("write Claude config: {}", e))?;
    Ok(())
}

// =============================================================================
// Menu — rebuilt on every right-click so state is always fresh
// =============================================================================

struct MenuBuild {
    menu: Menu,
    configure_id: Option<MenuId>,
    quit_id: MenuId,
    // Submenu items are disabled (no click action); we don't track their IDs.
}

fn build_menu(status: &Status) -> MenuBuild {
    let menu = Menu::new();

    // Header — identity + version, disabled (info only)
    let header = MenuItem::new(
        format!("Geniuz {}", env!("CARGO_PKG_VERSION")),
        false,
        None,
    );
    menu.append(&header).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();

    // Recent memories submenu — only if there are any.
    // Bullet prefix matches the Mac menu bar's secondary-color dot affordance;
    // `•` (U+2022) is the standard menu bullet on Windows too.
    if !status.recent_gists.is_empty() {
        let submenu = Submenu::new("Recent memories", true);
        for gist in &status.recent_gists {
            let shown = truncate_for_menu(gist, 78);
            let item = MenuItem::new(format!("•  {}", shown), false, None);
            submenu.append(&item).ok();
        }
        menu.append(&submenu).ok();
        menu.append(&PredefinedMenuItem::separator()).ok();
    }

    // Context-sensitive action: Configure (only when unconfigured)
    let configure_id = if !status.claude_configured {
        let item = MenuItem::new("Configure Claude Desktop", true, None);
        let id = item.id().clone();
        menu.append(&item).ok();
        menu.append(&PredefinedMenuItem::separator()).ok();
        Some(id)
    } else if status.restart_required {
        // Informational only — disabled, prefixed with warning glyph.
        let warn = MenuItem::new("⚠ Restart Claude Desktop to activate", false, None);
        menu.append(&warn).ok();
        menu.append(&PredefinedMenuItem::separator()).ok();
        None
    } else {
        None
    };

    // Quit — always present
    let quit = MenuItem::new("Quit Geniuz", true, None);
    let quit_id = quit.id().clone();
    menu.append(&quit).ok();

    MenuBuild {
        menu,
        configure_id,
        quit_id,
    }
}

fn truncate_for_menu(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max - 1).collect();
    format!("{}…", truncated)
}

// =============================================================================
// Tray lifecycle — winit event loop owns the message pump
// =============================================================================

struct TrayApp {
    tray: TrayIcon,
    status: Arc<Mutex<Status>>,
    restart_required: Arc<Mutex<bool>>,
    current_configure_id: Arc<Mutex<Option<MenuId>>>,
    current_quit_id: Arc<Mutex<MenuId>>,
    last_tooltip_refresh: std::time::Instant,
}

impl TrayApp {
    fn rebuild_menu(&self) {
        let rr = *self.restart_required.lock().unwrap();
        let fresh = Status::fetch(rr);

        let build = build_menu(&fresh);
        // Update tray menu + tooltip atomically from the fresh status.
        self.tray
            .set_menu(Some(Box::new(build.menu)));
        let _ = self.tray.set_tooltip(Some(fresh.tooltip()));

        *self.status.lock().unwrap() = fresh;
        *self.current_configure_id.lock().unwrap() = build.configure_id;
        *self.current_quit_id.lock().unwrap() = build.quit_id;
    }

    fn refresh_tooltip_only(&mut self) {
        let rr = *self.restart_required.lock().unwrap();
        let fresh = Status::fetch(rr);
        let _ = self.tray.set_tooltip(Some(fresh.tooltip()));
        *self.status.lock().unwrap() = fresh;
        self.last_tooltip_refresh = std::time::Instant::now();
    }
}

impl ApplicationHandler for TrayApp {
    fn resumed(&mut self, _: &ActiveEventLoop) {}

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Drain menu-click events. Each right-click on the tray rebuilds the
        // menu with fresh state before presenting it, so clicks always act on
        // the current reality (not a cached 10-seconds-ago view).
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let configure = self.current_configure_id.lock().unwrap().clone();
            let quit = self.current_quit_id.lock().unwrap().clone();

            if event.id == quit {
                event_loop.exit();
                return;
            }
            if let Some(cfg_id) = configure {
                if event.id == cfg_id {
                    match configure_claude() {
                        Ok(()) => {
                            *self.restart_required.lock().unwrap() = true;
                            // Rebuild so the menu now shows "⚠ Restart" instead
                            // of "Configure Claude Desktop".
                            self.rebuild_menu();
                        }
                        Err(e) => {
                            eprintln!("[geniuz-tray] configure failed: {}", e);
                        }
                    }
                }
            }
        }

        // Heartbeat: refresh the tooltip (and rebuild menu-state) every 5s so
        // hover is always near-live even when the user isn't right-clicking.
        // Menu items aren't rebuilt here — only tooltip — because the menu
        // isn't visible; it rebuilds on right-click via Windows tray mechanics
        // when set_menu is next called.
        if self.last_tooltip_refresh.elapsed() >= Duration::from_secs(5) {
            self.refresh_tooltip_only();
            // Also rebuild the menu so the next right-click is fresh.
            self.rebuild_menu();
        }

        event_loop.set_control_flow(ControlFlow::wait_duration(Duration::from_millis(500)));
    }
}

// =============================================================================
// Tray icon image
// =============================================================================

fn load_tray_icon() -> tray_icon::Icon {
    let png = include_bytes!("../images/tray-icon-teal.png");
    let img = image::load_from_memory(png)
        .expect("tray icon png decode")
        .to_rgba8();
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), w, h).expect("tray icon from rgba")
}

// =============================================================================
// Main
// =============================================================================

fn main() {
    let restart_required = Arc::new(Mutex::new(false));
    let initial_status = Status::fetch(*restart_required.lock().unwrap());

    // Build initial menu + tooltip before the tray is constructed so it starts
    // with the right state.
    let build = build_menu(&initial_status);

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(build.menu))
        .with_tooltip(initial_status.tooltip())
        .with_icon(load_tray_icon())
        .build()
        .expect("tray icon build");

    let status = Arc::new(Mutex::new(initial_status));
    let current_configure_id = Arc::new(Mutex::new(build.configure_id));
    let current_quit_id = Arc::new(Mutex::new(build.quit_id));

    let mut app = TrayApp {
        tray,
        status,
        restart_required,
        current_configure_id,
        current_quit_id,
        last_tooltip_refresh: std::time::Instant::now(),
    };

    let event_loop = EventLoop::new().expect("winit event loop");
    event_loop.set_control_flow(ControlFlow::wait_duration(Duration::from_millis(500)));
    event_loop.run_app(&mut app).expect("tray run");
}

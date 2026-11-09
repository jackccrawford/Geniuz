// api.js — typed wrappers around Tauri invoke calls.
// Every Tauri command the dashboard uses lives here as a named function.
// One file = one map between the JS frontend and the Rust commands in
// desktop/dashboard/src/lib.rs.

function invoke(cmd, args) {
  if (!window.__TAURI__) {
    return Promise.reject(new Error(`[api] Tauri not available; tried to call ${cmd}`));
  }
  return window.__TAURI__.core.invoke(cmd, args);
}

export const isTauri = () => !!window.__TAURI__;

export function getAppVersion() {
  return invoke('get_app_version');
}

/** { checked_at, newer: { version, url } | null } from the menu-bar app's last check, or null if it never checked. */
export function getUpdate() {
  return invoke('get_update');
}

export function bindCurrentFolder() {
  return invoke('bind_current_folder');
}

export function listFolders() {
  return invoke('list_folders');
}

/** Show this folder in this window, and change nothing else: looking
 *  is not choosing where memories go. */
export function viewFolder(uuid) {
  return invoke('view_folder', { uuid });
}

export function createFolder(name, description) {
  return invoke('create_folder', { name, description });
}

export function updateFolder(uuid, name, description) {
  return invoke('update_folder', { uuid, name, description });
}

export function addFolders(path) {
  return invoke('add_folders', { path });
}

export function moveFolder(uuid, to) {
  return invoke('move_folder', { uuid, to });
}

export function archiveFolder(uuid) {
  return invoke('archive_folder', { uuid });
}

export function restoreFolder(uuid) {
  return invoke('restore_folder', { uuid });
}

export function pinFolder(uuid, on) {
  return invoke('pin_folder', { uuid, on });
}

export function resolveAddress(address) {
  return invoke('resolve_address', { address });
}

export function getShortcuts() {
  return invoke('get_shortcuts');
}

export function setShortcut(id, keys) {
  return invoke('set_shortcut', { id, keys });
}

export function refreshMenu(shortcuts = true) {
  return invoke('refresh_menu', { shortcuts });
}

export function getFolderStats() {
  return invoke('get_folder_stats');
}

/** One page of the folder: `oldestFirst` reads from the oldest, `offset` memories in. */
export function getRecentMemories(limit = 24, offset = 0, oldestFirst = false) {
  return invoke('get_recent_memories', { limit, offset, oldestFirst });
}

/**
 * Save a new memory. `gist` is required (one line; spaces alone don't count). Returns the short
 * (8-char) UUID prefix of the new memory.
 */
export function rememberMemory(gist, content, folder, parent = null) {
  return invoke('remember_memory', { gist: gist || null, content, folder: folder || null, parent: parent || null });
}

export function getActivity(days = 14) {
  return invoke('get_activity', { days });
}

export function semanticSearch(query, limit = 20) {
  return invoke('semantic_search', { query, limit });
}

export function keywordSearch(query, limit = 20) {
  return invoke('keyword_search', { query, limit });
}

/**
 * Find — mode 'meaning' | 'exact', since e.g. '7d' or null for all time.
 * Returns: { memories: [...], lines: [{ uuid, created_at, line }],
 *            names: [string], pointers: [string], scope: string }
 */
export function find(query, mode, since = null, limit = null, folders = null) {
  return invoke('find', { query, mode, since, limit, folders });
}

export function getDataDir() {
  return invoke('get_data_dir');
}

export function openPath(path) {
  return invoke('plugin:opener|open_path', { path });
}

// -------------------------------------------------------------------------
// Phase 1 (Detail, Find, Status, Settings, Data&export support)
// -------------------------------------------------------------------------

/**
 * Fetch full details for a single memory by UUID.
 * Returns: { uuid, gist, content, created_at, parent_uuid } | null
 */
export function getMemoryDetail(uuid, folder = null) {
  return invoke('get_memory_detail', { uuid, folder });
}

/**
 * Fetch all memories in the same thread as `uuid`, ordered oldest-first.
 * Returns: Array<{ uuid, gist, content, created_at, parent_uuid }>
 */
export function getThreadChain(uuid, limit = 100) {
  return invoke('get_thread_chain', { uuid, limit });
}

/**
 * Bundled health report for the Status surface.
 * Returns: {
 *   memory_count, embedding_count, indexed_pct, embedding_model,
 *   data_dir, data_dir_bytes,
 *   claude_desktop_configured, claude_desktop_has_geniuz, claude_desktop_config_path
 * }
 */
export function getStatus() {
  return invoke('get_status');
}

/**
 * Current persisted settings. Returns the full Settings struct:
 * { version, launch_at_login, autoupdate_enabled,
 *   update_check_frequency (daily/weekly/manual),
 *   recent_memories_count }
 */
export function getSettings() {
  return invoke('get_settings');
}

/**
 * Persist a new Settings struct (full replace). Returns the loaded value
 * after save (which may differ from the patch if the file rejected a field).
 */
export function updateSettings(settings) {
  return invoke('update_settings', { patch: { settings } });
}

/**
 * Copy memory.db to `target_path` (full path including filename).
 * Returns the number of bytes copied.
 */
export function exportMemoryDbTo(targetPath) {
  return invoke('export_memory_db_to', { targetPath });
}

/** Open a web link from a memory in the viewer's browser. */
export function openLink(url) {
  return invoke('open_link', { url });
}

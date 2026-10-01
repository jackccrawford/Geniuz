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

export function getStationStats() {
  return invoke('get_station_stats');
}

export function getRecentMemories(limit = 24) {
  return invoke('get_recent_memories', { limit });
}

/**
 * Save a new memory. `gist` is optional — null/empty means the chassis
 * auto-derives one from the first 200 chars of content. Returns the short
 * (8-char) UUID prefix of the new memory.
 */
export function rememberMemory(gist, content) {
  return invoke('remember_memory', { gist: gist || null, content });
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
export function find(query, mode, since = null, limit = null) {
  return invoke('find', { query, mode, since, limit });
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
 * Returns: { uuid, gist, content, created_at, parent_uuid, category } | null
 */
export function getMemoryDetail(uuid) {
  return invoke('get_memory_detail', { uuid });
}

/**
 * Fetch all memories in the same thread as `uuid`, ordered oldest-first.
 * Returns: Array<{ uuid, gist, content, created_at, category, parent_uuid }>
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

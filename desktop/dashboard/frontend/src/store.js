// store.js — minimal pub/sub state container.
// Single source of truth for cross-surface app state. Components subscribe
// to changes; surfaces dispatch via setState.

const state = {
  currentSurface: 'recent', // 'recent' | 'remember' | 'find' | 'archived' | 'status' | 'settings' | 'data'
  searchQuery: '',          // active search query (empty = recent view)
  appVersion: null,         // set on bootstrap
  dataDir: null,            // set on bootstrap
  // Recent surface sort direction. 'desc' = newest first (default), 'asc' = oldest first.
  // Memory numbers stay stable per memory regardless of direction.
  sortDirection: 'desc',
};

const listeners = new Set();

export function getState() {
  return state;
}

export function setState(patch) {
  Object.assign(state, patch);
  for (const fn of listeners) fn(state);
}

export function subscribe(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function navigate(surface, extras = {}) {
  setState({ currentSurface: surface, ...extras });
}

// main.js — entry point. Bootstraps the app, mounts sidebar, routes surfaces.

import * as api from './api.js';
import { sidebar, wireCollapse } from './components/sidebar.js';
import * as recent from './surfaces/recent.js';
import * as remember from './surfaces/remember.js';
import * as detail from './surfaces/detail.js';
import * as find from './surfaces/find.js';
import * as status from './surfaces/status.js';
import * as settings from './surfaces/settings.js';
import * as dataSurface from './surfaces/data.js';
import * as placeholder from './surfaces/placeholder.js';
import { subscribe, getState, setState, navigate } from './store.js';

// Tag the document so CSS can target Tauri-specific rules.
if (window.__TAURI__) {
  document.documentElement.classList.add('is-tauri');
}
// A Mac draws our header under its window buttons; other systems keep their
// own title bar, so nothing here makes room for it.
if (/Mac/.test(navigator.platform)) {
  document.documentElement.classList.add('os-mac');
}

let toggleSidebar = () => {};

const SURFACE_MOUNTS = {
  recent: recent.mount,
  remember: remember.mount,
  detail: detail.mount,
  find: find.mount,
  status: status.mount,
  settings: settings.mount,
  data: dataSurface.mount,
};

async function bootstrap() {
  // Bootstrap meta first so the sidebar knows the version + counts.
  let version = '—';
  let dataDir = null;
  let stats = { total_memories: null };
  if (api.isTauri()) {
    try {
      [version, dataDir, stats] = await Promise.all([
        api.getAppVersion(),
        api.getDataDir(),
        api.getStationStats(),
      ]);
    } catch (e) {
      console.error('[geniuz] bootstrap failed:', e);
    }
  }
  setState({ appVersion: version, dataDir });

  // Sidebar
  const appShell = document.getElementById('app-shell');
  appShell.innerHTML = '';
  const aside = sidebar({
    version,
    stats: {
      totalMemories: stats.total_memories ?? null,
      storageBytes: stats.storage_bytes ?? null,
    },
  });
  appShell.appendChild(aside);

  // Main mount target
  const mainHost = document.createElement('div');
  mainHost.id = 'main-host';
  mainHost.style.flex = '1';
  mainHost.style.minWidth = '0';
  mainHost.style.display = 'flex';
  mainHost.style.flexDirection = 'column';
  appShell.appendChild(mainHost);

  // The strip across the top of a Mac window: drags it, and holds the toggle.
  const topbar = document.createElement('div');
  topbar.className = 'window-topbar';
  topbar.setAttribute('data-tauri-drag-region', '');
  document.querySelector('.window-frame').appendChild(topbar);
  toggleSidebar = wireCollapse(appShell, topbar);

  // Re-render whenever the surface changes, then render it. Subscribe
  // first: a surface that changes state while it mounts would otherwise
  // change it before anyone listened, and draw nothing (found in Geniuz
  // Team's dashboard, which shares this code, 2026-10-21).
  subscribe((s) => renderSurface(mainHost));
  await renderSurface(mainHost);
}

let renderToken = 0;
let lastRenderKey = null;
async function renderSurface(mainHost) {
  const s = getState();
  // Re-render when surface OR a navigation input changes
  // (selectedMemoryUuid for detail, sortDirection for memory list ordering).
  // searchQuery is deliberately NOT in the key — find is now submit-based, so
  // typing should mirror to state without triggering a remount + re-search.
  const key = `${s.currentSurface}|${s.selectedMemoryUuid || ''}|${s.sortDirection || 'desc'}`;
  if (key === lastRenderKey) return;
  lastRenderKey = key;
  const myToken = ++renderToken;
  const mounter = SURFACE_MOUNTS[s.currentSurface];
  if (mounter) {
    await mounter(mainHost);
  } else {
    await placeholder.mount(mainHost, s.currentSurface);
  }
  // If another render started while we were mounting, defer to it.
  if (myToken !== renderToken) return;
}

// Wire native menu + tray events from Rust → surface navigation / actions.
// IDs match those declared in setup() in src/lib.rs.
async function wireMenuEvents() {
  if (!window.__TAURI__) return;
  const { listen } = window.__TAURI__.event;

  const handleNav = async (id) => {
    switch (id) {
      case 'menu_recent':
      case 'tray_recent':
        navigate('recent');
        break;
      case 'menu_find':
      case 'tray_find':
        navigate('find');
        break;
      case 'menu_status':
      case 'tray_status':
        navigate('status');
        break;
      case 'menu_settings':
      case 'tray_settings':
        navigate('settings');
        break;
      case 'menu_export':
        navigate('data');
        break;
      case 'menu_sidebar':
        toggleSidebar();
        break;
      case 'menu_refresh':
        // Force a re-render of the current surface via state-key bump.
        setState({ _refresh: Date.now() });
        break;
      // menu_about and menu_website are handled in Rust (src/lib.rs
      // on_menu_event) and never reach the frontend.
      default:
        console.warn('[geniuz] unhandled nav event:', id);
    }
  };

  await listen('menu', (event) => handleNav(event.payload));
  await listen('tray-nav', (event) => handleNav(event.payload));
}

bootstrap();
wireMenuEvents();

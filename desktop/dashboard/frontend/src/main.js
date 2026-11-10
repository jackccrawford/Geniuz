// main.js — entry point. Bootstraps the app, mounts sidebar, routes surfaces.

import * as api from './api.js';
import { sidebar, wireCollapse, keepSidebarPlace } from './components/sidebar.js';
import * as recent from './surfaces/recent.js';
import * as remember from './surfaces/remember.js';
import * as find from './surfaces/find.js';
import * as archived from './surfaces/archived.js';
import * as status from './surfaces/status.js';
import * as settings from './surfaces/settings.js';
import * as dataSurface from './surfaces/data.js';
import * as placeholder from './surfaces/placeholder.js';
import { subscribe, getState, setState, navigate } from './store.js';
import { openMemory } from './components/memoryModal.js';

// A file dropped anywhere must never replace the window: left alone, the web view
// opens it like a browser would, with no way back. A surface that takes drops
// (the Keep form) handles its own; everything else is refused here.
window.addEventListener('dragover', (e) => e.preventDefault());
window.addEventListener('drop', (e) => e.preventDefault());
import { openFolderDialog } from './components/folderDialog.js';

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
  find: find.mount,
  archived: archived.mount,
  status: status.mount,
  settings: settings.mount,
  data: dataSurface.mount,
};

async function bootstrap() {
  // Bootstrap meta first so the sidebar knows the version + counts.
  let version = '—';
  let dataDir = null;
  let stats = { total_memories: null };
  let folders = null;
  if (api.isTauri()) {
    try {
      folders = await api.bindCurrentFolder();
      [version, dataDir] = await Promise.all([
        api.getAppVersion(),
        api.getDataDir(),
      ]);
      stats = await api.getFolderStats();
    } catch (e) {
      console.error('[geniuz] bootstrap failed:', e);
    }
  }
  setState({ appVersion: version, dataDir, folders });
  // The menu lists folders, so it follows whatever this load found.
  if (api.isTauri()) api.refreshMenu(true).catch((e) => console.error('[geniuz] menu:', e));

  // Sidebar
  const appShell = document.getElementById('app-shell');
  appShell.innerHTML = '';
  const aside = sidebar({
    version,
    folders,
    stats: {
      totalMemories: stats.total_memories ?? null,
      storageBytes: stats.storage_bytes ?? null,
    },
  });
  appShell.appendChild(aside);
  requestAnimationFrame(() => keepSidebarPlace(aside));

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
  // change it before anyone listened, and draw nothing.
  subscribe((s) => renderSurface(mainHost));
  await renderSurface(mainHost);
}

let renderToken = 0;
let lastRenderKey = null;
async function renderSurface(mainHost) {
  const s = getState();
  // Re-render when surface OR a navigation input changes
  // (sortDirection for memory list ordering).
  // searchQuery is deliberately NOT in the key — find is now submit-based, so
  // typing should mirror to state without triggering a remount + re-search.
  const key = `${s.currentSurface}|${s.sortDirection || 'desc'}`;
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
      case 'menu_keep':
        navigate('remember');
        break;
      case 'menu_new_folder':
        openFolderDialog();
        break;
      case 'menu_archived':
        navigate('archived');
        break;
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
      case 'menu_all_folders': {
        // The menu shows a shortlist; the sidebar's filter reaches the rest.
        // A collapsed sidebar hides the filter, so open it first.
        if (document.querySelector('.is-collapsed')) (toggleSidebar.reveal || toggleSidebar)();
        requestAnimationFrame(() => {
          const filter = document.querySelector('.sidebar-folders__filter');
          if (filter) { filter.focus(); filter.select(); }
        });
        break;
      }
      case 'menu_refresh':
        // Force a re-render of the current surface via state-key bump.
        setState({ _refresh: Date.now() });
        break;
      // menu_about and menu_website are handled in Rust (src/lib.rs
      // on_menu_event) and never reach the frontend.
      default:
        if (id.startsWith('menu_folder:')) {
          api.viewFolder(id.slice('menu_folder:'.length))
            .then(() => location.reload())
            .catch((e) => console.error('[geniuz] switching folder failed:', e));
          break;
        }
        console.warn('[geniuz] unhandled nav event:', id);
    }
  };

  await listen('menu', (event) => handleNav(event.payload));
  await listen('tray-nav', (event) => handleNav(event.payload));
}

// Memories arrive from agents and the command line while the window is open, so
// the sidebar's counts follow the folders instead of staying as they were at load.
async function refreshFolderCounts(list) {
  const fresh = list || await api.listFolders();
  // A folder made, renamed, archived or moved elsewhere (the command line, an
  // agent) changes the list itself, and the menu lists folders too: reload for
  // that, unless a draft is open. Counts alone are updated in place.
  // A folder can show twice (Recent repeats it in its own group), so compare the
  // set of folders, not the rows.
  const rows = [...document.querySelectorAll('.sidebar-folder[data-folder]')];
  const shown = [...new Set(rows.map((r) => `${r.dataset.folder}:${r.getAttribute('aria-label')}`))].sort().join('|');
  const now = (fresh.folders || []).map((f) => `${f.uuid}:${f.name}:${f.tree || ''}`).sort().join('|');
  const was = (getState().folders?.folders || []).map((f) => `${f.uuid}:${f.name}:${f.tree || ''}`).sort().join('|');
  const listChanged = shown !== (fresh.folders || []).map((f) => `${f.uuid}:${f.name}`).sort().join('|') || (was && was !== now);
  // A folder that was empty at load and now holds memories belongs out of
  // "Show N empty" (and maybe in Recent): that's a change of place, not count.
  const woke = (fresh.folders || []).some((f) => f.memories > 0
    && document.querySelector(`.sidebar-folder.is-empty-folder[data-folder="${f.uuid}"]`));
  // Never reload away work in progress: a draft, or an open dialog (New/Edit folder).
  const busy = getState().currentSurface === 'remember' || document.querySelector('dialog[open]');
  if (rows.length && (listChanged || woke) && !busy) {
    location.reload();
    return fresh;
  }
  for (const f of fresh.folders || []) {
    for (const el of document.querySelectorAll(`.sidebar-item[data-folder="${f.uuid}"] .sidebar-item__count`)) {
      if (el.textContent !== String(f.memories)) el.textContent = f.memories;
    }
  }
  // The sidebar re-sorts by count if that is the order the person chose.
  document.dispatchEvent(new CustomEvent('geniuz:folder-counts', { detail: fresh.folders || [] }));
  return fresh;
}

// The window keeps showing the folder chosen here. A reload here only follows a
// folder that vanished.
window.addEventListener('focus', async () => {
  if (!api.isTauri()) return;
  try {
    const fresh = await refreshFolderCounts();
    // Don't reload away a draft. It's bound to the folder it names, so it stays right.
    if (getState().currentSurface === 'remember') return;
    const shown = (getState().folders?.folders || []).find((f) => f.current)?.uuid;
    const now = fresh.folders.find((f) => f.current)?.uuid;
    if (shown && now && shown !== now) location.reload();
  } catch (e) {
    console.error('[geniuz] checking the folder failed:', e);
  }
});

// And while the window stays open and visible, every 15 seconds.
setInterval(() => {
  if (!api.isTauri() || document.hidden) return;
  refreshFolderCounts().catch((e) => console.error('[geniuz] refreshing counts failed:', e));
}, 15_000);

// An address in any memory's text opens that memory, whichever folder it's in.
document.addEventListener('click', async (ev) => {
  const chip = ev.target.closest?.('.sptr');
  if (!chip) return;
  ev.stopPropagation();
  ev.preventDefault();
  try {
    const target = await api.resolveAddress(chip.textContent.trim());
    if (target) {
      openMemory(target.uuid, { folder: target.folder });
    } else {
      chip.classList.add('is-missing');
      chip.title = 'No memory at this address in your folders';
      // Say so where they clicked: a faded pill alone read as a dead click.
      noteUnderChip(chip, `Not in your folders: ${chip.textContent.trim()}`);
    }
  } catch (e) {
    console.error('[geniuz] opening an address failed:', e);
  }
}, true);

/** A short note just under a pill, gone after a few seconds or on the next click. */
function noteUnderChip(chip, text) {
  document.querySelector('.sptr-note')?.remove();
  const note = document.createElement('div');
  note.className = 'sptr-note';
  note.setAttribute('role', 'status');
  note.textContent = text;
  const r = chip.getBoundingClientRect();
  note.style.left = `${Math.max(8, Math.min(r.left, window.innerWidth - 320))}px`;
  note.style.top = `${r.bottom + 6}px`;
  document.body.appendChild(note);
  const gone = () => {
    note.remove();
    document.removeEventListener('scroll', gone, true);
    document.removeEventListener('mousedown', gone, true);
  };
  setTimeout(gone, 3500);
  document.addEventListener('scroll', gone, true);
  // The next click anywhere dismisses it; added after this click has finished.
  setTimeout(() => document.addEventListener('mousedown', gone, true), 0);
}

bootstrap();
wireMenuEvents();

// sidebar.js — left navigation in Tool register.
// Compact rows, system-icon style, active state via subtle background + accent
// on the icon. Brand at top, footer signature at bottom.

import { navigate, subscribe, getState } from '../store.js';
import * as fmt from '../format.js';
import * as api from '../api.js';
import * as su from '../stream-ui.js';
import { openFolderDialog } from './folderDialog.js';

// Collapsing. Below this width the sidebar collapses to its
// icons on its own, so the window can narrow further. That automatic collapse
// never changes what the person chose: widen again and their choice returns.
const NARROW_BELOW = 900;
const PREF_KEY = 'geniuz.sidebarCollapsed';

function readPref() {
  try { return localStorage.getItem(PREF_KEY) === '1'; } catch { return false; }
}
function writePref(on) {
  try { localStorage.setItem(PREF_KEY, on ? '1' : '0'); } catch { /* still works this session */ }
}

const isMac = () => document.documentElement.classList.contains('os-mac');
const shortcutLabel = () => (isMac() ? '⌘B' : 'Ctrl+B');

/**
 * The toggle, and the state behind it. Wide: it sets and saves the choice.
 * Narrow: it opens the sidebar for now, without touching the saved choice.
 */
export function wireCollapse(shell, topbar) {
  let pref = readPref();
  let narrow = window.innerWidth < NARROW_BELOW;
  let openedWhileNarrow = false;
  // Wide, collapsed by choice, opened for a moment (All folders…): not saved.
  let shownForNow = false;

  const toggles = [];
  const collapsed = () => (narrow ? !openedWhileNarrow : pref && !shownForNow);
  const apply = () => {
    const on = collapsed();
    shell.classList.toggle('is-collapsed', on);
    const word = on ? 'Expand sidebar' : 'Collapse sidebar';
    for (const t of toggles) {
      t.dataset.tip = `${word}  ${shortcutLabel()}`;
      t.setAttribute('aria-label', word);
      t.setAttribute('aria-expanded', String(!on));
    }
  };
  const toggle = () => {
    if (narrow) {
      openedWhileNarrow = !openedWhileNarrow;
    } else if (shownForNow) {
      shownForNow = false; // back to the saved choice, which was collapsed
    } else {
      pref = !pref;
      writePref(pref);
    }
    apply();
  };
  // Open the sidebar for now, leaving the saved choice alone.
  toggle.reveal = () => {
    if (!collapsed()) return;
    if (narrow) openedWhileNarrow = true;
    else shownForNow = true;
    apply();
  };

  const makeToggle = (where, extraClass) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = `sidebar-toggle ${extraClass}`;
    b.innerHTML = ICONS.panel;
    b.addEventListener('click', toggle);
    toggles.push(b);
    where.appendChild(b);
  };
  // On a Mac the toggle sits in the header beside the window buttons. Windows
  // and Linux keep their own title bar, so it heads the sidebar instead.
  if (isMac()) {
    makeToggle(topbar, 'sidebar-toggle--header');
  } else {
    makeToggle(shell.querySelector('.sidebar-brand'), 'sidebar-toggle--brand');
  }

  window.addEventListener('resize', () => {
    const nowNarrow = window.innerWidth < NARROW_BELOW;
    if (nowNarrow !== narrow) {
      narrow = nowNarrow;
      openedWhileNarrow = false;
      shownForNow = false; // a moment's reveal ends with the width it was made in
      apply();
    }
  });
  apply();
  return toggle;
}

const NAV_ITEMS = [
  {
    surfaces: [
      { id: 'remember', label: 'Keep',     icon: 'plus' },
      { id: 'find',     label: 'Find',     icon: 'search' },
    ],
  },
  {
    label: 'Workspace',
    surfaces: [
      { id: 'status',   label: 'Status',         icon: 'check' },
      { id: 'data',     label: 'Data & export',  icon: 'database' },
      { id: 'settings', label: 'Settings',       icon: 'gear' },
    ],
  },
];

const ICONS = {
  archive:  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="5" rx="1"/><path d="M5 9v10a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V9"/><path d="M10 13h4"/></svg>',
  panel:    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4.5" width="18" height="15" rx="3"/><path d="M9 4.5v15"/></svg>',
  plus:     '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M12 5v14"/><path d="M5 12h14"/></svg>',
  search:   '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="7"/><path d="m21 21-4.35-4.35"/></svg>',
  check:    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>',
  database: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5v6c0 1.66 4 3 9 3s9-1.34 9-3V5"/><path d="M3 11v6c0 1.66 4 3 9 3s9-1.34 9-3v-6"/></svg>',
  gear:     '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>',
  // Brand mark: the wall-mini — the miniaturized app icon (3x4 mosaic
  // on teal-navy ground, self-contained color tile). Final canonical form.
  wall:     '<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><defs><radialGradient id="geniuz-mark-grad" cx="28%" cy="18%" r="120%"><stop offset="0%" stop-color="#1E7D8C"/><stop offset="58%" stop-color="#134E5A"/><stop offset="100%" stop-color="#0E3540"/></radialGradient></defs><rect width="24" height="24" rx="5.5" fill="url(#geniuz-mark-grad)"/><rect x="3.97" y="3.97" width="4.73" height="3.31" rx="0.93" fill="#2F94A6"/><rect x="9.63" y="3.97" width="4.73" height="3.31" rx="0.93" fill="#2F94A6"/><rect x="15.30" y="3.97" width="4.73" height="3.31" rx="0.93" fill="#1A6E7C"/><rect x="3.97" y="8.22" width="4.73" height="3.31" rx="0.93" fill="#2F94A6"/><rect x="9.63" y="8.22" width="4.73" height="3.31" rx="0.93" fill="#F2EADA"/><rect x="15.30" y="8.22" width="4.73" height="3.31" rx="0.93" fill="#F2EADA"/><rect x="3.97" y="12.47" width="4.73" height="3.31" rx="0.93" fill="#1A6E7C"/><rect x="9.63" y="12.47" width="4.73" height="3.31" rx="0.93" fill="#F2EADA"/><rect x="15.30" y="12.47" width="4.73" height="3.31" rx="0.93" fill="#E0A43B"/><rect x="3.97" y="16.72" width="4.73" height="3.31" rx="0.93" fill="#1A6E7C"/><rect x="9.63" y="16.72" width="4.73" height="3.31" rx="0.93" fill="#1A6E7C"/><rect x="15.30" y="16.72" width="4.73" height="3.31" rx="0.93" fill="#1A6E7C"/></svg>',
};

const FILTER_FROM = 10;

/** On hover: what it's for, and whether it's alive. */
function folderTip(f) {
  const lines = [];
  if (f.description) lines.push(f.description);
  lines.push(f.last_written ? `Last written ${su.timeFull(f.last_written)}` : 'Nothing kept here yet');
  return lines.join('\n');
}

// Folders replace the Memories row. Choosing one reloads the app, so nothing on
// screen can still be showing the last folder's memories.
function foldersSection(list, buttons) {
  const sec = document.createElement('div');
  sec.className = 'sidebar-section sidebar-section--folders';

  const label = document.createElement('div');
  label.className = 'sidebar-section__label sidebar-folders__label';
  label.innerHTML = `<span>Folders</span>`;
  const add = document.createElement('button');
  add.type = 'button';
  add.className = 'sidebar-folders__add';
  add.setAttribute('aria-label', 'New folder');
  add.title = 'New folder';
  add.innerHTML = ICONS.plus;
  add.addEventListener('click', () => openFolderDialog());
  label.appendChild(add);
  sec.appendChild(label);

  const all = list?.folders || [];
  const rows = document.createElement('div');
  rows.className = 'sidebar-folders';
  // A long list needs finding things in, not just scrolling.
  if (all.length > FILTER_FROM) {
    const filter = document.createElement('input');
    filter.type = 'search';
    filter.className = 'sidebar-folders__filter';
    filter.placeholder = 'Filter folders';
    filter.setAttribute('aria-label', 'Filter folders');
    filter.addEventListener('input', () => {
      const q = filter.value.trim().toLowerCase();
      // While filtering, every group is open and empty folders count too.
      rows.classList.toggle('is-filtering', !!q);
      for (const row of rows.querySelectorAll('.sidebar-folder')) {
        row.hidden = !!q && !row.dataset.tip?.toLowerCase().includes(q);
      }
      // Headings count what matches while filtering, and their totals after.
      for (const group of rows.querySelectorAll('.sidebar-group')) {
        const count = group.querySelector('.sidebar-group__count');
        if (!count) continue;
        count.textContent = q
          ? group.querySelectorAll('.sidebar-folder:not([hidden])').length
          : count.dataset.total;
      }
    });
    sec.appendChild(filter);
  }

  // One flat list while it's short. Past that: the folders the person pinned,
  // then every other folder with the most recently written first. Nothing here
  // sorts folders into kinds; a folder's face says whose it is. Empty folders
  // wait behind a "Show N empty" line.
  if (all.length <= FILTER_FROM) {
    for (const f of all) rows.appendChild(folderButton(f, buttons));
  } else {
    const pinned = all.filter((f) => f.pinned);
    const rest = all
      .filter((f) => !f.pinned)
      .sort((a, b) => (b.last_written || '').localeCompare(a.last_written || ''));
    if (pinned.length) rows.appendChild(folderGroup('pinned', 'Pinned', pinned, buttons, { pinned: true }));
    rows.appendChild(folderGroup('folders', 'Recent first', rest, buttons));
    rows.classList.add('has-groups');
  }
  sec.appendChild(rows);
  return finishFoldersSection(sec, list, buttons);
}

/**
 * Choosing a folder reloads the app (so no screen shows one folder's memories
 * under another's name). Keep the sidebar's place across that reload: each
 * section's scroll is saved as the page goes and restored after, and the
 * folder now showing is brought into view. Call once the
 * sidebar is in the document.
 */
const SCROLL_KEY = 'geniuz.sidebar.scroll';
export function keepSidebarPlace(aside) {
  const boxes = () => [...aside.querySelectorAll('.sidebar-folders, .sidebar-group__body')];
  const keyOf = (el) => (el.classList.contains('sidebar-folders') ? 'list' : el.closest('.sidebar-group')?.dataset.key || '');
  try {
    const saved = JSON.parse(sessionStorage.getItem(SCROLL_KEY) || '{}');
    for (const b of boxes()) if (saved[keyOf(b)]) b.scrollTop = saved[keyOf(b)];
  } catch { /* a convenience */ }
  // The folder now showing is brought into view, except in the recent-first
  // list: its top is the news, and scrolling down to an older folder would hide it.
  const current = aside.querySelector('.sidebar-folder.is-current');
  const inRecentFirst = !!current?.closest('.sidebar-group[data-key="folders"]');
  if (current && current.offsetParent && !inRecentFirst) current.scrollIntoView({ block: 'nearest' });
  const save = () => {
    try {
      const out = {};
      for (const b of boxes()) out[keyOf(b)] = b.scrollTop;
      sessionStorage.setItem(SCROLL_KEY, JSON.stringify(out));
    } catch { /* a convenience */ }
  };
  window.addEventListener('beforeunload', save);
  window.addEventListener('pagehide', save);
}

/** Group open/closed state, remembered per viewer (a convenience, so storage may fail). */
function groupOpen(key) {
  try { return localStorage.getItem(`geniuz.group.${key}`) !== 'closed'; } catch { return true; }
}
function setGroupOpen(key, open) {
  try { localStorage.setItem(`geniuz.group.${key}`, open ? 'open' : 'closed'); } catch { /* fine */ }
}

/** A titled, collapsible run of folders. Empty ones wait behind "Show N empty". */
function folderGroup(key, title, folders, buttons, { pinned = false } = {}) {
  const group = document.createElement('div');
  group.className = `sidebar-group${pinned ? ' sidebar-group--pinned' : ''}`;
  group.dataset.key = key;
  group.classList.toggle('is-closed', !groupOpen(key));
  const head = document.createElement('button');
  head.type = 'button';
  head.className = 'sidebar-group__head';
  head.setAttribute('aria-expanded', String(groupOpen(key)));
  head.innerHTML = `<span class="sidebar-group__caret" aria-hidden="true"></span><span class="sidebar-group__title"></span><span class="sidebar-group__count"></span>`;
  head.querySelector('.sidebar-group__title').textContent = title;
  head.querySelector('.sidebar-group__count').textContent = folders.length;
  head.querySelector('.sidebar-group__count').dataset.total = String(folders.length);
  head.addEventListener('click', () => {
    const open = group.classList.toggle('is-closed') === false;
    head.setAttribute('aria-expanded', String(open));
    setGroupOpen(key, open);
  });
  group.appendChild(head);

  const body = document.createElement('div');
  body.className = 'sidebar-group__body';
  const empty = folders.filter((f) => !f.memories && !f.current && !f.pinned);
  for (const f of folders) {
    const btn = folderButton(f, buttons);
    if (empty.includes(f)) btn.classList.add('is-empty-folder');
    body.appendChild(btn);
  }
  if (empty.length) {
    const more = document.createElement('button');
    more.type = 'button';
    more.className = 'sidebar-group__empties';
    const label = (shown) => (shown ? `Hide ${empty.length} empty` : `Show ${empty.length} empty`);
    more.textContent = label(false);
    more.addEventListener('click', () => {
      const shown = body.classList.toggle('shows-empty');
      more.textContent = label(shown);
    });
    body.appendChild(more);
  }
  group.appendChild(body);
  return group;
}

/**
 * The face a folder has before anyone chooses one: a small mosaic, nine tiles
 * in the colours of the Geniuz mark, laid out from the folder's ID. Every
 * folder gets its own, it never changes, and nobody had to pick it. Most
 * tiles are teal and a few are light, as in the mark, so what the eye learns
 * is where the light ones sit.
 */
const MOSAIC = ['#2F94A6', '#2F94A6', '#2F94A6', '#1A6E7C', '#1A6E7C', '#1A6E7C', '#F2EADA', '#F2EADA', '#E0A43B', '#CF5535'];
function mosaic(uuid) {
  // FNV-1a over the ID, then a fresh draw for each tile.
  let h = 0x811c9dc5;
  for (const ch of uuid) h = Math.imul(h ^ ch.codePointAt(0), 0x01000193) >>> 0;
  const draw = () => {
    h ^= h << 13; h >>>= 0;
    h ^= h >>> 17;
    h ^= h << 5; h >>>= 0;
    return h;
  };
  const tiles = Array.from({ length: 9 }, () => MOSAIC[draw() % MOSAIC.length]);
  // A mosaic with no light tile reads as blank, so one is always lit.
  if (!tiles.some((c) => c === '#F2EADA' || c === '#E0A43B' || c === '#CF5535')) tiles[draw() % 9] = '#F2EADA';
  const cells = tiles
    .map((fill, i) => `<rect x="${2 + (i % 3) * 5}" y="${2 + Math.floor(i / 3) * 5}" width="4" height="4" rx="1" fill="${fill}"/>`)
    .join('');
  return `<svg viewBox="0 0 18 18" xmlns="http://www.w3.org/2000/svg"><rect width="18" height="18" rx="4" fill="#0E3540"/>${cells}</svg>`;
}

/**
 * A folder's face: the emoji its writer chose, or else its mosaic.
 */
function folderFace(f) {
  const face = document.createElement('span');
  face.setAttribute('aria-hidden', 'true');
  if (f.face) {
    face.className = 'sidebar-item__icon sidebar-face is-emoji';
    face.textContent = f.face;
  } else {
    face.className = 'sidebar-item__icon sidebar-face';
    face.innerHTML = mosaic(f.uuid || f.name);
  }
  return face;
}

let openMenu = null;
function closeFolderMenu() {
  if (!openMenu) return;
  openMenu.menu.remove();
  openMenu.more.setAttribute('aria-expanded', 'false');
  openMenu.more.closest('.sidebar-folder')?.classList.remove('has-menu');
  openMenu = null;
}
document.addEventListener('click', (ev) => { if (openMenu && !openMenu.menu.contains(ev.target)) closeFolderMenu(); });
document.addEventListener('keydown', (ev) => {
  if (ev.key !== 'Escape' || !openMenu) return;
  const more = openMenu.more;
  closeFolderMenu();
  more.focus();
});
window.addEventListener('resize', closeFolderMenu);

/** What can be done with a folder, from its row. */
function openFolderMenu(f, more) {
  closeFolderMenu();
  const menu = document.createElement('div');
  menu.className = 'folder-menu';
  menu.setAttribute('role', 'menu');
  menu.setAttribute('aria-label', f.name);
  const item = (label, detail, run) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.setAttribute('role', 'menuitem');
    b.className = 'folder-menu__item';
    const text = document.createElement('span');
    text.textContent = label;
    b.appendChild(text);
    if (detail) {
      const d = document.createElement('span');
      d.className = 'folder-menu__detail';
      d.textContent = detail;
      b.appendChild(d);
    }
    b.addEventListener('click', async () => {
      closeFolderMenu();
      try { await run(); } catch (e) { console.error(`[geniuz] ${label} failed:`, e); }
    });
    menu.appendChild(b);
  };
  item(f.pinned ? 'Unpin' : 'Pin to the top', '', async () => {
    await api.pinFolder(f.uuid, !f.pinned);
    location.reload();
  });
  item('Edit…', '', () => openFolderDialog(f));
  item('Copy ID', f.id, () => navigator.clipboard.writeText(f.id));
  document.body.appendChild(menu);
  const r = more.getBoundingClientRect();
  menu.style.left = `${Math.max(8, Math.min(r.left, window.innerWidth - menu.offsetWidth - 8))}px`;
  menu.style.top = `${Math.max(8, Math.min(r.bottom + 4, window.innerHeight - menu.offsetHeight - 8))}px`;
  more.setAttribute('aria-expanded', 'true');
  more.closest('.sidebar-folder')?.classList.add('has-menu');
  openMenu = { menu, more };
  menu.querySelector('button')?.focus();
}

function folderButton(f, buttons) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = `sidebar-item sidebar-folder${f.current ? ' is-current' : ''}`;
    btn.dataset.tip = f.name;
    btn.title = folderTip(f);
    if (f.pinned) btn.classList.add('is-pinned');
    btn.setAttribute('aria-label', f.name);
    const name = document.createElement('span');
    name.className = 'sidebar-item__label';
    name.textContent = f.name;
    btn.appendChild(folderFace(f));
    btn.appendChild(name);
    // More: shown on hover. Pin, edit and copy the ID live behind it. A span,
    // because a button can't hold another button.
    const more = document.createElement('span');
    more.className = 'sidebar-folder__more';
    more.setAttribute('role', 'button');
    more.setAttribute('aria-haspopup', 'menu');
    more.setAttribute('aria-expanded', 'false');
    more.setAttribute('aria-label', `More for ${f.name}`);
    more.title = 'More';
    more.textContent = '···';
    more.tabIndex = 0;
    const toggleMenu = (ev) => {
      ev.stopPropagation();
      ev.preventDefault();
      if (openMenu?.more === more) closeFolderMenu();
      else openFolderMenu(f, more);
    };
    more.addEventListener('click', toggleMenu);
    // A span doesn't answer Enter or Space on its own, and must not pass them
    // to the row's button underneath.
    more.addEventListener('keydown', (ev) => { if (ev.key === 'Enter' || ev.key === ' ') toggleMenu(ev); });
    more.addEventListener('keyup', (ev) => { if (ev.key === ' ') { ev.stopPropagation(); ev.preventDefault(); } });
    btn.appendChild(more);
    const count = document.createElement('span');
    count.className = 'sidebar-item__count';
    count.textContent = f.memories;
    btn.appendChild(count);
    // So refreshFolderCounts can find this row when memories arrive.
    btn.dataset.folder = f.uuid;
    if (f.current) {
      btn.dataset.surface = 'recent';
      btn.addEventListener('click', () => navigate('recent'));
      buttons.push(btn);
    } else {
      btn.addEventListener('click', async () => {
        try {
          await api.viewFolder(f.uuid);
          location.reload();
        } catch (e) {
          console.error('[geniuz] switching folder failed:', e);
        }
      });
    }
    return btn;
}

function finishFoldersSection(sec, list, buttons) {
  const archived = list?.archived || [];
  if (archived.length) {
    const row = document.createElement('button');
    row.type = 'button';
    row.className = 'sidebar-item sidebar-folders__archived';
    row.innerHTML = `<span class="sidebar-item__icon">${ICONS.archive}</span>
      <span class="sidebar-item__label">Archived</span>
      <span class="sidebar-item__count">${archived.length}</span>`;
    row.dataset.surface = 'archived';
    row.dataset.tip = 'Archived';
    row.setAttribute('aria-label', 'Archived');
    row.addEventListener('click', () => navigate('archived'));
    buttons.push(row);
    sec.appendChild(row);
  }

  const notes = [];
  if (list?.blocked) notes.push(`${list.blocked} blocked as ${list.blocked === 1 ? 'a copy' : 'copies'}`);
  if (list?.unavailable) notes.push(`${list.unavailable} out of reach`);
  if (notes.length) {
    const note = document.createElement('div');
    note.className = 'sidebar-folders__note';
    note.textContent = notes.join(' · ');
    sec.appendChild(note);
  }
  return sec;
}

export function sidebar({ version, stats, folders }) {
  const aside = document.createElement('aside');
  aside.className = 'sidebar';

  const brand = document.createElement('div');
  brand.className = 'sidebar-brand';
  brand.innerHTML = `
    <span class="sidebar-brand__mark">${ICONS.wall}</span>
    <span class="sidebar-brand__name">Geniuz</span>
    <span class="sidebar-brand__version">${version ? `v${version}` : ''}</span>
  `;
  aside.appendChild(brand);

  // Track all rendered button elements so we can update active state on
  // store changes without re-rendering the DOM.
  const buttons = [];

  aside.appendChild(foldersSection(folders, buttons));

  for (const section of NAV_ITEMS) {
    const sec = document.createElement('div');
    sec.className = 'sidebar-section';
    if (section.label) {
      const lbl = document.createElement('div');
      lbl.className = 'sidebar-section__label';
      lbl.textContent = section.label;
      sec.appendChild(lbl);
    }
    for (const item of section.surfaces) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'sidebar-item';
      btn.dataset.surface = item.id;
      btn.dataset.tip = item.label;
      btn.setAttribute('aria-label', item.label);
      const count = item.count ? item.count(stats || {}) : null;
      btn.innerHTML = `
        <span class="sidebar-item__icon">${ICONS[item.icon] || ''}</span>
        <span class="sidebar-item__label">${item.label}</span>
        ${count != null ? `<span class="sidebar-item__count">${count}</span>` : ''}
      `;
      btn.addEventListener('click', () => navigate(item.id));
      buttons.push(btn);
      sec.appendChild(btn);
    }
    aside.appendChild(sec);
  }

  // Footer: always-on storage line. Replaced the "local · private · yours"
  // tagline (a hosted brand promise, out of place in the open-source app) and
  // its green status dot (which competed with the wordmark's clay dot).
  const footer = document.createElement('div');
  footer.className = 'sidebar-footer';
  const [storageNum, storageUnit] = fmt.bytes(stats?.storageBytes);
  // The size is the folder on screen; with several folders, say so.
  const where = (folders?.folders || []).length > 1 ? ' · this folder' : ' on disk';
  footer.textContent = storageUnit ? `${storageNum} ${storageUnit}${where}` : '';
  aside.appendChild(footer);

  const applyActive = (currentSurface) => {
    for (const b of buttons) {
      b.classList.toggle('is-active', b.dataset.surface === currentSurface);
    }
  };
  applyActive(getState().currentSurface);
  subscribe((s) => applyActive(s.currentSurface));

  return aside;
}

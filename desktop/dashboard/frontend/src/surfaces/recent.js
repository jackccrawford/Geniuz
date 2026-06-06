// recent.js — home surface in Tool register.
//
// Stats strip mirrors the menubar popover language (Memories · Week · Threads).
// Memories render as dense rows grouped by Today / Yesterday / Earlier this
// week / Older. Click a row to open detail.
//
// Reference surface for the Tool register: parallel implementers of the other
// five surfaces should match this register (system font, dense layout, hairline
// dividers, hover at --bg-elevated, no decoration that doesn't earn its place).

import * as api from '../api.js';
import * as fmt from '../format.js';
import { navigate, getState, setState } from '../store.js';

export async function mount(container) {
  container.innerHTML = `<div class="surface-loading">Loading your memory…</div>`;

  let stats, recent;
  try {
    [stats, recent] = await Promise.all([
      api.getStationStats(),
      api.getRecentMemories(40),
    ]);
  } catch (e) {
    container.innerHTML = `
      <main class="main">
        <div class="surface-error">
          <h2>Couldn't read your station.</h2>
          <p>${escapeHtml(e?.message || String(e))}</p>
          <p>Expected file: <code>~/.geniuz/memory.db</code></p>
        </div>
      </main>
    `;
    return;
  }

  const root = document.createElement('main');
  root.className = 'main';

  const direction = getState().sortDirection || 'desc';

  // Header
  const header = document.createElement('header');
  header.className = 'main-header';
  header.innerHTML = `
    <h1 class="main-header__title">Your memory</h1>
    <p class="main-header__sub">Everything you've remembered.</p>
  `;
  root.appendChild(header);

  // Stats strip — fixed in the header region (mirrors the menubar popover) so
  // it stays put while the day-grouped list scrolls beneath it, instead of
  // scrolling away with the list.
  const strip = document.createElement('div');
  strip.className = 'stats-strip';
  strip.innerHTML = `
    <div class="stats-strip__cell">
      <div class="stats-strip__value">${fmt.number(stats.total_memories)}</div>
      <div class="stats-strip__label">Memories</div>
    </div>
    <div class="stats-strip__cell">
      <div class="stats-strip__value">${fmt.number(stats.this_week)}</div>
      <div class="stats-strip__label">This week</div>
    </div>
    <div class="stats-strip__cell">
      <div class="stats-strip__value">${fmt.number(stats.conversations)}</div>
      <div class="stats-strip__label">Threads</div>
    </div>
  `;
  root.appendChild(strip);

  // Body — scrolls beneath the fixed header + stats.
  const body = document.createElement('div');
  body.className = 'main-body';
  root.appendChild(body);

  // Memory list — grouped by day-section
  if (!recent || recent.length === 0) {
    const empty = document.createElement('div');
    empty.className = 'surface-empty';
    empty.innerHTML = `
      <p>No memories yet.</p>
      <p>Click <button type="button" class="surface-empty__link" data-action="go-remember">Remember</button> in the sidebar to save your first one.</p>
    `;
    const goBtn = empty.querySelector('[data-action="go-remember"]');
    if (goBtn) goBtn.addEventListener('click', () => navigate('remember'));
    body.appendChild(empty);
    container.innerHTML = '';
    container.appendChild(root);
    return;
  }

  // List controls — sort toggle. Sits above the day-grouped list.
  const controls = document.createElement('div');
  controls.className = 'list-controls';
  const sortBtn = document.createElement('button');
  sortBtn.type = 'button';
  sortBtn.className = 'sort-toggle';
  const arrowDown = `<svg class="sort-toggle__icon" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6 2v8"/><path d="M3 7l3 3 3-3"/></svg>`;
  const arrowUp   = `<svg class="sort-toggle__icon" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6 10V2"/><path d="M3 5l3-3 3 3"/></svg>`;
  sortBtn.innerHTML = direction === 'desc'
    ? `${arrowDown}<span>Newest first</span>`
    : `${arrowUp}<span>Oldest first</span>`;
  sortBtn.addEventListener('click', () => {
    setState({ sortDirection: direction === 'desc' ? 'asc' : 'desc' });
  });
  controls.appendChild(sortBtn);
  // Fixed in the header band (inserted before the scroll body) so the sort
  // toggle stays put instead of sliding up under the stats when scrolling.
  root.insertBefore(controls, body);

  // Memory "number": newest = total count, counts down stably regardless of
  // sort direction. The number is assigned by position in newest-first order,
  // so flipping to oldest-first just reverses the visual list — each memory
  // keeps its number.
  const total = stats.total_memories ?? recent.length;
  const selectedUuid = getState().selectedMemoryUuid;

  // Number each memory once based on its newest-first position. Then reverse
  // the working list if asc — numbers stay stable per memory.
  const numbered = recent.map((m, i) => ({ ...m, _num: total - i }));
  const working = direction === 'asc' ? [...numbered].reverse() : numbered;

  // Day-section headers (Today / Yesterday / Older) made the list feel busy
  // when most memories were in one bucket — two stacked uppercase labels
  // under the sort toggle. The per-row date already says when. Render the
  // working list flat; grouping is still used for direction-reversal so
  // sections stay in chronological order when ASC.
  const groups = groupByDaySection(working, direction);
  for (const [, items] of groups) {
    const section = document.createElement('section');
    section.className = 'day-section';

    const list = document.createElement('div');
    list.className = 'memory-list';
    for (const m of items) {
      const row = document.createElement('button');
      row.type = 'button';
      row.className = m.uuid === selectedUuid ? 'memory-row is-current' : 'memory-row';
      // Category lives inline in the gist (formatGist bolds the prefix).
      // No separate chip — one visual anchor per row, matching the TUI.
      row.innerHTML = `
        <div class="memory-row__meta">
          <span class="memory-row__num">#${m._num}</span>
          <span class="memory-row__sep">·</span>
          <span class="memory-row__date">${escapeHtml(fmt.dateTime(m.created_at))}</span>
        </div>
        <div class="memory-row__gist">${formatGist(m.gist || '(no gist)')}</div>
      `;
      row.addEventListener('click', () => navigate('detail', { selectedMemoryUuid: m.uuid }));
      list.appendChild(row);
    }
    section.appendChild(list);
    body.appendChild(section);
  }

  container.innerHTML = '';
  container.appendChild(root);
}

// ---- helpers ---------------------------------------------------------------

function groupByDaySection(memories, direction = 'desc') {
  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const startOfYesterday = startOfToday - 24 * 60 * 60 * 1000;
  const startOfThisWeek = startOfToday - 6 * 24 * 60 * 60 * 1000;

  const today = [];
  const yesterday = [];
  const thisWeek = [];
  const older = [];

  for (const m of memories) {
    const d = fmt.parseSqliteIso(m.created_at);
    const ts = d ? d.getTime() : 0;
    if (ts >= startOfToday)         today.push(m);
    else if (ts >= startOfYesterday) yesterday.push(m);
    else if (ts >= startOfThisWeek)  thisWeek.push(m);
    else                              older.push(m);
  }

  // In asc order, oldest sections come first.
  const desc = [];
  if (today.length)     desc.push(['Today', today]);
  if (yesterday.length) desc.push(['Yesterday', yesterday]);
  if (thisWeek.length)  desc.push(['Earlier this week', thisWeek]);
  if (older.length)     desc.push(['Older', older]);
  return direction === 'asc' ? desc.reverse() : desc;
}

// Bold the convention-driven gist category — the short prefix before the
// first colon (e.g. "substrate-discipline: failed to recall_recent..."
// renders the leading "substrate-discipline" in semibold). Only bolds when
// the colon falls within the first 40 characters and the prefix has no
// internal spaces of more than one word — that filters out unrelated
// colons inside long sentences ("12:00 PM", "use ratio of 3:1", etc.).
function formatGist(gist) {
  // Find the earliest separator: |, ;, or :. Matches the TUI's split_gist
  // discipline so the dashboard's inline-bold category and the TUI's cyan
  // category render the same way.
  const positions = ['|', ';', ':']
    .map((c) => gist.indexOf(c))
    .filter((i) => i > 0 && i < 40);
  if (positions.length === 0) return escapeHtml(gist);
  const sepIdx = Math.min(...positions);
  const prefix = gist.slice(0, sepIdx);
  // Require the prefix to look like a short tag: no more than 3 words.
  const wordCount = prefix.trim().split(/\s+/).length;
  if (wordCount > 3 || prefix.trim().length === 0) {
    return escapeHtml(gist);
  }
  return `<strong class="memory-row__gist-tag">${escapeHtml(prefix)}</strong>${escapeHtml(gist.slice(sepIdx))}`;
}

function escapeHtml(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

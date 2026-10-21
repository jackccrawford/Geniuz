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
import * as su from '../stream-ui.js';

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

  // The stream (2026-10-21, from Geniuz Team's room): day dividers, and each
  // memory read in place: its category's mark, the gist as the header line,
  // the words themselves (links, pointers and code drawn), long ones clamped,
  // a reply saying what it answers, a memory saying how many answered it.
  const key8 = (u) => String(u || '').slice(0, 8).toUpperCase();
  const byKey = new Map(numbered.map((m) => [key8(m.uuid), m]));
  const replyCount = new Map();
  for (const m of numbered) {
    const p = key8(m.parent_uuid);
    if (p && p !== key8(m.uuid) && byKey.has(p)) replyCount.set(p, (replyCount.get(p) || 0) + 1);
  }
  const stream = document.createElement('div');
  stream.className = 'stream';
  let html = '';
  let lastDay = '';
  for (const m of working) {
    const day = su.dayKey(m.created_at);
    if (day !== lastDay) {
      html += `<div class="s-day"><span>${su.esc(su.dayLabel(m.created_at))}</span></div>`;
      lastDay = day;
    }
    const { tag, rest } = su.splitGist(m.gist || '');
    const parent = byKey.get(key8(m.parent_uuid));
    const isReply = parent && key8(parent.uuid) !== key8(m.uuid);
    const replies = replyCount.get(key8(m.uuid)) || 0;
    const text = (m.content || '').trim();
    const head = tag
      ? `<b class="smsg__tag">${su.esc(tag)}</b><span class="smsg__gist">${su.esc(rest)}</span>`
      : `<b class="smsg__tag smsg__tag--plain">${su.esc(m.gist || '(no gist)')}</b>`;
    html += `
      <article class="smsg${m.uuid === selectedUuid ? ' is-current' : ''}" data-uuid="${su.esc(m.uuid)}" tabindex="0">
        <div class="smsg__gutter">${su.mark(tag)}</div>
        <div class="smsg__body">
          <div class="smsg__head">${head}
            <time datetime="${su.esc(m.created_at)}" title="${su.esc(su.timeFull(m.created_at))}">${su.esc(su.timeShort(m.created_at))}</time>
            <span class="smsg__num">#${m._num}</span></div>
          ${isReply ? `<div class="smsg__re">↳ answers #${parent._num} · ${su.esc((parent.gist || '').slice(0, 80))}</div>` : ''}
          ${text && text !== (m.gist || '').trim() ? `<div class="smsg__text">${su.renderText(text)}</div><button type="button" class="smsg__more" hidden>Read all</button>` : ''}
          ${replies ? `<div class="smsg__replies">${replies} ${replies === 1 ? 'reply' : 'replies'}</div>` : ''}
        </div>
      </article>`;
  }
  stream.innerHTML = html;
  stream.addEventListener('click', (ev) => {
    const link = ev.target.closest('.slink');
    if (link) {
      ev.preventDefault();
      api.openLink(link.dataset.href).catch((e) => console.error(e));
      return;
    }
    const card = ev.target.closest('.smsg');
    if (card) navigate('detail', { selectedMemoryUuid: card.dataset.uuid });
  });
  stream.addEventListener('keydown', (ev) => {
    const card = ev.target.closest('.smsg');
    if (card && ev.key === 'Enter') navigate('detail', { selectedMemoryUuid: card.dataset.uuid });
  });
  body.appendChild(stream);
  // "Read all" only where the clamp actually cut something.
  requestAnimationFrame(() => {
    for (const t of stream.querySelectorAll('.smsg__text')) {
      if (t.scrollHeight > t.clientHeight + 2) t.nextElementSibling.hidden = false;
    }
  });

  container.innerHTML = '';
  container.appendChild(root);
}

// ---- helpers ---------------------------------------------------------------

function escapeHtml(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

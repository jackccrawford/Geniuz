// recent.js — home surface in Tool register.
//
// The header carries the two numbers a person uses: how many, and how lately.
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
import { openMemory } from '../components/memoryModal.js';

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
          <h2>Couldn't read your memory.</h2>
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
    <p class="main-header__sub">${fmt.number(stats.total_memories)} ${stats.total_memories === 1 ? 'memory' : 'memories'} · ${fmt.number(stats.this_week)} this week</p>
  `;
  root.appendChild(header);

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

  // Number each memory once based on its newest-first position. Then reverse
  // the working list if asc — numbers stay stable per memory.
  const numbered = recent.map((m, i) => ({ ...m, _num: total - i }));
  const working = direction === 'asc' ? [...numbered].reverse() : numbered;

  // The stream (2026-10-21, from Geniuz Team's room), with threads as a tree
  // (Jack: "for threads, I suggest tree navigation, like windows file
  // manager"). The top level holds memories that answer nothing on screen,
  // ordered by their thread's latest activity; a thread folds under its first
  // memory behind a disclosure arrow, and opening it draws the whole thread
  // as a tree that folds at every level. What is open stays open.
  const key8 = (u) => String(u || '').slice(0, 8).toUpperCase();
  const byKey = new Map(numbered.map((m) => [key8(m.uuid), m]));
  const parentOf = (m) => { const p = key8(m.parent_uuid); return p && p !== key8(m.uuid) ? p : null; };
  const kids = new Map();
  for (const m of numbered) {
    const p = parentOf(m);
    if (p && byKey.has(p)) {
      if (!kids.has(p)) kids.set(p, []);
      kids.get(p).push(m);
    }
  }
  const subtree = (k, seen = new Set()) => {
    if (seen.has(k)) return [];
    seen.add(k);
    const out = [];
    for (const c of kids.get(k) || []) out.push(c, ...subtree(key8(c.uuid), seen));
    return out;
  };
  const top = numbered.filter((m) => !(parentOf(m) && byKey.has(parentOf(m))));
  const activity = (m) => [m, ...subtree(key8(m.uuid))].reduce((a, x) => (x.created_at > a ? x.created_at : a), m.created_at);
  top.sort((a, b) => (direction === 'asc' ? 1 : -1) * activity(a).localeCompare(activity(b)));

  const OPEN_KEY = 'geniuz.tree.open';
  const open = (() => { try { return new Set(JSON.parse(localStorage.getItem(OPEN_KEY) || '[]')); } catch { return new Set(); } })();
  const saveOpen = () => { try { localStorage.setItem(OPEN_KEY, JSON.stringify([...open].slice(-300))); } catch { /* a convenience */ } };

  const numOf = new Map(numbered.map((m) => [key8(m.uuid), m._num]));
  const card = (m, { depth = 0, here = false, count = 0, opened = false } = {}) => {
    const { tag, rest } = su.splitGist(m.gist || '');
    const text = (m.content || '').trim();
    const num = numOf.get(key8(m.uuid));
    const head = tag
      ? `<b class="smsg__tag">${su.esc(tag)}</b><span class="smsg__gist">${su.esc(rest)}</span>`
      : `<b class="smsg__tag smsg__tag--plain">${su.esc(m.gist || '(no gist)')}</b>`;
    const twisty = count || depth === 0 && parentOf(m)
      ? `<button type="button" class="stwisty${opened ? ' is-open' : ''}" data-twisty="${su.esc(m.uuid)}" aria-expanded="${opened}" aria-label="${opened ? 'Fold' : 'Open'} this thread"><svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true"><path d="M4 2.5 8 6 4 9.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg></button>`
      : `<span class="stwisty stwisty--leaf"></span>`;
    return `
      <article class="smsg${depth ? ' smsg--child' : ''}${here ? ' is-here' : ''}" data-uuid="${su.esc(m.uuid)}" tabindex="0">
        <div class="smsg__gutter">${twisty}${su.mark(tag, depth ? 28 : 36)}</div>
        <div class="smsg__body">
          <div class="smsg__head">${head}
            <time datetime="${su.esc(m.created_at)}" title="${su.esc(su.timeFull(m.created_at))}">${su.esc(su.timeShort(m.created_at))}</time>
            ${num ? `<span class="smsg__num">#${num}</span>` : ''}</div>
          ${depth === 0 && parentOf(m) ? `<div class="smsg__re">↳ part of a thread that began earlier</div>` : ''}
          ${text && text !== (m.gist || '').trim() ? `<div class="smsg__text">${su.renderText(text)}</div><button type="button" class="smsg__more" hidden>Read all</button>` : ''}
          ${count && !opened ? `<button type="button" class="smsg__replies" data-twisty="${su.esc(m.uuid)}">${count} ${count === 1 ? 'reply' : 'replies'}</button>` : ''}
        </div>
      </article>`;
  };

  // A whole thread as a tree: children under their parent, oldest first.
  const treeHtml = (entries, rootKey, hereKey, depth) => {
    const ch = new Map();
    for (const e of entries) {
      const p = parentOf(e);
      if (!p) continue;
      if (!ch.has(p)) ch.set(p, []);
      ch.get(p).push(e);
    }
    const walk = (k, d, seen) => {
      if (seen.has(k)) return '';
      seen.add(k);
      const list = (ch.get(k) || []).slice().sort((a, b) => a.created_at.localeCompare(b.created_at));
      if (!list.length) return '';
      return `<div class="stree" style="--d:${d}">${list.map((c) => {
        const ck = key8(c.uuid);
        const n = (ch.get(ck) || []).length;
        const isOpen = !n || open.has(ck) || ck === hereKey || descends(ck, hereKey);
        return `<div class="stree__node">${card(c, { depth: d, here: ck === hereKey, count: n, opened: isOpen && n > 0 })}${n && isOpen ? walk(ck, d + 1, seen) : ''}</div>`;
      }).join('')}</div>`;
    };
    const parents = new Map(entries.map((e) => [key8(e.uuid), parentOf(e)]));
    const descends = (anc, k) => { let x = parents.get(k); const seen = new Set(); while (x && !seen.has(x)) { if (x === anc) return true; seen.add(x); x = parents.get(x); } return false; };
    return walk(rootKey, depth, new Set());
  };

  const stream = document.createElement('div');
  stream.className = 'stream';
  let html = '';
  let lastDay = '';
  for (const m of top) {
    const when = activity(m);
    const day = su.dayKey(when);
    if (day !== lastDay) {
      html += `<div class="s-day"><span>${su.esc(su.dayLabel(when))}</span></div>`;
      lastDay = day;
    }
    const k = key8(m.uuid);
    const count = subtree(k).length;
    const opened = open.has(k);
    html += `<div class="s-thread" data-key="${k}">${card(m, { count, opened })}<div class="s-thread__tree"></div></div>`;
  }
  stream.innerHTML = html;

  async function drawTree(threadEl) {
    const k = threadEl.dataset.key;
    const host = threadEl.querySelector('.s-thread__tree');
    const m = byKey.get(k);
    if (!open.has(k)) { host.innerHTML = ''; return; }
    let entries;
    try {
      entries = await api.getThreadChain(m.uuid, 500);
    } catch (e) {
      host.innerHTML = `<p class="s-tree-err">${su.esc(e)}</p>`;
      return;
    }
    for (const e of entries) if (!numOf.has(key8(e.uuid))) e._num = null;
    const rootEntry = entries.find((e) => !parentOf(e) || !entries.some((x) => key8(x.uuid) === parentOf(e))) || entries[0];
    const rootKey = key8(rootEntry.uuid);
    if (rootKey === k) {
      host.innerHTML = treeHtml(entries, k, null, 1);
    } else {
      // A reply whose thread began earlier: the whole tree from its first
      // memory, this one marked where it sits.
      host.innerHTML = `<div class="stree" style="--d:1"><div class="stree__node">${card(rootEntry, { depth: 1, count: 1, opened: true })}${treeHtml(entries, rootKey, k, 2)}</div></div>`;
    }
    clampCheck(host);
  }

  function toggle(uuid) {
    const k = key8(uuid);
    if (open.has(k)) open.delete(k); else open.add(k);
    saveOpen();
    const threadEl = stream.querySelector(`.s-thread[data-key="${k}"]`);
    if (threadEl) {
      const head = threadEl.querySelector(':scope > .smsg');
      head.outerHTML = card(byKey.get(k), { count: subtree(k).length, opened: open.has(k) });
      drawTree(threadEl);
    } else {
      // A fold inside an open tree: redraw that tree.
      const owner = stream.querySelector(`.smsg[data-uuid="${uuid}"]`)?.closest('.s-thread');
      if (owner) drawTree(owner);
    }
  }

  // The memory last read stays marked, so closing the modal leaves you placed.
  function read(card) {
    for (const x of stream.querySelectorAll('.smsg.is-read')) x.classList.remove('is-read');
    card.classList.add('is-read');
    openMemory(card.dataset.uuid);
  }

  stream.addEventListener('click', (ev) => {
    const link = ev.target.closest('.slink');
    if (link) {
      ev.preventDefault();
      api.openLink(link.dataset.href).catch((e) => console.error(e));
      return;
    }
    const tw = ev.target.closest('[data-twisty]');
    if (tw) {
      ev.stopPropagation();
      toggle(tw.dataset.twisty);
      return;
    }
    const c = ev.target.closest('.smsg');
    // Selecting words to copy them is not asking to open the memory.
    if (c && !String(window.getSelection() || '')) read(c);
  });
  stream.addEventListener('keydown', (ev) => {
    const c = ev.target.closest('.smsg');
    if (!c) return;
    if (ev.key === 'Enter') read(c);
    const tw = c.querySelector('[data-twisty]');
    if (tw && ((ev.key === 'ArrowRight' && tw.getAttribute('aria-expanded') === 'false') || (ev.key === 'ArrowLeft' && tw.getAttribute('aria-expanded') === 'true'))) {
      ev.preventDefault();
      toggle(tw.dataset.twisty);
    }
  });
  body.appendChild(stream);

  // "Read all" only where the clamp actually cut something.
  function clampCheck(scope) {
    requestAnimationFrame(() => {
      for (const t of scope.querySelectorAll('.smsg__text')) {
        if (t.scrollHeight > t.clientHeight + 2) t.nextElementSibling.hidden = false;
      }
    });
  }
  clampCheck(stream);
  for (const t of stream.querySelectorAll('.s-thread')) if (open.has(t.dataset.key)) drawTree(t);

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

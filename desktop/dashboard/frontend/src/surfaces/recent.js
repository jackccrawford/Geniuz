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
import { openFolderDialog } from '../components/folderDialog.js';

export async function mount(container) {
  container.innerHTML = `<div class="surface-loading">Loading your memory…</div>`;

  // A page at a time, so a folder of thousands is all reachable. The first page
  // is read in the chosen order, so oldest first starts at the oldest memory.
  const PAGE = 100;
  let stats, recent;
  try {
    [stats, recent] = await Promise.all([
      api.getFolderStats(),
      api.getRecentMemories(PAGE, 0, (getState().sortDirection || 'desc') === 'asc'),
    ]);
  } catch (e) {
    container.innerHTML = `
      <main class="main">
        <div class="surface-error">
          <h2>Couldn't read your memory.</h2>
          <p>${escapeHtml(e?.message || String(e))}</p>
        </div>
      </main>
    `;
    return;
  }

  const root = document.createElement('main');
  root.className = 'main';

  const direction = getState().sortDirection || 'desc';
  const folder = (getState().folders?.folders || []).find((f) => f.current);

  // Header
  const header = document.createElement('header');
  header.className = 'main-header';
  header.innerHTML = `
    <div class="main-header__titles">
      <h1 class="main-header__title">${escapeHtml(folder?.name || 'Your memory')}</h1>
      ${folder ? `<button type="button" class="folder-edit" data-action="edit-folder" aria-label="Edit folder" title="Edit the name and description"><svg viewBox="0 0 16 16" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M11.2 2.3a1.6 1.6 0 0 1 2.3 2.3L5.6 12.5 2.5 13.5l1-3.1z"/><path d="M10 3.5l2.5 2.5"/></svg></button>` : ''}
    </div>
    ${folder?.description ? `<p class="folder-description">${escapeHtml(folder.description)}</p>` : ''}
    <p class="main-header__sub">${fmt.number(stats.total_memories)} ${stats.total_memories === 1 ? 'memory' : 'memories'} · ${fmt.number(stats.this_week)} this week</p>
  `;
  header.querySelector('[data-action="edit-folder"]')?.addEventListener('click', () => openFolderDialog(folder));
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
      <p class="folder-welcome__title">${escapeHtml(folder?.name || 'This folder')} is empty.</p>
      ${folder?.description ? '' : '<p>What should whoever comes here next know? Say so in its description.</p>'}
      <div class="folder-welcome__actions">
        <button type="button" class="btn btn--primary" data-action="go-remember">Keep something here</button>
        ${folder?.description ? '' : '<button type="button" class="btn btn--ghost" data-action="edit-folder">Add a description</button>'}
      </div>
    `;
    empty.querySelector('[data-action="edit-folder"]')?.addEventListener('click', () => openFolderDialog(folder));
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

  // Memory "number": the oldest is #1 and the newest is the total, whichever way
  // the list runs, so a memory keeps its number when the order flips. Pages
  // arrive in the order shown, so a memory's number follows from its position.
  const total = stats.total_memories ?? recent.length;
  const numberAt = (i) => (direction === 'asc' ? i + 1 : total - i);
  let loaded = recent;
  let served = recent.length;
  let more = recent.length === PAGE;

  // The stream, with threads as a tree, like a file manager's folders.
  // The top level holds memories that answer nothing on screen,
  // ordered by their thread's latest activity; a thread folds under its first
  // memory behind a disclosure arrow, and opening it draws the whole thread
  // as a tree that folds at every level. What is open stays open.
  const key8 = (u) => String(u || '').slice(0, 8).toUpperCase();
  const parentOf = (m) => { const p = key8(m.parent_uuid); return p && p !== key8(m.uuid) ? p : null; };
  // Rebuilt whenever a page arrives: what's loaded, who answers whom, the top level.
  let numbered, byKey, kids, top, numOf;
  function index() {
    numbered = loaded.map((m, i) => ({ ...m, _num: numberAt(i) }));
    byKey = new Map(numbered.map((m) => [key8(m.uuid), m]));
    kids = new Map();
    for (const m of numbered) {
      const p = parentOf(m);
      if (p && byKey.has(p)) {
        if (!kids.has(p)) kids.set(p, []);
        kids.get(p).push(m);
      }
    }
    top = numbered.filter((m) => !(parentOf(m) && byKey.has(parentOf(m))));
    top.sort((a, b) => (direction === 'asc' ? 1 : -1) * activity(a).localeCompare(activity(b)));
    numOf = new Map(numbered.map((m) => [key8(m.uuid), m._num]));
  }
  const subtree = (k, seen = new Set()) => {
    if (seen.has(k)) return [];
    seen.add(k);
    const out = [];
    for (const c of kids.get(k) || []) out.push(c, ...subtree(key8(c.uuid), seen));
    return out;
  };
  const activity = (m) => [m, ...subtree(key8(m.uuid))].reduce((a, x) => (x.created_at > a ? x.created_at : a), m.created_at);
  index();

  const OPEN_KEY = 'geniuz.tree.open';
  const open = (() => { try { return new Set(JSON.parse(localStorage.getItem(OPEN_KEY) || '[]')); } catch { return new Set(); } })();
  const saveOpen = () => { try { localStorage.setItem(OPEN_KEY, JSON.stringify([...open].slice(-300))); } catch { /* a convenience */ } };

  const card = (m, { depth = 0, here = false, count = 0, opened = false } = {}) => {
    const text = su.bodyAfterGist(m.content, m.gist);
    const num = numOf.get(key8(m.uuid));
    // The gist as written, nothing singled out. Its first words are the writer's, not a category.
    const head = `<span class="smsg__gist">${su.esc(m.gist || '(no gist)')}</span>`;
    const twisty = count || depth === 0 && parentOf(m)
      ? `<button type="button" class="stwisty${opened ? ' is-open' : ''}" data-twisty="${su.esc(m.uuid)}" aria-expanded="${opened}" aria-label="${opened ? 'Fold' : 'Open'} this thread"><svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true"><path d="M4 2.5 8 6 4 9.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg></button>`
      : `<span class="stwisty stwisty--leaf"></span>`;
    return `
      <article class="smsg${depth ? ' smsg--child' : ''}${here ? ' is-here' : ''}" data-uuid="${su.esc(m.uuid)}" tabindex="0">
        <div class="smsg__gutter">${twisty}${su.node(!!parentOf(m))}</div>
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
  // The memory last read, kept across redraws (another page, a tree opening).
  let readUuid = null;
  const markRead = () => {
    if (!readUuid) return;
    for (const x of stream.querySelectorAll(`.smsg[data-uuid="${CSS.escape(readUuid)}"]`)) x.classList.add('is-read');
  };
  function drawStream() {
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
    markRead();
  }
  drawStream();

  async function drawTree(threadEl) {
    const k = threadEl.dataset.key;
    const host = threadEl.querySelector('.s-thread__tree');
    const m = byKey.get(k);
    if (!open.has(k)) { host.innerHTML = ''; return; }
    // Whether this draw belongs to an Expand all, decided now, not after the wait.
    const all = expandingAll;
    let entries;
    try {
      entries = await api.getThreadChain(m.uuid, 500);
    } catch (e) {
      host.innerHTML = `<p class="s-tree-err">${su.esc(e)}</p>`;
      return;
    }
    // Folded while this was loading (Collapse all, or a click): stay folded.
    if (!open.has(k)) { host.innerHTML = ''; return; }
    for (const e of entries) if (!numOf.has(key8(e.uuid))) e._num = null;
    // Expand all reaches every depth: open each reply that has replies of its own.
    if (all) {
      const parentsHere = new Set(entries.map(parentOf).filter(Boolean));
      for (const e of entries) if (parentsHere.has(key8(e.uuid))) open.add(key8(e.uuid));
    }
    const rootEntry = entries.find((e) => !parentOf(e) || !entries.some((x) => key8(x.uuid) === parentOf(e))) || entries[0];
    const rootKey = key8(rootEntry.uuid);
    if (rootKey === k) {
      host.innerHTML = treeHtml(entries, k, null, 1);
      markRead();
    } else {
      // A reply whose thread began earlier: the whole tree from its first
      // memory, this one marked where it sits.
      host.innerHTML = `<div class="stree" style="--d:1"><div class="stree__node">${card(rootEntry, { depth: 1, count: 1, opened: true })}${treeHtml(entries, rootKey, k, 2)}</div></div>`;
      markRead();
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
      markRead();
      clampCheck(threadEl);
      drawTree(threadEl);
    } else {
      // A fold inside an open tree: redraw that tree.
      const owner = stream.querySelector(`.smsg[data-uuid="${uuid}"]`)?.closest('.s-thread');
      if (owner) drawTree(owner);
    }
  }

  // Expand all / Collapse all: every thread loaded on screen,
  // at every depth. Folding one afterwards still works, since what's open is
  // the same set a single fold changes.
  let expandingAll = false;
  async function setAll(openEverything) {
    const threads = [...stream.querySelectorAll('.s-thread')];
    if (openEverything) {
      for (const t of threads) if (subtree(t.dataset.key).length) open.add(t.dataset.key);
    } else {
      open.clear();
    }
    for (const t of threads) {
      const k = t.dataset.key;
      t.querySelector(':scope > .smsg').outerHTML = card(byKey.get(k), { count: subtree(k).length, opened: open.has(k) });
      markRead();
    }
    // Re-drawn cards start with "Read all" hidden; show it where text is cut.
    clampCheck(stream);
    expandingAll = openEverything;
    try {
      await Promise.all(threads.map((t) => drawTree(t)));
    } finally {
      expandingAll = false;
      saveOpen();
    }
  }
  if (top.some((m) => subtree(key8(m.uuid)).length)) {
    for (const [label, everything] of [['Expand all', true], ['Collapse all', false]]) {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'sort-toggle list-controls__all';
      b.textContent = label;
      b.addEventListener('click', () => setAll(everything));
      controls.appendChild(b);
    }
  }

  // The memory last read stays marked, so closing the modal leaves you placed.
  function read(card) {
    for (const x of stream.querySelectorAll('.smsg.is-read')) x.classList.remove('is-read');
    card.classList.add('is-read');
    readUuid = card.dataset.uuid;
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

  // The next page arrives as the end of the list comes into view. A memory written
  // while reading shifts the pages by one; one seen twice is kept once.
  const pager = document.createElement('div');
  pager.className = 'stream-pager';
  pager.setAttribute('role', 'status');
  const showPager = (text) => {
    pager.textContent = text ?? (more ? 'Loading more…' : (total > PAGE ? `All ${fmt.number(total)} memories shown` : ''));
  };
  showPager();
  body.appendChild(pager);
  let loading = false;
  async function loadMore() {
    if (!more || loading) return;
    loading = true;
    try {
      // The offset counts rows the server has handed over, not rows kept: memories
      // written while reading shift the newest-first list, and counting only the
      // kept ones could ask for the same page forever.
      const next = await api.getRecentMemories(PAGE, served, direction === 'asc');
      served += next.length;
      const have = new Set(loaded.map((m) => m.uuid));
      loaded = loaded.concat(next.filter((m) => !have.has(m.uuid)));
      more = next.length === PAGE;
      // Keep the reader's place by the thread they're looking at, not by pixels:
      // a reply arriving on this page can move its thread.
      const view = body.getBoundingClientRect();
      const anchor = [...stream.querySelectorAll('.s-thread')].find((t) => t.getBoundingClientRect().bottom > view.top);
      const anchorKey = anchor?.dataset.key;
      const anchorOffset = anchor ? anchor.getBoundingClientRect().top - view.top : 0;
      const y = body.scrollTop;
      index();
      drawStream();
      const again = anchorKey && stream.querySelector(`.s-thread[data-key="${anchorKey}"]`);
      body.scrollTop = again ? body.scrollTop + (again.getBoundingClientRect().top - body.getBoundingClientRect().top) - anchorOffset : y;
      clampCheck(stream);
      for (const t of stream.querySelectorAll('.s-thread')) if (open.has(t.dataset.key)) drawTree(t);
      showPager();
    } catch (e) {
      showPager(`Couldn't load more: ${e?.message || e}`);
      loading = false;
      return; // a failure must not retry in a loop
    } finally {
      loading = false;
    }
    // Replies fold under their threads, so a page can add little height and leave
    // the end still in view; the observer won't fire again by itself, so ask here.
    if (more && pager.getBoundingClientRect().top < body.getBoundingClientRect().bottom + 800) loadMore();
  }
  new IntersectionObserver((seen) => { if (seen.some((x) => x.isIntersecting)) loadMore(); },
    { root: body, rootMargin: '0px 0px 800px 0px' }).observe(pager);

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

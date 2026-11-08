// find.js — Find surface in Tool register.
//
// Large prominent input, mode toggle (Meaning | Exact), a time window, and
// results: Meaning ranks memories (proper names first); Exact returns the
// matching LINES from full content (Ember's dig). Every answer ends with
// where to go next (names seen, pointers) and what was searched
// (search features 1-5). Mode and window are
// local to the surface; the query persists to the store so navigating to
// detail and back restores it.
//
// Mirrors recent.js's discipline: mount(container) signature, escapeHtml,
// handle loading / empty / error states inside the result region.

import * as api from '../api.js';
import * as fmt from '../format.js';
import { getState, setState, navigate } from '../store.js';
import { openMemory } from '../components/memoryModal.js';

const MODES = {
  meaning: {
    label: 'Meaning',
    placeholder: 'Search by meaning… (Capitalized names match exactly)',
  },
  exact: {
    label: 'Exact',
    placeholder: 'Exact words or "a phrase"… every term must appear',
  },
};

// The time window: a value the CLI's --since takes, or null for all time.
const WINDOWS = [
  { label: 'Any time', since: null },
  { label: '24 hours', since: '24h' },
  { label: '7 days', since: '7d' },
  { label: '30 days', since: '30d' },
];

const DEBOUNCE_MS = 250;
// Up to this many folders, chips; past it, the picker (a wall of chips stops
// being readable somewhere around a screenful).
const PICKER_FROM = 8;
const RESULT_LIMIT = 40;

export async function mount(container) {
  // Mount-scoped state. Local `mounted` flag drops late results when the
  // router replaces this surface mid-flight.
  let mounted = true;
  let mode = 'meaning';
  let since = null;
  let query = (getState().searchQuery || '');
  let debounceTimer = null;
  let inFlight = 0; // monotonic ticket; ignore replies older than the latest.
  // Which folders to search: the one on screen, until someone chooses. The choice
  // lasts while the app is open, like the query.
  const allFolders = getState().folders?.folders || [];
  const onScreen = allFolders.find((f) => f.current)?.uuid;
  let chosen = new Set(
    (getState().searchFolders || [onScreen]).filter((id) => allFolders.some((f) => f.uuid === id)),
  );
  if (chosen.size === 0 && onScreen) chosen.add(onScreen);

  // ---- Scaffold -------------------------------------------------------
  const root = document.createElement('main');
  root.className = 'main';

  const header = document.createElement('header');
  header.className = 'main-header';
  header.innerHTML = `
    <h1 class="main-header__title">Find</h1>
    <p class="main-header__sub">Search your folders, by meaning or by exact words.</p>
  `;
  root.appendChild(header);

  const body = document.createElement('div');
  body.className = 'main-body';
  root.appendChild(body);

  // ---- Input + submit + mode toggle ----------------------------------
  const inputWrap = document.createElement('form');
  inputWrap.className = 'find-input-wrap';
  inputWrap.setAttribute('role', 'search');
  const input = document.createElement('input');
  input.type = 'text';
  input.className = 'find-input';
  input.placeholder = MODES[mode].placeholder;
  input.value = query;
  input.autocomplete = 'off';
  input.spellcheck = false;
  inputWrap.appendChild(input);
  const submitBtn = document.createElement('button');
  submitBtn.type = 'submit';
  submitBtn.className = 'find-submit';
  submitBtn.textContent = 'Search';
  inputWrap.appendChild(submitBtn);

  const toggle = document.createElement('div');
  toggle.className = 'find-mode-toggle';
  const modeButtons = {};
  for (const key of Object.keys(MODES)) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.textContent = MODES[key].label;
    if (key === mode) btn.classList.add('is-active');
    btn.addEventListener('click', () => {
      if (mode === key) return;
      mode = key;
      for (const k of Object.keys(modeButtons)) {
        modeButtons[k].classList.toggle('is-active', k === mode);
      }
      input.placeholder = MODES[mode].placeholder;
      // Re-run immediately against the current query on mode change.
      runSearch(query, /* immediate */ true);
      input.focus();
    });
    modeButtons[key] = btn;
    toggle.appendChild(btn);
  }
  const controls = document.createElement('div');
  controls.className = 'find-controls';
  controls.appendChild(toggle);

  const windowToggle = document.createElement('div');
  windowToggle.className = 'find-mode-toggle';
  windowToggle.setAttribute('aria-label', 'Time window');
  const windowButtons = [];
  for (const w of WINDOWS) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.textContent = w.label;
    if (w.since === since) btn.classList.add('is-active');
    btn.addEventListener('click', () => {
      if (since === w.since) return;
      since = w.since;
      windowButtons.forEach((b, i) => b.classList.toggle('is-active', WINDOWS[i].since === since));
      runSearch(query, /* immediate */ true);
    });
    windowButtons.push(btn);
    windowToggle.appendChild(btn);
  }
  controls.appendChild(windowToggle);

  // ---- Which folders --------------------------------------------------
  // Many folders: one line that opens a solid, filterable, grouped list.
  if (allFolders.length > PICKER_FROM) {
    body.appendChild(scopePicker({
      allFolders,
      getChosen: () => chosen,
      setChosen: (next) => {
        chosen = next;
        setState({ searchFolders: [...chosen] });
        runSearch(input.value, /* immediate */ true);
      },
      onScreen,
    }));
  } else if (allFolders.length > 1) {
    const picker = document.createElement('div');
    picker.className = 'find-folders';
    picker.setAttribute('role', 'group');
    picker.setAttribute('aria-label', 'Folders to search');
    const label = document.createElement('span');
    label.className = 'find-folders__label';
    label.textContent = 'Search in';
    picker.appendChild(label);

    const chips = [];
    const paint = () => {
      for (const { btn, id } of chips) {
        const on = id ? chosen.has(id) : chosen.size === allFolders.length;
        btn.classList.toggle('is-active', on);
        btn.setAttribute('aria-pressed', String(on));
      }
    };
    const choose = () => {
      setState({ searchFolders: [...chosen] });
      paint();
      runSearch(input.value, /* immediate */ true);
    };
    const makeChip = (text, id, onClick) => {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'find-folder';
      btn.textContent = text;
      btn.addEventListener('click', onClick);
      chips.push({ btn, id });
      picker.appendChild(btn);
    };
    makeChip('All', null, () => {
      // All on; from all, back to the folder on screen. Never none.
      chosen = chosen.size === allFolders.length
        ? new Set(onScreen ? [onScreen] : [allFolders[0].uuid])
        : new Set(allFolders.map((f) => f.uuid));
      choose();
    });
    for (const f of allFolders) {
      makeChip(f.name, f.uuid, () => {
        if (chosen.has(f.uuid)) {
          if (chosen.size === 1) return; // always at least one
          chosen.delete(f.uuid);
        } else {
          chosen.add(f.uuid);
        }
        choose();
      });
    }
    paint();
    if (allFolders.length > 10) {
      const narrow = document.createElement('input');
      narrow.type = 'search';
      narrow.className = 'find-folders__filter';
      narrow.placeholder = 'Filter folders';
      narrow.setAttribute('aria-label', 'Filter folders');
      narrow.addEventListener('input', () => {
        const q = narrow.value.trim().toLowerCase();
        for (const { btn, id } of chips) {
          btn.hidden = !!id && !!q && !btn.textContent.toLowerCase().includes(q);
        }
      });
      picker.insertBefore(narrow, label.nextSibling);
    }
    body.appendChild(picker);
  }

  body.appendChild(controls);
  body.appendChild(inputWrap);

  // ---- Results region -------------------------------------------------
  const resultsEl = document.createElement('div');
  resultsEl.className = 'find-results';
  body.appendChild(resultsEl);

  // ---- Input wiring ---------------------------------------------------
  // No per-keystroke search — at large db sizes that pays the embed/SQL cost
  // for every character. Submit on Enter or click; track the query in state
  // so navigating away and back restores it.
  input.addEventListener('input', (e) => {
    query = e.target.value;
    setState({ searchQuery: query });
  });
  inputWrap.addEventListener('submit', (e) => {
    e.preventDefault();
    runSearch(input.value, /* immediate */ true);
  });

  // ---- Paint ----------------------------------------------------------
  container.innerHTML = '';
  container.appendChild(root);

  // Focus and place cursor at end of any restored query.
  input.focus();
  if (query) {
    input.setSelectionRange(query.length, query.length);
    runSearch(query, /* immediate */ true);
  } else {
    renderHint();
  }

  // Surface tear-down — when the router replaces our DOM, drop in-flight work.
  const observer = new MutationObserver(() => {
    if (!root.isConnected) {
      mounted = false;
      if (debounceTimer) clearTimeout(debounceTimer);
      observer.disconnect();
    }
  });
  if (container.parentNode) observer.observe(container.parentNode, { childList: true, subtree: true });

  // ---- Search ---------------------------------------------------------
  async function runSearch(rawQ, immediate = false) {
    if (immediate && debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    const q = (rawQ || '').trim();
    if (!q) {
      renderHint();
      return;
    }
    const ticket = ++inFlight;
    renderLoading();
    let results;
    try {
      results = await api.find(q, mode, since, mode === 'exact' ? 100 : RESULT_LIMIT, [...chosen]);
    } catch (e) {
      if (!mounted || ticket !== inFlight) return;
      renderError(e);
      return;
    }
    if (!mounted || ticket !== inFlight) return;
    renderResults(results);
  }

  // ---- Render states --------------------------------------------------
  function renderHint() {
    resultsEl.innerHTML = `
      <div class="surface-empty">
        Type to search. Meaning ranks by similarity, with Capitalized names
        matched exactly and first. Exact shows the lines holding every word.
      </div>
    `;
  }

  function renderLoading() {
    resultsEl.innerHTML = `<div class="surface-loading">Searching…</div>`;
  }

  function renderError(err) {
    resultsEl.innerHTML = `
      <div class="surface-error">
        <h2>Search failed.</h2>
        <p>${escapeHtml(err?.message || String(err))}</p>
      </div>
    `;
  }

  function renderResults(r) {
    resultsEl.innerHTML = '';
    const empty = r.memories.length === 0 && r.lines.length === 0;
    if (empty) {
      const none = document.createElement('div');
      none.className = 'surface-empty';
      none.textContent = chosen.size > 1 ? 'No matches in these folders' : 'No matches in this folder';
      resultsEl.appendChild(none);
    }
    const list = document.createElement('div');
    list.className = 'memory-list';
    // Say where a result came from, whenever it could have come from more than one.
    const from = (name) => (chosen.size > 1 && name ? `<span class="find-row-folder">${escapeHtml(name)}</span>` : '');
    for (const l of r.lines) {
      const row = document.createElement('button');
      row.type = 'button';
      row.className = 'memory-row find-line';
      row.innerHTML = `
        <span class="find-line__id">${escapeHtml(l.uuid.slice(0, 8))}</span>
        <span class="memory-row__gist">${escapeHtml(l.line)}</span>
        ${from(l.folder_name)}
        <span class="memory-row__time">${fmt.ago(l.created_at)}</span>
      `;
      row.addEventListener('click', () => openMemory(l.uuid, { folder: l.folder }));
      list.appendChild(row);
    }
    for (const m of r.memories) {
      const row = document.createElement('button');
      row.type = 'button';
      row.className = 'memory-row';
      // Category lives inline in the gist (formatGist bolds the prefix).
      // No separate chip — one visual anchor per row, matching Recent and TUI.
      row.innerHTML = `
        <span class="memory-row__gist">${formatGist(m.gist || '(no gist)')}</span>
        ${from(m.folder_name)}
        <span class="memory-row__time">${fmt.ago(m.created_at)}</span>
      `;
      row.addEventListener('click', () => openMemory(m.uuid, { folder: m.folder }));
      list.appendChild(row);
    }
    // Where to go next, at the top for a person reading down (agents get it
    // at the end of the text answer): a name digs for it exactly; an id
    // opens it.
    if (r.names.length || r.pointers.length) {
      const next = document.createElement('div');
      next.className = 'find-next';
      if (r.names.length) {
        const label = document.createElement('span');
        label.className = 'find-next__label';
        label.textContent = 'Names seen';
        next.appendChild(label);
        for (const n of r.names) {
          const chip = document.createElement('button');
          chip.type = 'button';
          chip.className = 'find-next__chip';
          chip.textContent = n;
          chip.title = `Find lines naming ${n}`;
          chip.addEventListener('click', () => {
            mode = 'exact';
            for (const k of Object.keys(modeButtons)) modeButtons[k].classList.toggle('is-active', k === mode);
            input.placeholder = MODES[mode].placeholder;
            input.value = n;
            query = n;
            setState({ searchQuery: query });
            runSearch(n, true);
          });
          next.appendChild(chip);
        }
      }
      if (r.pointers.length) {
        const label = document.createElement('span');
        label.className = 'find-next__label';
        label.textContent = 'Pointers';
        next.appendChild(label);
        for (const p of r.pointers) {
          // A pointer opens if it's a memory in a folder searched, or an address
          // whose folder is reachable. The rest are only shown.
          const target = (r.pointer_targets || {})[p];
          const here = !!target;
          const chip = document.createElement(here ? 'button' : 'span');
          chip.className = 'find-next__chip find-next__chip--mono' + (here ? '' : ' find-next__chip--elsewhere');
          chip.textContent = p;
          if (!here) chip.title = 'Not in the folders searched';
          if (here) {
            chip.type = 'button';
            chip.title = 'Open this memory';
            chip.addEventListener('click', () => openMemory(target.uuid, { folder: target.folder }));
          }
          next.appendChild(chip);
        }
      }
      resultsEl.appendChild(next);
    }

    if (!empty) resultsEl.appendChild(list);

    const scope = document.createElement('div');
    scope.className = 'find-scope';
    scope.textContent = r.scope;
    resultsEl.appendChild(scope);
  }
}

// ---- helpers ---------------------------------------------------------------

function formatGist(gist) {
  // Find the earliest separator: |, ;, or :. Matches recent.js and the TUI.
  const positions = ['|', ';', ':']
    .map((c) => gist.indexOf(c))
    .filter((i) => i > 0 && i < 40);
  if (positions.length === 0) return escapeHtml(gist);
  const sepIdx = Math.min(...positions);
  const prefix = gist.slice(0, sepIdx);
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

/**
 * "Search in" for many folders: one line showing what's chosen, which opens a
 * solid list to filter and tick, grouped into Agents and Folders when there are
 * agents' homes. The choice is never empty; "All folders" toggles everything,
 * and from all, back to the folder on screen (as the chips did).
 */
function scopePicker({ allFolders, getChosen, setChosen, onScreen }) {
  const wrap = document.createElement('div');
  wrap.className = 'find-folders find-scope';
  const label = document.createElement('span');
  label.className = 'find-folders__label';
  label.textContent = 'Search in';
  wrap.appendChild(label);

  const summary = document.createElement('div');
  summary.className = 'find-scope__summary';
  wrap.appendChild(summary);

  const trigger = document.createElement('button');
  trigger.type = 'button';
  trigger.className = 'find-scope__trigger';
  trigger.setAttribute('aria-haspopup', 'listbox');
  trigger.setAttribute('aria-expanded', 'false');
  trigger.setAttribute('aria-label', 'Choose folders to search');
  trigger.innerHTML = '<span class="find-scope__caret" aria-hidden="true"></span>';
  wrap.appendChild(trigger);

  const panel = document.createElement('div');
  panel.className = 'find-scope__panel';
  panel.hidden = true;
  const filter = document.createElement('input');
  filter.type = 'search';
  filter.className = 'find-scope__filter';
  filter.placeholder = 'Filter folders';
  filter.setAttribute('aria-label', 'Filter folders');
  panel.appendChild(filter);
  const list = document.createElement('div');
  list.className = 'find-scope__list';
  list.setAttribute('role', 'listbox');
  list.setAttribute('aria-multiselectable', 'true');
  panel.appendChild(list);
  wrap.appendChild(panel);

  const all = () => getChosen().size === allFolders.length;
  const byId = new Map(allFolders.map((f) => [f.uuid, f]));

  const paintSummary = () => {
    summary.innerHTML = '';
    if (all()) {
      const t = document.createElement('button');
      t.type = 'button';
      t.className = 'find-scope__all';
      t.textContent = `All folders · ${allFolders.length}`;
      t.addEventListener('click', () => open());
      summary.appendChild(t);
      return;
    }
    const picked = [...getChosen()].map((id) => byId.get(id)).filter(Boolean);
    for (const f of picked.slice(0, 3)) {
      const chip = document.createElement('span');
      chip.className = 'find-scope__chip';
      const chipName = document.createElement('span');
      chipName.className = 'find-scope__chip-name';
      chipName.textContent = f.name;
      chip.title = f.name;
      chip.appendChild(chipName);
      if (picked.length > 1) {
        const x = document.createElement('button');
        x.type = 'button';
        x.className = 'find-scope__remove';
        x.setAttribute('aria-label', `Stop searching ${f.name}`);
        x.textContent = '×';
        x.addEventListener('click', () => {
          const next = new Set(getChosen());
          next.delete(f.uuid);
          if (next.size) setChosen(next);
          paint();
          // The chip is gone; keep the keyboard in the same place.
          (summary.querySelector('.find-scope__remove') || trigger).focus();
        });
        chip.appendChild(x);
      }
      summary.appendChild(chip);
    }
    if (picked.length > 3) {
      const more = document.createElement('button');
      more.type = 'button';
      more.className = 'find-scope__more';
      more.textContent = `+${picked.length - 3} more`;
      more.addEventListener('click', () => open());
      summary.appendChild(more);
    }
  };

  const rows = [];
  const row = (text, count, id) => {
    const r = document.createElement('button');
    r.type = 'button';
    r.className = 'find-scope__row';
    r.setAttribute('role', 'option');
    r.dataset.name = text.toLowerCase();
    r.innerHTML = '<span class="find-scope__tick" aria-hidden="true"></span><span class="find-scope__name"></span><span class="find-scope__count"></span>';
    r.querySelector('.find-scope__name').textContent = text;
    r.querySelector('.find-scope__count').textContent = count ?? '';
    r.addEventListener('click', () => {
      const cur = getChosen();
      let next;
      if (!id) {
        next = all() ? new Set(onScreen ? [onScreen] : [allFolders[0].uuid]) : new Set(allFolders.map((f) => f.uuid));
      } else {
        next = new Set(cur);
        if (next.has(id)) { if (next.size === 1) return; next.delete(id); } else next.add(id);
      }
      setChosen(next);
      paint();
    });
    rows.push({ r, id });
    return r;
  };

  list.appendChild(row('All folders', allFolders.length, null));
  const hasAgents = allFolders.some((f) => f.tree === 'agents');
  const groups = hasAgents
    ? [['Agents', allFolders.filter((f) => f.tree === 'agents')], ['Shared', allFolders.filter((f) => f.tree !== 'agents')]]
    : [[null, allFolders]];
  const heads = [];
  for (const [title, fs] of groups) {
    if (title) {
      const h = document.createElement('div');
      h.className = 'find-scope__group';
      h.textContent = `${title} · ${fs.length}`;
      list.appendChild(h);
      heads.push({ h, fs });
    }
    for (const f of fs) list.appendChild(row(f.name, f.memories, f.uuid));
  }

  const paintRows = () => {
    for (const { r, id } of rows) {
      const on = id ? getChosen().has(id) : all();
      r.classList.toggle('is-on', on);
      r.setAttribute('aria-selected', String(on));
    }
  };
  const paint = () => { paintSummary(); paintRows(); };

  filter.addEventListener('input', () => {
    const q = filter.value.trim().toLowerCase();
    // "All folders" (no id) stays: choosing everything shouldn't need the filter cleared.
    for (const { r, id } of rows) r.hidden = !!q && !!id && !r.dataset.name.includes(q);
    for (const { h, fs } of heads) h.hidden = !!q && !fs.some((f) => f.name.toLowerCase().includes(q));
  });

  const close = () => {
    panel.hidden = true;
    trigger.setAttribute('aria-expanded', 'false');
    document.removeEventListener('mousedown', outside, true);
  };
  const outside = (ev) => { if (!wrap.contains(ev.target)) close(); };
  const open = () => {
    if (!panel.hidden) return close();
    panel.hidden = false;
    trigger.setAttribute('aria-expanded', 'true');
    filter.value = '';
    filter.dispatchEvent(new Event('input'));
    document.addEventListener('mousedown', outside, true);
    filter.focus();
  };
  trigger.addEventListener('click', open);
  panel.addEventListener('keydown', (ev) => {
    if (ev.key === 'Escape') { ev.stopPropagation(); close(); trigger.focus(); return; }
    if (ev.key !== 'ArrowDown' && ev.key !== 'ArrowUp') return;
    ev.preventDefault();
    const visible = rows.map((x) => x.r).filter((r) => !r.hidden);
    const at = visible.indexOf(document.activeElement);
    const next = ev.key === 'ArrowDown' ? visible[Math.min(at + 1, visible.length - 1)] : (at <= 0 ? filter : visible[at - 1]);
    next?.focus();
  });

  paint();
  return wrap;
}

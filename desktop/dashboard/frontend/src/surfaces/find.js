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

  // ---- Scaffold -------------------------------------------------------
  const root = document.createElement('main');
  root.className = 'main';

  const header = document.createElement('header');
  header.className = 'main-header';
  header.innerHTML = `
    <h1 class="main-header__title">Find</h1>
    <p class="main-header__sub">Search across everything you've remembered, by meaning or by word.</p>
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
  body.appendChild(inputWrap);

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
  body.appendChild(controls);

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
      results = await api.find(q, mode, since, mode === 'exact' ? 100 : RESULT_LIMIT);
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
      none.textContent = 'No matches in your memory';
      resultsEl.appendChild(none);
    }
    const list = document.createElement('div');
    list.className = 'memory-list';
    for (const l of r.lines) {
      const row = document.createElement('button');
      row.type = 'button';
      row.className = 'memory-row find-line';
      row.innerHTML = `
        <span class="find-line__id">${escapeHtml(l.uuid.slice(0, 8))}</span>
        <span class="memory-row__gist">${escapeHtml(l.line)}</span>
        <span class="memory-row__time">${fmt.ago(l.created_at)}</span>
      `;
      row.addEventListener('click', () => navigate('detail', { selectedMemoryUuid: l.uuid }));
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
        <span class="memory-row__time">${fmt.ago(m.created_at)}</span>
      `;
      row.addEventListener('click', () => navigate('detail', { selectedMemoryUuid: m.uuid }));
      list.appendChild(row);
    }
    if (!empty) resultsEl.appendChild(list);

    // Where to go next: a name digs for it exactly; an id opens it.
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
          // A bare id may be a memory here; a signal pointer (A:B) lives
          // on a station elsewhere, so it is shown, not followed.
          const bare = !p.includes(':');
          const chip = document.createElement(bare ? 'button' : 'span');
          chip.className = 'find-next__chip find-next__chip--mono';
          chip.textContent = p;
          if (bare) {
            chip.type = 'button';
            chip.title = 'Open this memory';
            chip.addEventListener('click', () => navigate('detail', { selectedMemoryUuid: p }));
          }
          next.appendChild(chip);
        }
      }
      resultsEl.appendChild(next);
    }

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

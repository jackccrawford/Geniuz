// detail.js — single-memory detail surface in Tool register.
//
// Shows one memory's gist, full content, metadata, and (when present) its
// thread context. Reached by selecting a memory in Recent or Find, which
// writes selectedMemoryUuid into the store.
//
// Matches the Recent reference: <main class="main"> with header + body,
// graceful loading/error/empty states, escapeHtml on any text headed into
// a template string, fmt.* for time. No markdown rendering — content is
// shown as preformatted text via .detail-body's white-space: pre-wrap.

import * as api from '../api.js';
import * as fmt from '../format.js';
import { getState, navigate } from '../store.js';

export async function mount(container) {
  const { selectedMemoryUuid } = getState();

  if (!selectedMemoryUuid) {
    container.innerHTML = `
      <main class="main">
        <div class="surface-empty">No memory selected. Pick one from Recent.</div>
      </main>
    `;
    return;
  }

  container.innerHTML = `<main class="main"><div class="surface-loading">Loading memory…</div></main>`;

  // Detail is required; thread is best-effort. Use Promise.allSettled so a
  // thread-fetch failure doesn't blank the whole surface.
  let memory;
  let thread = [];
  try {
    const [m, t] = await Promise.allSettled([
      api.getMemoryDetail(selectedMemoryUuid),
      api.getThreadChain(selectedMemoryUuid),
    ]);
    if (m.status === 'rejected') throw m.reason;
    memory = m.value;
    if (t.status === 'fulfilled' && Array.isArray(t.value)) thread = t.value;
  } catch (e) {
    container.innerHTML = `
      <main class="main">
        <div class="surface-error">
          <h2>Couldn't load this memory.</h2>
          <p>${escapeHtml(e?.message || String(e))}</p>
        </div>
      </main>
    `;
    return;
  }

  if (!memory) {
    container.innerHTML = `
      <main class="main">
        <div class="surface-error">
          <h2>Memory not found.</h2>
          <p>UUID <code>${escapeHtml(shortUuid(selectedMemoryUuid))}</code> didn't return a record. It may have been archived.</p>
        </div>
      </main>
    `;
    return;
  }

  // ---- Build layout ------------------------------------------------------
  const root = document.createElement('main');
  root.className = 'main';

  // Header — back button (top-left), gist as title, plain-prose metadata.
  const header = document.createElement('header');
  header.className = 'main-header';
  const returnTo = getState().returnTo || 'recent';
  const returnLabel = returnTo === 'find' ? 'Find' : 'Recent';
  header.innerHTML = `
    <button type="button" class="back-button" aria-label="Back to ${returnLabel}">
      <svg class="back-button__icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
        <path d="M10 3L5 8l5 5"/>
      </svg>
      <span>${returnLabel}</span>
    </button>
    <h1 class="main-header__title">${escapeHtml(memory.gist || '(no gist)')}</h1>
    <p class="main-header__sub">${escapeHtml(subtitleLine(memory))}</p>
  `;
  header.querySelector('.back-button').addEventListener('click', () => {
    navigate(returnTo);
  });
  root.appendChild(header);

  // Body
  const body = document.createElement('div');
  body.className = 'main-body';
  root.appendChild(body);

  // Meta strip — mono chips: uuid, category, full timestamp.
  const meta = document.createElement('div');
  meta.className = 'detail-meta';
  const chips = [];
  chips.push(chip('uuid', shortUuid(memory.uuid)));
  if (memory.category) chips.push(chip('category', memory.category));
  chips.push(chip('created', fmt.timeFull(memory.created_at)));
  if (memory.parent_uuid) chips.push(chip('parent', shortUuid(memory.parent_uuid)));
  meta.innerHTML = chips.join('');
  body.appendChild(meta);

  // Body content — preformatted via .detail-body.
  const content = document.createElement('div');
  content.className = 'detail-body';
  if (memory.content && memory.content.trim()) {
    content.textContent = memory.content;
  } else {
    content.innerHTML = `<span style="color:var(--ink-3);font-style:italic;">(gist-only memory, no body)</span>`;
  }
  body.appendChild(content);

  // Thread section — only if there's more than just this memory.
  if (thread.length > 1) {
    const section = document.createElement('section');
    section.className = 'detail-thread';

    const label = document.createElement('div');
    label.className = 'detail-thread__label';
    label.textContent = `Thread (${thread.length} memories)`;
    section.appendChild(label);

    const list = document.createElement('div');
    list.className = 'memory-list';
    for (const m of thread) {
      const isCurrent = m.uuid === memory.uuid;
      const row = document.createElement('button');
      row.type = 'button';
      row.className = 'memory-row' + (isCurrent ? ' is-current' : '');
      if (isCurrent) {
        // No .is-current rule lives in styles.css yet; weight bump is the
        // signal. Quiet, in keeping with the register.
        row.style.fontWeight = '500';
        row.style.background = 'var(--bg-elevated)';
      }
      row.innerHTML = `
        <span class="memory-row__gist">${escapeHtml(m.gist || '(no gist)')}</span>
        ${m.category ? `<span class="memory-row__chip">${escapeHtml(m.category)}</span>` : '<span></span>'}
        <span class="memory-row__time">${escapeHtml(fmt.ago(m.created_at))}</span>
      `;
      if (!isCurrent) {
        row.addEventListener('click', () => navigate('detail', { selectedMemoryUuid: m.uuid }));
      } else {
        row.disabled = true;
        row.style.cursor = 'default';
      }
      list.appendChild(row);
    }
    section.appendChild(list);
    body.appendChild(section);
  }

  container.innerHTML = '';
  container.appendChild(root);
}

// ---- helpers ---------------------------------------------------------------

// "Remembered Tuesday at 3:42pm · 47 days ago" — plain prose, no chrome.
function subtitleLine(m) {
  const full = fmt.timeFull(m.created_at);
  const rel = fmt.ago(m.created_at);
  if (full === '—' && rel === '—') return 'Timestamp unavailable.';
  if (full === '—') return `Remembered ${rel}.`;
  return `Remembered ${full} · ${rel}`;
}

function chip(label, value) {
  return `<span class="detail-meta__chip">${escapeHtml(label)}: ${escapeHtml(value)}</span>`;
}

function shortUuid(uuid) {
  if (!uuid) return '—';
  return String(uuid).slice(0, 8);
}

function escapeHtml(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

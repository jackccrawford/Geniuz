// memoryModal.js — reading one memory (Jack, 2026-11-02: "user navigates tree,
// clicks on memory, it pops up a modal"). The stream or Find stays exactly
// where it was behind it; closing returns there. Read-only: moving through a
// thread is the tree's job. Its one way out is Reply, which opens Keep pointed
// at this memory (Jack, 2026-11-06).

import * as api from '../api.js';
import * as su from '../stream-ui.js';
import { getState } from '../store.js';
import { replyTo } from '../surfaces/remember.js';

let dialog = null;

function closeDialog() {
  if (dialog?.open) dialog.close();
}

function ensureDialog() {
  if (dialog) return dialog;
  dialog = document.createElement('dialog');
  dialog.className = 'memory-modal';
  dialog.setAttribute('aria-label', 'Memory');
  // A click on the backdrop lands on the dialog itself, outside its panel.
  dialog.addEventListener('click', (ev) => {
    if (ev.target === dialog) closeDialog();
    const link = ev.target.closest('.slink');
    if (link) {
      ev.preventDefault();
      api.openLink(link.dataset.href).catch((e) => console.error(e));
    }
  });
  document.body.appendChild(dialog);
  return dialog;
}

function panel(inner) {
  return `<div class="memory-modal__panel">
    <button type="button" class="memory-modal__close" aria-label="Close" title="Close (esc)">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round"><path d="M6 6l12 12"/><path d="M18 6L6 18"/></svg>
    </button>
    ${inner}
  </div>`;
}

export async function openMemory(uuid, { onClose, folder = null } = {}) {
  const d = ensureDialog();
  d.onclose = () => { if (onClose) onClose(); };
  const wire = () => d.querySelector('.memory-modal__close').addEventListener('click', closeDialog);
  d.innerHTML = panel('<p class="memory-modal__status">Loading…</p>');
  wire();
  if (!d.open) d.showModal();

  let m;
  try {
    m = await api.getMemoryDetail(uuid, folder);
  } catch (e) {
    d.innerHTML = panel(`<p class="memory-modal__status">Couldn't load this memory. ${su.esc(e?.message || e)}</p>`);
    wire();
    return;
  }
  if (!m) {
    d.innerHTML = panel(`<p class="memory-modal__status">No memory ${su.esc(String(uuid).slice(0, 8))} here.</p>`);
    wire();
    return;
  }

  const text = su.bodyAfterGist(m.content, m.gist);
  // The full address works from any folder, so that's what gets copied.
  const folderId = folder || (getState().folders?.folders || []).find((f) => f.current)?.uuid || '';
  const id = `${String(folderId).slice(0, 8)}:${String(m.uuid).slice(0, 8)}`.toUpperCase();
  d.innerHTML = panel(`
    <header class="memory-modal__head">
      ${su.node(!!m.parent_uuid && String(m.parent_uuid).toUpperCase() !== String(m.uuid).toUpperCase())}
      <div class="memory-modal__titles">
        <h2 class="memory-modal__gist">${su.esc(m.gist || '(no gist)')}</h2>
      </div>
    </header>
    <div class="memory-modal__text">${text ? su.renderText(text) : ''}</div>
    <footer class="memory-modal__foot">
      <span>${su.esc(su.timeFull(m.created_at))}</span>
      <span class="memory-modal__actions">
        <button type="button" class="memory-modal__reply" title="Keep a new memory in this thread">Reply</button>
        <button type="button" class="memory-modal__id" title="Copy this memory's address">${su.esc(id)}</button>
      </span>
    </footer>
  `);
  wire();
  d.querySelector('.memory-modal__reply').addEventListener('click', () => {
    closeDialog();
    replyTo({ uuid: m.uuid, folder: folderId, gist: m.gist });
  });
  const idBtn = d.querySelector('.memory-modal__id');
  idBtn.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(id);
      idBtn.textContent = 'Copied';
      setTimeout(() => { idBtn.textContent = id; }, 1200);
    } catch (e) {
      console.error(e);
    }
  });
  d.querySelector('.memory-modal__panel').scrollTop = 0;
}

// memoryModal.js — reading one memory (Jack, 2026-11-02: "user navigates tree,
// clicks on memory, it pops up a modal"). The stream or Find stays exactly
// where it was behind it; closing returns there. Read-only, and it goes nowhere
// else: moving through a thread is the tree's job.

import * as api from '../api.js';
import * as su from '../stream-ui.js';

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

export async function openMemory(uuid, { onClose } = {}) {
  const d = ensureDialog();
  d.onclose = () => { if (onClose) onClose(); };
  const wire = () => d.querySelector('.memory-modal__close').addEventListener('click', closeDialog);
  d.innerHTML = panel('<p class="memory-modal__status">Loading…</p>');
  wire();
  if (!d.open) d.showModal();

  let m;
  try {
    m = await api.getMemoryDetail(uuid);
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

  const { tag, rest } = su.splitGist(m.gist || '');
  const text = (m.content || '').trim();
  const id = String(m.uuid).slice(0, 8);
  d.innerHTML = panel(`
    <header class="memory-modal__head">
      ${su.mark(tag, 36)}
      <div class="memory-modal__titles">
        ${tag ? `<div class="memory-modal__tag">${su.esc(tag)}</div>` : ''}
        <h2 class="memory-modal__gist">${su.esc(tag ? rest : (m.gist || '(no gist)'))}</h2>
      </div>
    </header>
    <div class="memory-modal__text">${text ? su.renderText(text) : '<em>No more than its gist.</em>'}</div>
    <footer class="memory-modal__foot">
      <span>${su.esc(su.timeFull(m.created_at))}</span>
      <button type="button" class="memory-modal__id" title="Copy the full ID">${su.esc(id)}</button>
    </footer>
  `);
  wire();
  const idBtn = d.querySelector('.memory-modal__id');
  idBtn.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(m.uuid);
      idBtn.textContent = 'Copied';
      setTimeout(() => { idBtn.textContent = id; }, 1200);
    } catch (e) {
      console.error(e);
    }
  });
  d.querySelector('.memory-modal__panel').scrollTop = 0;
}

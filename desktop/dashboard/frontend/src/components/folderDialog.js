// folderDialog.js — making and editing a folder. The prompt comes from how
// people really described their folders: what's here, what must not go here,
// how to read it, when it ends.

import * as api from '../api.js';
import * as su from '../stream-ui.js';

const PROMPT = 'What should whoever comes here next know?';
const CLOSE = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>';

let dialog = null;

function ensureDialog() {
  if (dialog) return dialog;
  dialog = document.createElement('dialog');
  dialog.className = 'memory-modal folder-dialog';
  dialog.addEventListener('click', (ev) => {
    if (ev.target === dialog) dialog.close();
  });
  document.body.appendChild(dialog);
  return dialog;
}

async function pickDirectory(title) {
  const { open } = window.__TAURI__.dialog || {};
  if (!open) throw new Error('Choosing a directory is not available here.');
  return open({ directory: true, multiple: false, title });
}

/** With no folder, makes a new one. */
export function openFolderDialog(folder = null) {
  const d = ensureDialog();
  d.setAttribute('aria-label', folder ? `Edit ${folder.name}` : 'New folder');
  d.innerHTML = `<form class="memory-modal__panel folder-form" method="dialog">
    <button type="button" class="memory-modal__close" data-action="close" aria-label="Close" title="Close (esc)">${CLOSE}</button>
    <h2 class="folder-form__title">${folder ? 'Edit folder' : 'New folder'}</h2>
    <label class="remember-field">
      <span class="remember-field__label">Name</span>
      <input class="remember-field__input" name="name" maxlength="60" autocomplete="off" autocorrect="off" value="${su.esc(folder?.name || '')}">
    </label>
    <label class="remember-field">
      <span class="remember-field__label">Description</span>
      <textarea class="remember-field__textarea folder-form__description" name="description" autocorrect="off" rows="5" placeholder="${PROMPT}">${su.esc(folder?.description || '')}</textarea>
    </label>
    ${folder ? `<p class="folder-form__where">Location: <code>${su.esc(folder.location)}</code></p>` : ''}
    <div class="remember-status" aria-live="polite"></div>
    <div class="remember-actions folder-form__actions">
      ${folder
        ? (folder.fixed ? '' : `<button type="button" class="remember-actions__cancel" data-action="move">Move…</button>
           <button type="button" class="remember-actions__cancel" data-action="archive">Archive</button>`)
        : '<button type="button" class="remember-actions__cancel" data-action="add">Add existing…</button>'}
      <span class="folder-form__spacer"></span>
      <button type="button" class="remember-actions__cancel" data-action="close">Cancel</button>
      <button type="submit" class="remember-actions__submit">${folder ? 'Save' : 'Create'}</button>
    </div>
  </form>`;

  const form = d.querySelector('form');
  const status = form.querySelector('.remember-status');
  const fail = (e) => { status.textContent = e?.message || String(e); };
  const action = (name) => form.querySelector(`[data-action="${name}"]`);

  form.querySelectorAll('[data-action="close"]').forEach((b) => b.addEventListener('click', () => d.close()));

  form.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const name = form.elements.name.value;
    const description = form.elements.description.value;
    try {
      if (folder) await api.updateFolder(folder.uuid, name, description);
      else await api.createFolder(name, description);
      location.reload();
    } catch (e) { fail(e); }
  });

  action('add')?.addEventListener('click', async () => {
    try {
      const dir = await pickDirectory('Add a directory of Geniuz folders');
      if (!dir) return;
      await api.addFolders(dir);
      location.reload();
    } catch (e) { fail(e); }
  });

  action('move')?.addEventListener('click', async () => {
    try {
      const dir = await pickDirectory(`Move ${folder.name} to…`);
      if (!dir) return;
      const leftBehind = await api.moveFolder(folder.uuid, dir);
      if (!leftBehind) { location.reload(); return; }
      status.textContent = `Moved. The original is at ${leftBehind}. Geniuz won't read it again; delete it when you're satisfied.`;
      form.querySelectorAll('button, input, textarea').forEach((el) => { el.disabled = el.dataset.action === 'close' ? false : true; });
      d.addEventListener('close', () => location.reload(), { once: true });
    } catch (e) { fail(e); }
  });

  // Two steps, on the button itself: a native confirm would block the window.
  const archive = action('archive');
  archive?.addEventListener('click', async () => {
    if (!archive.dataset.armed) {
      archive.dataset.armed = '1';
      archive.textContent = 'Archive: hide it, keep it';
      return;
    }
    try {
      await api.archiveFolder(folder.uuid);
      location.reload();
    } catch (e) { fail(e); }
  });

  if (!d.open) d.showModal();
  form.elements.name.focus();
}

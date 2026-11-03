// remember.js — author a new memory.
//
// Two-field form: Gist (single line) + Content (multi-line). Matches the TUI's
// two-field compose. Gist is optional; if blank the chassis auto-derives one
// from the first 200 chars of content. Submit calls `remember_memory` which
// wraps `db::signal()` — same write path as the CLI and TUI.

import * as su from '../stream-ui.js';
import * as api from '../api.js';
import { getState, navigate, setState } from '../store.js';

// A draft outlives leaving this page, and the reload that switching folder does.
// Nobody should lose what they typed to a click in the sidebar.
const DRAFT_KEY = 'geniuz.keepDraft';
function readDraft() {
  try { return JSON.parse(sessionStorage.getItem(DRAFT_KEY)) || {}; } catch { return {}; }
}
function writeDraft(draft) {
  try { sessionStorage.setItem(DRAFT_KEY, JSON.stringify(draft)); } catch { /* still works, without surviving */ }
}
function clearDraft() {
  try { sessionStorage.removeItem(DRAFT_KEY); } catch { /* nothing to clear */ }
}

export async function mount(container) {
  const root = document.createElement('main');
  root.className = 'main';

  const folders = getState().folders?.folders || [];
  const shown = folders.find((f) => f.current);
  const header = document.createElement('header');
  header.className = 'main-header';
  header.innerHTML = `
    <h1 class="main-header__title">Keep</h1>
    <p class="main-header__sub">Gist is a short shelf-label for retrieval. Content is the body.</p>
  `;
  root.appendChild(header);

  const body = document.createElement('div');
  body.className = 'main-body';
  body.style.cssText = 'max-width:720px;';
  root.appendChild(body);

  const form = document.createElement('form');
  form.className = 'remember-form';
  const folderField = folders.length > 1
    ? `<label class="remember-field">
      <span class="remember-field__label">Folder</span>
      <select class="remember-field__input remember-field__select" name="folder">
        ${folders.map((f) => `<option value="${su.esc(f.uuid)}"${f.current ? ' selected' : ''}>${su.esc(f.name)}</option>`).join('')}
      </select>
    </label>`
    : '';
  form.innerHTML = `
    ${folderField}
    <label class="remember-field">
      <span class="remember-field__label">Gist <span class="remember-field__hint">· short shelf-label, e.g. "fix: auth token order"</span></span>
      <input
        type="text"
        class="remember-field__input"
        name="gist"
        autocomplete="off"
        placeholder="(optional, auto-derived from content if blank)"
      />
    </label>
    <label class="remember-field">
      <span class="remember-field__label">Content</span>
      <textarea
        class="remember-field__textarea"
        name="content"
        rows="10"
        placeholder="What you learned, decided, or discovered."
      ></textarea>
    </label>
    <div class="remember-actions">
      <button type="button" class="remember-actions__cancel" data-action="cancel">Cancel</button>
      <button type="submit" class="remember-actions__submit" disabled>Keep</button>
    </div>
    <div class="remember-status" aria-live="polite"></div>
  `;
  body.appendChild(form);

  container.innerHTML = '';
  container.appendChild(root);

  const gistInput = form.querySelector('input[name="gist"]');
  const contentInput = form.querySelector('textarea[name="content"]');
  const submitBtn = form.querySelector('button[type="submit"]');
  const cancelBtn = form.querySelector('button[data-action="cancel"]');
  const status = form.querySelector('.remember-status');

  // Focus the gist field on mount — matches TUI default
  setTimeout(() => gistInput.focus(), 0);

  const updateSubmitState = () => {
    submitBtn.disabled = !contentInput.value.trim();
  };
  contentInput.addEventListener('input', updateSubmitState);

  const folderSelect = form.querySelector('select[name="folder"]');
  const draft = readDraft();
  gistInput.value = draft.gist || '';
  contentInput.value = draft.content || '';
  // A choice made for this draft stands, if that folder is still there.
  if (folderSelect && folders.some((f) => f.uuid === draft.folder)) folderSelect.value = draft.folder;
  updateSubmitState();
  const saveDraft = () => writeDraft({ gist: gistInput.value, content: contentInput.value, folder: folderSelect?.value });
  gistInput.addEventListener('input', saveDraft);
  contentInput.addEventListener('input', saveDraft);
  folderSelect?.addEventListener('change', saveDraft);

  cancelBtn.addEventListener('click', () => {
    clearDraft();
    navigate('recent');
  });

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const gist = gistInput.value.trim();
    const content = contentInput.value.trim();
    if (!content) return;
    submitBtn.disabled = true;
    status.textContent = 'Saving…';
    try {
      const chosen = folderSelect?.value || shown?.uuid || null;
      const shortUuid = await api.rememberMemory(gist || null, content, chosen);
      clearDraft();
      if (chosen && chosen !== shown?.uuid) {
        // It went elsewhere, so the list here won't show it. Say so, and stay.
        const name = folders.find((f) => f.uuid === chosen)?.name || 'that folder';
        status.textContent = `Kept in ${name} · ${shortUuid}`;
        gistInput.value = '';
        contentInput.value = '';
        folderSelect.value = shown.uuid;
        updateSubmitState();
        gistInput.focus();
        return;
      }
      status.textContent = `Kept · ${shortUuid}`;
      // Navigate to Recent and select the new memory
      setTimeout(() => navigate('recent'), 300);
    } catch (err) {
      status.textContent = `Couldn't save: ${err?.message || err}`;
      submitBtn.disabled = false;
    }
  });

  // Cmd-Enter / Ctrl-Enter submits from either field
  const submitOnHotkey = (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      if (!submitBtn.disabled) form.requestSubmit();
    }
  };
  folderSelect?.addEventListener('keydown', submitOnHotkey);
  gistInput.addEventListener('keydown', submitOnHotkey);
  contentInput.addEventListener('keydown', submitOnHotkey);
}

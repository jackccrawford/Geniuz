// remember.js — author a new memory.
//
// Two-field form: Gist (single line) + Content (multi-line). Matches the TUI's
// two-field compose. Gist is optional; if blank the chassis auto-derives one
// from the first 200 chars of content. Submit calls `remember_memory` which
// wraps `db::signal()` — same write path as the CLI and TUI.

import * as api from '../api.js';
import { navigate, setState } from '../store.js';

export async function mount(container) {
  const root = document.createElement('main');
  root.className = 'main';

  const header = document.createElement('header');
  header.className = 'main-header';
  header.innerHTML = `
    <h1 class="main-header__title">Remember</h1>
    <p class="main-header__sub">Save what matters. Gist is a short shelf-label for retrieval. Content is the body.</p>
  `;
  root.appendChild(header);

  const body = document.createElement('div');
  body.className = 'main-body';
  body.style.cssText = 'max-width:720px;';
  root.appendChild(body);

  const form = document.createElement('form');
  form.className = 'remember-form';
  form.innerHTML = `
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
      <button type="submit" class="remember-actions__submit" disabled>Remember</button>
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

  cancelBtn.addEventListener('click', () => {
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
      const shortUuid = await api.rememberMemory(gist || null, content);
      status.textContent = `Remembered · ${shortUuid}`;
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
  gistInput.addEventListener('keydown', submitOnHotkey);
  contentInput.addEventListener('keydown', submitOnHotkey);
}

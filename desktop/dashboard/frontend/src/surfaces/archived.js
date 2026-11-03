// archived.js — what has been archived, and the way back. A page rather than a
// dialog, because it's a place you go to, like Status or Settings (Jack, 2026-11-02).

import * as api from '../api.js';
import * as su from '../stream-ui.js';
import { getState } from '../store.js';

export async function mount(container) {
  const folders = getState().folders?.archived || [];

  const root = document.createElement('main');
  root.className = 'main';
  root.innerHTML = `
    <header class="main-header">
      <h1 class="main-header__title">Archived</h1>
      <p class="main-header__sub">Hidden, not deleted. Restoring puts a folder back exactly as it was.</p>
    </header>
    <div class="main-body">
      <div class="archived-list"></div>
      <div class="remember-status archived-status" aria-live="polite"></div>
    </div>
  `;

  const list = root.querySelector('.archived-list');
  const status = root.querySelector('.archived-status');
  for (const f of folders) {
    const row = document.createElement('div');
    row.className = 'archived-row';
    const count = `${f.memories} ${f.memories === 1 ? 'memory' : 'memories'}`;
    const summary = f.description ? f.description.split('\n')[0] : '';
    row.innerHTML = `<div class="archived-row__text">
        <div class="archived-row__name">${su.esc(f.name)}</div>
        <div class="archived-row__meta">${count}${summary ? ` · ${su.esc(summary)}` : ''}</div>
      </div>
      <button type="button" class="remember-actions__cancel">Restore</button>`;
    row.querySelector('button').addEventListener('click', async () => {
      try {
        await api.restoreFolder(f.uuid);
        location.reload();
      } catch (e) {
        status.textContent = e?.message || String(e);
      }
    });
    list.appendChild(row);
  }

  container.innerHTML = '';
  container.appendChild(root);
}

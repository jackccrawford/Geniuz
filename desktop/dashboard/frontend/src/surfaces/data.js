// data.js — Data & Export surface in Tool register.
//
// The statement-of-sovereignty page: where a folder lives and how to take a copy
// of it. A Folder pulldown (like Keep's) chooses which folder; it starts on the
// one the window shows. Two .data-cards, max-width 720px (the class supplies the cap).
// No destructive actions. No cloud. Just observation + safe export.

import * as api from '../api.js';
import * as fmt from '../format.js';

export async function mount(container) {
  container.innerHTML = `<div class="surface-loading">Loading your data…</div>`;

  let list;
  try {
    list = await api.listFolders();
  } catch (e) {
    container.innerHTML = `
      <main class="main">
        <div class="surface-error">
          <h2>Couldn't read your folders.</h2>
          <p>${escapeHtml(e?.message || String(e))}</p>
        </div>
      </main>
    `;
    return;
  }
  const folders = list?.folders || [];
  let chosen = folders.find((f) => f.current) || folders[0] || null;

  const root = document.createElement('main');
  root.className = 'main';

  // Header
  const header = document.createElement('header');
  header.className = 'main-header';
  header.innerHTML = `
    <h1 class="main-header__title">Data &amp; Export</h1>
    <p class="main-header__sub">Where your memory lives, and how to take it with you.</p>
  `;
  root.appendChild(header);

  const body = document.createElement('div');
  body.className = 'main-body';
  root.appendChild(body);

  // ---- Folder: which one this page is about (only when there's a choice) ----
  let folderSelect = null;
  if (folders.length > 1) {
    const pick = document.createElement('label');
    pick.className = 'remember-field data-folder';
    pick.style.cssText = 'max-width:720px;';
    pick.innerHTML = `
      <span class="remember-field__label">Folder</span>
      <select class="remember-field__input remember-field__select" name="folder">
        ${folders.map((f) => `<option value="${escapeHtml(f.uuid)}"${f === chosen ? ' selected' : ''}>${escapeHtml(f.name)}</option>`).join('')}
      </select>
    `;
    body.appendChild(pick);
    folderSelect = pick.querySelector('select');
  }

  // ---- Card 1: Where this folder lives --------------------------------------
  const locCard = document.createElement('section');
  locCard.className = 'data-card';
  locCard.innerHTML = `
    <h2 class="data-card__title">Where your memory lives</h2>
    <div class="data-card__body">
      <p>Your memory is on this computer. No cloud. No account. No telemetry.</p>
      <p><span class="data-card__path data-loc-path"></span></p>
      <p class="data-loc-stats" style="color:var(--ink-3);font-size:var(--fs-micro);"></p>
    </div>
    <div class="data-card__actions">
      <button type="button" class="btn" data-action="reveal">${document.documentElement.classList.contains('os-mac') ? 'Reveal in Finder' : 'Open folder'}</button>
      <button type="button" class="btn btn--ghost" data-action="copy">Copy path</button>
      <span class="data-card__feedback" style="font-size:var(--fs-micro);color:var(--ink-3);align-self:center;"></span>
    </div>
  `;
  body.appendChild(locCard);

  const locPath = locCard.querySelector('.data-loc-path');
  const locStats = locCard.querySelector('.data-loc-stats');
  const revealBtn = locCard.querySelector('[data-action="reveal"]');
  const copyBtn = locCard.querySelector('[data-action="copy"]');
  const locFeedback = locCard.querySelector('.data-card__feedback');

  revealBtn.addEventListener('click', async () => {
    if (!chosen) return;
    try {
      await api.openPath(chosen.location);
    } catch (e) {
      flashFeedback(locFeedback, `Couldn't open: ${e?.message || String(e)}`, true);
    }
  });

  copyBtn.addEventListener('click', async () => {
    if (!chosen) return;
    try {
      await navigator.clipboard.writeText(chosen.location);
      flashFeedback(locFeedback, 'Copied.');
    } catch (e) {
      flashFeedback(locFeedback, `Couldn't copy: ${e?.message || String(e)}`, true);
    }
  });

  // ---- Card 2: Export ------------------------------------------------------
  const exportCard = document.createElement('section');
  exportCard.className = 'data-card';
  exportCard.innerHTML = `
    <h2 class="data-card__title">Export</h2>
    <div class="data-card__body">
      <p>Export a copy of <span class="data-export-name"></span> as one file. Plain SQLite,
      portable, yours. Open it with any SQLite tool, back it up, or move it to another machine.</p>
    </div>
    <div class="data-card__actions">
      <button type="button" class="btn btn--primary" data-action="export"></button>
    </div>
    <div class="data-card__export-result" style="margin-top:var(--s-3);font-size:var(--fs-micro);"></div>
  `;
  body.appendChild(exportCard);

  const exportName = exportCard.querySelector('.data-export-name');
  const exportBtn = exportCard.querySelector('[data-action="export"]');
  const exportResult = exportCard.querySelector('.data-card__export-result');

  // Both cards describe the folder chosen above. The size is asked for separately;
  // a folder that changes while it's being asked for keeps only the newest answer.
  let asked = 0;
  const show = async () => {
    exportResult.textContent = '';
    if (!chosen) {
      locPath.textContent = '(no folder)';
      locStats.textContent = '';
      exportName.textContent = 'a folder';
      exportBtn.textContent = 'Export…';
      exportBtn.disabled = true;
      return;
    }
    locPath.textContent = chosen.location;
    exportName.innerHTML = `<strong>${escapeHtml(chosen.name)}</strong>`;
    exportBtn.textContent = `Export ${chosen.name}…`;
    exportBtn.disabled = false;
    const parts = (bytes) => [
      `${chosen.name}`,
      `${fmt.number(chosen.memories)} ${chosen.memories === 1 ? 'memory' : 'memories'}`,
      bytes == null ? null : fmt.bytes(bytes).join(' ') + ' on disk',
      chosen.last_written ? `last write ${fmt.ago(chosen.last_written)}` : null,
    ].filter(Boolean).join(' · ');
    locStats.textContent = parts(null);
    const mine = ++asked;
    try {
      const bytes = await api.folderDiskBytes(chosen.uuid);
      if (mine === asked) locStats.textContent = parts(bytes);
    } catch { /* the count and date are shown already */ }
  };

  folderSelect?.addEventListener('change', () => {
    chosen = folders.find((f) => f.uuid === folderSelect.value) || chosen;
    show();
  });

  exportBtn.addEventListener('click', async () => {
    if (!chosen) return;
    const folder = chosen;
    const suggested = defaultExportPath(folder);

    // Native save dialog via tauri-plugin-dialog. window.prompt() is
    // unavailable in WKWebView, so we ask the OS for a Save As panel.
    let targetPath = null;
    try {
      const { save } = window.__TAURI__.dialog || {};
      if (typeof save === 'function') {
        targetPath = await save({
          defaultPath: suggested,
          title: `Export ${folder.name}`,
          filters: [{ name: 'SQLite database', extensions: ['db'] }],
        });
      }
    } catch (e) {
      exportResult.style.cssText = `margin-top:var(--s-3);font-size:var(--fs-micro);color:var(--status-bad);`;
      exportResult.textContent = `Couldn't open save panel: ${e?.message || String(e)}`;
      return;
    }
    if (!targetPath) {
      // User cancelled or dialog unavailable.
      exportResult.textContent = '';
      exportResult.style.cssText = 'margin-top:var(--s-3);font-size:var(--fs-micro);';
      return;
    }

    exportBtn.disabled = true;
    exportResult.style.cssText = 'margin-top:var(--s-3);font-size:var(--fs-micro);color:var(--ink-3);';
    exportResult.textContent = `Exporting ${folder.name}…`;

    try {
      const bytes = await api.exportMemoryDbTo(targetPath, folder.uuid);
      const [num, unit] = fmt.bytes(bytes);
      exportResult.style.cssText = 'margin-top:var(--s-3);font-size:var(--fs-micro);color:var(--ink-2);';
      exportResult.innerHTML = `Exported ${escapeHtml(folder.name)}, ${escapeHtml(num)} ${escapeHtml(unit)} (${escapeHtml(String(bytes))} bytes), to <span class="data-card__path">${escapeHtml(targetPath)}</span>`;
    } catch (e) {
      exportResult.style.cssText = `margin-top:var(--s-3);font-size:var(--fs-micro);color:var(--status-bad);`;
      exportResult.textContent = `Export failed: ${e?.message || String(e)}`;
    } finally {
      exportBtn.disabled = false;
    }
  });

  container.innerHTML = '';
  container.appendChild(root);
  show();
}

// ---- helpers --------------------------------------------------------------

function flashFeedback(el, msg, isError = false) {
  if (!el) return;
  el.textContent = msg;
  el.style.color = isError ? 'var(--status-bad)' : 'var(--ink-2)';
  clearTimeout(el.__t);
  el.__t = setTimeout(() => {
    el.textContent = '';
    el.style.color = 'var(--ink-3)';
  }, 2400);
}

// Suggest ~/Desktop/geniuz-<NAME>-<ID>-<stamp>.db, so the file says which folder
// is inside. The home directory is taken from the folder's own location when it
// sits under /Users/<name> or /home/<name> (or C:\Users\<name>).
function defaultExportPath(folder) {
  const loc = folder.location || '';
  const sep = loc.includes('\\') && !loc.includes('/') ? '\\' : '/';
  const home = (loc.match(/^(.*?[\\/](?:Users|home)[\\/][^\\/]+)/) || [])[1];

  const now = new Date();
  const pad = (n) => String(n).padStart(2, '0');
  const stamp =
    `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}` +
    `-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
  // Letters, digits, dash and underscore only: a name can hold anything.
  const safe = (folder.name || 'folder').replace(/[^A-Za-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '') || 'folder';
  const file = `geniuz-${safe}-${folder.id}-${stamp}.db`;

  return home ? `${home}${sep}Desktop${sep}${file}` : file;
}

function escapeHtml(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

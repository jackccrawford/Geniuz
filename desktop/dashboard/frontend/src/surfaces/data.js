// data.js — Data & Export surface in Tool register.
//
// The statement-of-sovereignty page: where your memory lives and how to take
// a copy of it. Two .data-cards, max-width 720px (the class supplies the cap).
// No destructive actions. No cloud. Just observation + safe export.

import * as api from '../api.js';
import * as fmt from '../format.js';

export async function mount(container) {
  container.innerHTML = `<div class="surface-loading">Loading your data…</div>`;

  let dataDir, stats;
  try {
    [dataDir, stats] = await Promise.all([
      api.getDataDir(),
      api.getStationStats(),
    ]);
  } catch (e) {
    container.innerHTML = `
      <main class="main">
        <div class="surface-error">
          <h2>Couldn't read your data location.</h2>
          <p>${escapeHtml(e?.message || String(e))}</p>
          <p>Expected file: <code>~/.geniuz/memory.db</code></p>
        </div>
      </main>
    `;
    return;
  }

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

  // ---- Card 1: Where your memory lives -------------------------------------
  const [storageNum, storageUnit] = fmt.bytes(stats?.storage_bytes);
  const memCount = fmt.number(stats?.total_memories);
  const threadCount = fmt.number(stats?.conversations);
  const lastWrite = stats?.last_write_iso ? fmt.ago(stats.last_write_iso) : null;

  const inlineStats = [
    `${memCount} memories`,
    `${threadCount} conversations`,
    `${storageNum} ${storageUnit} on disk`,
    lastWrite ? `last write ${lastWrite}` : null,
  ].filter(Boolean).join(' · ');

  const locCard = document.createElement('section');
  locCard.className = 'data-card';
  locCard.innerHTML = `
    <h2 class="data-card__title">Where your memory lives</h2>
    <div class="data-card__body">
      <p>Your memory is on this Mac. No cloud. No account. No telemetry.
      Geniuz never sends it anywhere.</p>
      <p><span class="data-card__path">${escapeHtml(dataDir || '(unknown)')}</span></p>
      <p style="color:var(--ink-3);font-size:var(--fs-micro);">${escapeHtml(inlineStats)}</p>
    </div>
    <div class="data-card__actions">
      <button type="button" class="btn" data-action="reveal">Reveal in Finder</button>
      <button type="button" class="btn btn--ghost" data-action="copy">Copy path</button>
      <span class="data-card__feedback" style="font-size:var(--fs-micro);color:var(--ink-3);align-self:center;"></span>
    </div>
  `;
  body.appendChild(locCard);

  const revealBtn = locCard.querySelector('[data-action="reveal"]');
  const copyBtn = locCard.querySelector('[data-action="copy"]');
  const locFeedback = locCard.querySelector('.data-card__feedback');

  revealBtn.addEventListener('click', async () => {
    if (!dataDir) return;
    try {
      await api.openPath(dataDir);
    } catch (e) {
      flashFeedback(locFeedback, `Couldn't open: ${e?.message || String(e)}`, true);
    }
  });

  copyBtn.addEventListener('click', async () => {
    if (!dataDir) return;
    try {
      await navigator.clipboard.writeText(dataDir);
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
      <p>Export a copy of <span class="data-card__path">memory.db</span>. Plain SQLite,
      portable, yours. Open it with any SQLite tool, back it up, or move it to another machine.</p>
    </div>
    <div class="data-card__actions">
      <button type="button" class="btn btn--primary" data-action="export" ${dataDir ? '' : 'disabled'}>Export memory.db…</button>
    </div>
    <div class="data-card__export-result" style="margin-top:var(--s-3);font-size:var(--fs-micro);"></div>
  `;
  body.appendChild(exportCard);

  const exportBtn = exportCard.querySelector('[data-action="export"]');
  const exportResult = exportCard.querySelector('.data-card__export-result');

  exportBtn.addEventListener('click', async () => {
    if (!dataDir) return;
    const suggested = defaultExportPath(dataDir);

    // Native save dialog via tauri-plugin-dialog. window.prompt() is
    // unavailable in WKWebView, so we ask the OS for a Save As panel.
    let targetPath = null;
    try {
      const { save } = window.__TAURI__.dialog || {};
      if (typeof save === 'function') {
        targetPath = await save({
          defaultPath: suggested,
          title: 'Export memory.db',
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
      exportResult.removeAttribute('style');
      exportResult.style.cssText = 'margin-top:var(--s-3);font-size:var(--fs-micro);';
      return;
    }

    exportBtn.disabled = true;
    exportResult.style.cssText = 'margin-top:var(--s-3);font-size:var(--fs-micro);color:var(--ink-3);';
    exportResult.textContent = 'Exporting…';

    try {
      const bytes = await api.exportMemoryDbTo(targetPath);
      const [num, unit] = fmt.bytes(bytes);
      exportResult.style.cssText = 'margin-top:var(--s-3);font-size:var(--fs-micro);color:var(--ink-2);';
      exportResult.innerHTML = `Exported ${escapeHtml(num)} ${escapeHtml(unit)} (${escapeHtml(String(bytes))} bytes) to <span class="data-card__path">${escapeHtml(targetPath)}</span>`;
    } catch (e) {
      exportResult.style.cssText = `margin-top:var(--s-3);font-size:var(--fs-micro);color:var(--status-bad);`;
      exportResult.textContent = `Export failed: ${e?.message || String(e)}`;
    } finally {
      exportBtn.disabled = false;
    }
  });

  container.innerHTML = '';
  container.appendChild(root);
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

// Suggest ~/Desktop/geniuz-export-<stamp>.db given the dataDir's parent.
function defaultExportPath(dataDir) {
  const sep = dataDir.includes('\\') && !dataDir.includes('/') ? '\\' : '/';
  const trimmed = dataDir.replace(/[\/\\]+$/, '');
  const lastSep = Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\'));
  const homeDir = lastSep > 0 ? trimmed.slice(0, lastSep) : trimmed;

  const now = new Date();
  const pad = (n) => String(n).padStart(2, '0');
  const stamp =
    `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}` +
    `-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;

  return `${homeDir}${sep}Desktop${sep}geniuz-export-${stamp}.db`;
}

function escapeHtml(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

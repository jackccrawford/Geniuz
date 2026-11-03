// status.js — Status surface in Tool register.
//
// Linear-style status page: three sections — Memory, Storage, Connections.
// Each section uses .status-section / .status-row primitives from styles.css.
// Dots signal health (ok/warn/bad). No actions live here; this is observation.

import * as api from '../api.js';
import * as fmt from '../format.js';

export async function mount(container) {
  container.innerHTML = `<div class="surface-loading">Reading status…</div>`;

  let status;
  try {
    status = await api.getStatus();
  } catch (e) {
    container.innerHTML = `
      <main class="main">
        <header class="main-header">
          <h1 class="main-header__title">Status</h1>
          <p class="main-header__sub">Where your memory lives and what it's connected to.</p>
        </header>
        <div class="main-body">
          <div class="surface-error">
            <h2>Couldn't read status.</h2>
            <p>${escapeHtml(e?.message || String(e))}</p>
            <p>Expected file: <code>~/.geniuz/memory.db</code></p>
          </div>
        </div>
      </main>
    `;
    return;
  }

  const root = document.createElement('main');
  root.className = 'main';

  // ---- Header --------------------------------------------------------------
  const header = document.createElement('header');
  header.className = 'main-header';
  header.innerHTML = `
    <h1 class="main-header__title">Status</h1>
    <p class="main-header__sub">Where your memory lives and what it's connected to.</p>
  `;
  root.appendChild(header);

  const body = document.createElement('div');
  body.className = 'main-body';
  root.appendChild(body);

  // ---- Compute health flags ------------------------------------------------
  const memCount = status?.memory_count ?? null;
  const embCount = status?.embedding_count ?? null;
  const indexedPct = status?.indexed_pct ?? null;
  const embedModel = status?.embedding_model ?? null;
  const dataDir = status?.data_dir ?? null;
  const dataBytes = status?.data_dir_bytes ?? null;
  const cdConfigured = status?.claude_desktop_configured;
  const cdHasGeniuz = status?.claude_desktop_has_geniuz;
  const cdPath = status?.claude_desktop_config_path ?? null;

  // Indexed health: <50% bad, <95% warn, else ok. No memories → ok (nothing to index).
  let indexedDot = 'ok';
  if (memCount && memCount > 0 && indexedPct !== null && indexedPct !== undefined) {
    if (indexedPct < 50) indexedDot = 'bad';
    else if (indexedPct < 95) indexedDot = 'warn';
  }
  const memoriesDot = memCount === null || memCount === undefined ? 'warn' : 'ok';
  const modelDot = embedModel ? 'ok' : 'warn';

  // Connections health
  const cdConfiguredDot = cdConfigured ? 'ok' : 'warn';
  const cdRegisteredDot = cdHasGeniuz ? 'ok' : 'warn';

  // ---- Memory section ------------------------------------------------------
  const embeddedValue =
    embCount === null || embCount === undefined || memCount === null || memCount === undefined
      ? '—'
      : `${fmt.number(embCount)} / ${fmt.number(memCount)} (${
          indexedPct === null || indexedPct === undefined ? '—' : Math.round(indexedPct) + '%'
        })`;

  body.appendChild(section('Memory', [
    row('Memories', fmt.number(memCount), memoriesDot),
    row('Embedded', embeddedValue, indexedDot),
    row('Embedding model', embedModel ? escapeHtml(embedModel) : '—', modelDot),
  ]));

  // ---- Storage section -----------------------------------------------------
  const [sizeNum, sizeUnit] = fmt.bytes(dataBytes);
  const sizeValue = dataBytes === null || dataBytes === undefined ? '—' : `${sizeNum} ${sizeUnit}`;

  body.appendChild(section('Storage', [
    row('Data folder', dataDir ? escapeHtml(dataDir) : '—', 'ok'),
    row('Size on disk', sizeValue, 'ok'),
  ]));

  // ---- Connections section -------------------------------------------------
  const configuredValue =
    cdConfigured === null || cdConfigured === undefined
      ? '—'
      : cdConfigured ? 'Configured' : 'Not configured';
  const registeredValue =
    cdHasGeniuz === null || cdHasGeniuz === undefined
      ? '—'
      : cdHasGeniuz ? 'yes' : 'no';
  const pathValue = cdPath
    ? `<code style="font-family:var(--font-mono);font-size:var(--fs-micro);color:var(--ink-2);">${escapeHtml(cdPath)}</code>`
    : '—';

  body.appendChild(section('Connections', [
    row('Claude Desktop', configuredValue, cdConfiguredDot),
    row('Geniuz registered', registeredValue, cdRegisteredDot),
    row('Config path', pathValue, 'ok'),
  ]));

  container.innerHTML = '';
  container.appendChild(root);
}

// ---- helpers ---------------------------------------------------------------

function section(label, rowsHtml) {
  const sec = document.createElement('section');
  sec.className = 'status-section';
  sec.innerHTML = `
    <div class="status-section__label">${escapeHtml(label)}</div>
    ${rowsHtml.join('')}
  `;
  return sec;
}

function row(label, valueHtml, dotState) {
  const dotClass =
    dotState === 'warn' ? 'status-row__dot is-warn'
    : dotState === 'bad' ? 'status-row__dot is-bad'
    : 'status-row__dot';
  return `
    <div class="status-row">
      <span class="status-row__label">${escapeHtml(label)}</span>
      <span class="status-row__value">${valueHtml}</span>
      <span class="${dotClass}"></span>
    </div>
  `;
}

function escapeHtml(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

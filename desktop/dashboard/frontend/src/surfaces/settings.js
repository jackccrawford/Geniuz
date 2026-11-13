// settings.js — Settings surface in Tool register.
//
// macOS-style two-column form, centered max-width 720px. Four sections:
// Startup, Updates, Display, About. Local edited state diffs against the
// persisted saved state; when they differ, an action bar appears at the
// bottom of the form. No themes — the dashboard only has one register.

import * as api from '../api.js';
import { openShortcuts } from '../components/shortcutsDialog.js';

export async function mount(container) {
  container.innerHTML = `<div class="surface-loading">Loading settings…</div>`;

  let saved, appVersion, dataDir;
  try {
    [saved, appVersion, dataDir] = await Promise.all([
      api.getSettings(),
      api.getAppVersion(),
      api.getDataDir(),
    ]);
  } catch (e) {
    container.innerHTML = `
      <main class="main">
        <div class="surface-error">
          <h2>Couldn't load settings.</h2>
          <p>${escapeHtml(e?.message || String(e))}</p>
        </div>
      </main>
    `;
    return;
  }

  // ---- local state ---------------------------------------------------------
  let edited = { ...saved };
  let savedFlashTimer = null;

  function dirty() {
    return JSON.stringify(edited) !== JSON.stringify(saved);
  }

  function update(patch) {
    edited = { ...edited, ...patch };
    render();
  }

  async function onSave() {
    saveBtn.disabled = true;
    discardBtn.disabled = true;
    try {
      const persisted = await api.updateSettings(edited);
      saved = persisted;
      edited = { ...persisted };
      flashSaved();
      render();
    } catch (e) {
      flashError(e?.message || String(e));
    } finally {
      saveBtn.disabled = false;
      discardBtn.disabled = false;
    }
  }

  function onDiscard() {
    edited = { ...saved };
    render();
  }

  // ---- DOM scaffold --------------------------------------------------------
  const root = document.createElement('main');
  root.className = 'main';

  const header = document.createElement('header');
  header.className = 'main-header';
  header.innerHTML = `
    <h1 class="main-header__title">Settings</h1>
    <p class="main-header__sub">Your preferences. Stored locally.</p>
  `;
  root.appendChild(header);

  const body = document.createElement('div');
  body.className = 'main-body';
  root.appendChild(body);

  const shell = document.createElement('div');
  shell.className = 'settings-shell';
  body.appendChild(shell);

  // Action bar lives below the shell; render() toggles its visibility.
  const actionBar = document.createElement('div');
  actionBar.className = 'data-card__actions';
  actionBar.style.cssText = 'max-width:720px;margin:0 auto;justify-content:flex-end;align-items:center;';

  const flashEl = document.createElement('span');
  flashEl.style.cssText = 'font-size:var(--fs-micro);color:var(--ink-3);margin-right:auto;';
  flashEl.textContent = '';

  const discardBtn = document.createElement('button');
  discardBtn.type = 'button';
  discardBtn.className = 'btn btn--ghost';
  discardBtn.textContent = 'Discard';
  discardBtn.addEventListener('click', onDiscard);

  const saveBtn = document.createElement('button');
  saveBtn.type = 'button';
  saveBtn.className = 'btn btn--primary';
  saveBtn.textContent = 'Save changes';
  saveBtn.addEventListener('click', onSave);

  actionBar.append(flashEl, discardBtn, saveBtn);
  body.appendChild(actionBar);

  function flashSaved() {
    flashEl.textContent = 'Saved';
    flashEl.style.color = 'var(--ink-2)';
    flashEl.style.cursor = ''; flashEl.title = ''; flashEl.onclick = null;
    if (savedFlashTimer) clearTimeout(savedFlashTimer);
    savedFlashTimer = setTimeout(() => { flashEl.textContent = ''; }, 2400);
  }

  function flashError(msg) {
    flashEl.textContent = `Couldn't save: ${msg}`;
    flashEl.style.color = 'var(--status-bad)';
    if (savedFlashTimer) clearTimeout(savedFlashTimer);
    // Stays until the next save, and a click copies it (as on Data & export).
    const text = flashEl.textContent;
    flashEl.style.cursor = 'pointer';
    flashEl.title = 'Click to copy';
    flashEl.onclick = async () => {
      try { await navigator.clipboard.writeText(text); flashEl.textContent = `${text} (copied)`; } catch { /* stays readable */ }
    };
  }

  // ---- render --------------------------------------------------------------
  function render() {
    shell.innerHTML = '';

    // Startup
    shell.appendChild(section('Startup', [
      row({
        title: 'Launch Geniuz at login',
        help: 'Auto-start the menubar app when you sign in.',
        control: toggle({
          checked: !!edited.launch_at_login,
          onChange: (v) => update({ launch_at_login: v }),
        }),
      }),
    ]));

    // Display
    const countInput = document.createElement('input');
    countInput.type = 'number';
    countInput.className = 'input';
    countInput.min = '0';
    countInput.max = '20';
    countInput.style.width = '80px';
    countInput.value = String(edited.recent_memories_count ?? 5);
    countInput.addEventListener('change', () => {
      let n = parseInt(countInput.value, 10);
      if (Number.isNaN(n)) n = 0;
      n = Math.max(0, Math.min(20, n));
      countInput.value = String(n);
      update({ recent_memories_count: n });
    });

    // Only the macOS menubar app (desktop/Geniuz, Swift) reads
    // recent_memories_count. The Windows/Linux tray is a plain menu with no
    // memory list, so there the setting changed nothing — show it only
    // where it does something.
    if (/Macintosh|Mac OS X/.test(navigator.userAgent)) {
      shell.appendChild(section('Display', [
        row({
          title: 'Recent memories in the menu bar',
          help: 'How many memories the menu bar dropdown shows. 0 hides the section.',
          control: countInput,
        }),
      ]));
    }

    // Writing
    shell.appendChild(section('Writing', [
      row({
        title: 'Autocorrect as you type',
        help: 'Off, a memory is kept exactly as you typed it; the AI that reads it understands a typo. On, your computer may change words as you write, and on a Mac a pending correction takes the first click on Keep.',
        control: toggle({
          checked: !!edited.autocorrect,
          onChange: (v) => update({ autocorrect: v }),
        }),
      }),
    ]));

    // Updates
    const often = document.createElement('select');
    often.className = 'input';
    for (const [value, label] of [['daily', 'Daily'], ['weekly', 'Weekly'], ['manual', 'Only when I ask']]) {
      const o = document.createElement('option');
      o.value = value;
      o.textContent = label;
      often.appendChild(o);
    }
    often.value = edited.update_check_frequency || 'daily';
    often.disabled = !edited.autoupdate_enabled;
    often.addEventListener('change', () => update({ update_check_frequency: often.value }));

    const checkWrap = document.createElement('div');
    checkWrap.className = 'update-check';
    const checkBtn = document.createElement('button');
    checkBtn.type = 'button';
    checkBtn.className = 'btn';
    checkBtn.textContent = 'Check now';
    const checkSaid = document.createElement('span');
    checkSaid.className = 'update-check__said';
    checkSaid.setAttribute('role', 'status');
    checkSaid.setAttribute('aria-live', 'polite');
    checkBtn.addEventListener('click', async () => {
      checkBtn.disabled = true;
      checkBtn.textContent = 'Checking…';
      try {
        const found = await api.checkUpdateNow();
        // The same words the Mac's About window uses, so one check never reads two
        // ways. Neither sentence ends on the version: "5.0.11." reads as a number
        // cut short (Jack, 10 Nov 2026).
        checkSaid.textContent = found?.newer
          ? `Geniuz ${found.newer.version} is available. Status has the download.`
          : `Geniuz ${appVersion || ''} is up to date.`.replace('  ', ' ');
      } catch (e) {
        checkSaid.textContent = `Couldn't check: ${e?.message || e}`;
      }
      checkBtn.textContent = 'Check now';
      checkBtn.disabled = false;
    });
    checkWrap.appendChild(checkSaid);
    checkWrap.appendChild(checkBtn);

    shell.appendChild(section('Updates', [
      row({
        title: 'Check for updates automatically',
        help: 'A check asks GitHub for the latest Geniuz release. GitHub sees this computer\'s internet address and the Geniuz version; none of your memories or folders are sent.',
        control: toggle({
          checked: !!edited.autoupdate_enabled,
          onChange: (v) => update({ autoupdate_enabled: v }),
        }),
      }),
      row({ title: 'How often', control: often }),
      row({ title: 'Check for a new version', help: 'Asks once, now, whatever is set above.', control: checkWrap }),
    ]));

    // Keyboard shortcuts: one row. The list itself is help that can be changed,
    // and lives in its own panel (also under Help in the menu).
    const showKeys = document.createElement('button');
    showKeys.type = 'button';
    showKeys.className = 'btn';
    showKeys.textContent = 'Show shortcuts';
    showKeys.addEventListener('click', () => openShortcuts());
    shell.appendChild(section('Keyboard', [
      row({
        title: 'Keyboard shortcuts',
        help: 'See them all, or change one. Also under Help in the menu.',
        control: showKeys,
      }),
    ]));

    // About
    const aboutSection = document.createElement('section');
    aboutSection.className = 'settings-section';
    const aboutLabel = document.createElement('div');
    aboutLabel.className = 'settings-section__label';
    aboutLabel.textContent = 'About';
    aboutSection.appendChild(aboutLabel);

    const kv = document.createElement('div');
    kv.className = 'kv-list';
    kv.innerHTML = `
      <div class="kv-list__row">
        <div class="kv-list__key">Settings version</div>
        <div class="kv-list__val">${escapeHtml(String(saved.version ?? '—'))}</div>
      </div>
      <div class="kv-list__row">
        <div class="kv-list__key">App version</div>
        <div class="kv-list__val">${escapeHtml(appVersion || '—')}</div>
      </div>
      <div class="kv-list__row">
        <div class="kv-list__key">Data directory</div>
        <div class="kv-list__val">${escapeHtml(dataDir || '—')}</div>
      </div>
    `;
    aboutSection.appendChild(kv);
    shell.appendChild(aboutSection);

    actionBar.style.display = dirty() ? 'flex' : 'none';
  }

  render();
  container.innerHTML = '';
  container.appendChild(root);
}

// ---- inline helpers --------------------------------------------------------

let rowIds = 0;

function section(label, rows) {
  const sec = document.createElement('section');
  sec.className = 'settings-section';
  const lbl = document.createElement('div');
  lbl.className = 'settings-section__label';
  lbl.textContent = label;
  sec.appendChild(lbl);
  for (const r of rows) sec.appendChild(r);
  return sec;
}

function row({ title, help, control }) {
  const r = document.createElement('div');
  r.className = 'settings-row';

  const main = document.createElement('div');
  main.className = 'settings-row__main';
  const t = document.createElement('div');
  t.className = 'settings-row__title';
  t.textContent = title;
  // The control is named by this title for anyone using a screen reader: a
  // switch or a menu beside a line of text is otherwise an unnamed control.
  t.id = `settings-row-${(rowIds += 1)}`;
  main.appendChild(t);
  if (help) {
    const h = document.createElement('div');
    h.className = 'settings-row__help';
    h.textContent = help;
    main.appendChild(h);
  }
  r.appendChild(main);

  const ctl = document.createElement('div');
  ctl.className = 'settings-row__control';
  // Switches, menus and number boxes say nothing of themselves; a button with
  // words on it ("Check now") already has its name.
  const unnamed = '[role="switch"], select, input';
  const named = control.matches?.(unnamed) ? control : control.querySelector?.(unnamed);
  if (named && !named.hasAttribute('aria-label') && !named.hasAttribute('aria-labelledby')) {
    named.setAttribute('aria-labelledby', t.id);
  }
  ctl.appendChild(control);
  r.appendChild(ctl);

  return r;
}

function toggle({ checked, onChange }) {
  const btn = document.createElement('button');
  btn.type = 'button';
  btn.className = 'toggle' + (checked ? ' is-on' : '');
  btn.setAttribute('role', 'switch');
  btn.setAttribute('aria-checked', String(!!checked));
  btn.addEventListener('click', () => onChange(!checked));
  return btn;
}

function select({ value, options, onChange }) {
  const sel = document.createElement('select');
  sel.className = 'select';
  for (const opt of options) {
    const o = document.createElement('option');
    o.value = opt.value;
    o.textContent = opt.label;
    if (opt.value === value) o.selected = true;
    sel.appendChild(o);
  }
  sel.addEventListener('change', () => onChange(sel.value));
  return sel;
}

function escapeHtml(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

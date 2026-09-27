// settings.js — Settings surface in Tool register.
//
// macOS-style two-column form, centered max-width 720px. Four sections:
// Startup, Updates, Display, About. Local edited state diffs against the
// persisted saved state; when they differ, an action bar appears at the
// bottom of the form. No themes — the dashboard only has one register.

import * as api from '../api.js';

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
    if (savedFlashTimer) clearTimeout(savedFlashTimer);
    savedFlashTimer = setTimeout(() => { flashEl.textContent = ''; }, 2400);
  }

  function flashError(msg) {
    flashEl.textContent = `Couldn't save: ${msg}`;
    flashEl.style.color = 'var(--status-bad)';
    if (savedFlashTimer) clearTimeout(savedFlashTimer);
    savedFlashTimer = setTimeout(() => { flashEl.textContent = ''; }, 5000);
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

    // Updates
    const updateRows = [
      row({
        title: 'Check for updates automatically',
        help: 'Geniuz only checks; it never installs without your click.',
        control: toggle({
          checked: !!edited.autoupdate_enabled,
          onChange: (v) => update({ autoupdate_enabled: v }),
        }),
      }),
    ];
    if (edited.autoupdate_enabled) {
      updateRows.push(row({
        title: 'Check frequency',
        help: '',
        control: select({
          value: edited.update_check_frequency || 'weekly',
          options: [
            { value: 'daily',  label: 'Daily' },
            { value: 'weekly', label: 'Weekly' },
            { value: 'manual', label: 'Manual' },
          ],
          onChange: (v) => update({ update_check_frequency: v }),
        }),
      }));
    }
    shell.appendChild(section('Updates', updateRows));

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

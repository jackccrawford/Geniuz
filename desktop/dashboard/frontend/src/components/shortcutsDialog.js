// Keyboard shortcuts: a panel to look them up, and to change one.
//
// It opens from Help > Keyboard Shortcuts and from a row in Settings. It used to
// be a long list in the middle of Settings, where it read as help that had taken
// over the page, and its shortcuts looked like plain text nobody would think to
// click (Jack, 10 Nov 2026). Here each one is drawn as key caps, and the panel
// says once that clicking a shortcut changes it.
//
// A change saves as soon as it's made and shows in the menu at once.

import * as api from '../api.js';

const isMac = () => /Mac/.test(navigator.platform);

// "CmdOrCtrl+Shift+N" -> the caps a person sees on their own keyboard.
function caps(keys) {
  if (!keys) return [];
  const mac = isMac();
  const names = {
    CmdOrCtrl: mac ? '⌘' : 'Ctrl',
    Shift: mac ? '⇧' : 'Shift',
    Alt: mac ? '⌥' : 'Alt',
    Comma: ',',
  };
  return keys.split('+').map((k) => names[k] || k);
}

function keysFromEvent(e) {
  if (['Meta', 'Control', 'Shift', 'Alt'].includes(e.key)) return null;
  const parts = [];
  if (e.metaKey || e.ctrlKey) parts.push('CmdOrCtrl');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  if (!parts.length) return null; // a plain letter would fire while typing
  const code = e.code.replace(/^Key/, '').replace(/^Digit/, '');
  parts.push(code === 'Comma' ? ',' : code);
  return parts.join('+');
}

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

function capsHtml(keys) {
  const list = caps(keys);
  return list.length
    ? list.map((k) => `<kbd class="keycap">${esc(k)}</kbd>`).join('')
    : '<span class="keycap keycap--none">None</span>';
}

let dialog = null;

function ensureDialog() {
  if (dialog) return dialog;
  dialog = document.createElement('dialog');
  dialog.className = 'shortcuts-dialog';
  dialog.setAttribute('aria-labelledby', 'shortcuts-title');
  // A click on the dimmed area around the panel closes it.
  dialog.addEventListener('click', (e) => { if (e.target === dialog) dialog.close(); });
  document.body.appendChild(dialog);
  return dialog;
}

export async function openShortcuts() {
  const d = ensureDialog();
  const first = isMac() ? '⌘1' : 'Ctrl+1';
  const ninth = isMac() ? '⌘9' : 'Ctrl+9';
  d.innerHTML = `
    <div class="shortcuts-dialog__panel">
      <header class="shortcuts-dialog__head">
        <h2 id="shortcuts-title">Keyboard shortcuts</h2>
        <button type="button" class="shortcuts-dialog__close" aria-label="Close">
          <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M3.5 3.5l9 9m0-9l-9 9" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>
        </button>
      </header>
      <p class="shortcuts-dialog__hint">Click a shortcut to change it.</p>
      <div class="shortcuts-dialog__list"></div>
      <p class="shortcuts-dialog__note">${esc(first)} to ${esc(ninth)} switch to your first nine folders, in sidebar order.</p>
      <p class="shortcuts-dialog__status" role="status" aria-live="polite"></p>
    </div>`;
  const list = d.querySelector('.shortcuts-dialog__list');
  const status = d.querySelector('.shortcuts-dialog__status');
  d.querySelector('.shortcuts-dialog__close').addEventListener('click', () => d.close());

  let stopRecording = null;
  // Leaving mid-change puts the menu's shortcuts back.
  d.onclose = () => {
    if (stopRecording) {
      stopRecording();
      api.refreshMenu(true).catch(() => {});
    }
  };

  const draw = async () => {
    const rows = await api.getShortcuts();
    list.innerHTML = rows.map((s, i) => `
      <div class="shortcut-row">
        <span class="shortcut-row__label" id="shortcut-label-${i}">${esc(s.label)}</span>
        <span class="shortcut-row__control">
          ${s.keys !== s.default_keys ? `<button type="button" class="shortcut-reset" data-reset="${i}">Reset</button>` : ''}
          <button type="button" class="shortcut-keys" data-change="${i}" aria-describedby="shortcut-label-${i}"
            title="Click, then press the new keys. Delete clears it, Esc cancels.">${capsHtml(s.keys)}</button>
        </span>
      </div>`).join('');
    list.querySelectorAll('[data-change]').forEach((b) => b.addEventListener('click', () => record(rows[Number(b.dataset.change)], b)));
    list.querySelectorAll('[data-reset]').forEach((b) => b.addEventListener('click', () => {
      const s = rows[Number(b.dataset.reset)];
      save(s, s.default_keys);
    }));
  };

  const save = async (s, keys) => {
    try {
      await api.setShortcut(s.id, keys);
      status.textContent = '';
    } catch (e) {
      status.textContent = e?.message || String(e);
    }
    await api.refreshMenu(true).catch(() => {});
    await draw();
  };

  // The menu would act on these keys first, so it goes without shortcuts meanwhile.
  const record = async (s, btn) => {
    if (stopRecording) return;
    btn.textContent = 'Press keys…';
    btn.classList.add('is-recording');
    await api.refreshMenu(false).catch(() => {});
    const onKey = (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === 'Escape') { finish(); api.refreshMenu(true).catch(() => {}); draw(); return; }
      if (e.key === 'Backspace' || e.key === 'Delete') { finish(); save(s, ''); return; }
      const keys = keysFromEvent(e);
      if (keys) { finish(); save(s, keys); }
    };
    const finish = () => { window.removeEventListener('keydown', onKey, true); stopRecording = null; };
    stopRecording = finish;
    window.addEventListener('keydown', onKey, true);
  };

  if (!d.open) d.showModal();
  try {
    await draw();
  } catch (e) {
    status.textContent = e?.message || String(e);
  }
}

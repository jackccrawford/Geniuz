// remember.js — author a new memory.
//
// Two-field form: Gist (single line) + Content (multi-line). Matches the TUI's
// two-field compose. Both are required: the gist is the headline every list and
// search shows, written by whoever keeps it. Submit calls `remember_memory` which
// wraps `db::keep()` — same write path as the CLI and TUI.
//
// Reply to: optional, the memory this one threads to. Empty
// starts a new thread, as before. Pick a memory by searching the chosen folder,
// by pasting an address, or with Reply on a memory's own window, which opens
// this page already pointed at it. A reply lives in the folder of the memory it
// answers, so changing the folder lets go of the choice.

import * as su from '../stream-ui.js';
import * as api from '../api.js';
import { dateTime } from '../format.js';
import { getState, navigate } from '../store.js';

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

const ADDRESS = /^([0-9a-f]{8}):([0-9a-f]{8})$/i;
const SHORT_ID = /^[0-9a-f]{8}$/i;

const address = (folder, uuid) => `${String(folder).slice(0, 8)}:${String(uuid).slice(0, 8)}`.toUpperCase();

// A gist exactly as written, nothing singled out, as everywhere in the app.
// It is written for the next reader, often an agent in thoughtform
// ("fix | auth_middleware | bug:token_refresh"), where every field carries
// weight. So it is shown whole, never parsed, sorted or filtered by.
function gistHtml(gist) {
  return gist ? su.esc(gist) : '(no gist)';
}

/**
 * Open Keep as a reply to this memory. Called from a memory's window. Whatever
 * is already typed stays; only what it answers changes.
 */
export function replyTo({ uuid, folder, gist }) {
  writeDraft({ ...readDraft(), folder, replyTo: { uuid, folder, gist } });
  navigate('remember');
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
    <p class="main-header__sub">Whoever reads this next, you or an agent, decides from the gist whether it's for them. The content is the rest.</p>
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
  // autocorrect is off on Gist and Content unless Settings turns it on: what is
  // typed is what is kept. With it on, the Mac rewrites the last word on the click
  // meant for Keep and swallows that click, so Keep needed two (Jack, 10 Nov 2026).
  form.innerHTML = `
    ${folderField}
    <div class="remember-field reply-field">
      <label class="remember-field__label" for="reply-search">Reply to <span class="remember-field__hint">· optional. Leave empty to start a new thread</span></label>
      <div class="reply-chosen" role="button" tabindex="0" title="Reply to a different memory" hidden>
        <span class="reply-chosen__arrow" aria-hidden="true">↳</span>
        <span class="reply-chosen__gist"></span>
        <span class="reply-chosen__id"></span>
        <svg class="reply-chosen__caret" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 6.5 8 3.5l3 3"/><path d="M5 9.5l3 3 3-3"/></svg>
        <button type="button" class="reply-chosen__clear" aria-label="Don't reply to this memory" title="Start a new thread instead">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round"><path d="M6 6l12 12"/><path d="M18 6L6 18"/></svg>
        </button>
      </div>
      <div class="reply-search">
        <input
          type="text"
          id="reply-search"
          class="remember-field__input"
          autocomplete="off"
          spellcheck="false"
          role="combobox"
          aria-expanded="false"
          aria-controls="reply-results"
          placeholder="Search this folder, or paste an address"
        />
        <svg class="reply-search__caret" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 6.5 8 3.5l3 3"/><path d="M5 9.5l3 3 3-3"/></svg>
        <div class="reply-results" id="reply-results" role="listbox" hidden></div>
      </div>
    </div>
    <label class="remember-field">
      <span class="remember-field__label">Gist <span class="remember-field__hint gist-hint"></span></span>
      <input
        type="text"
        class="remember-field__input"
        name="gist"
        autocomplete="off"
        autocorrect="off"
        required
        placeholder="One line saying what this is"
      />
    </label>
    <label class="remember-field">
      <span class="remember-field__label">Content</span>
      <textarea
        class="remember-field__textarea"
        name="content"
        autocorrect="off"
        rows="10"
        placeholder="What you learned, decided, or discovered."
      ></textarea>
    </label>
    <div class="remember-field remember-images">
      <span class="remember-field__label">Images <span class="remember-field__hint">· optional, up to 5 images. Paste a screenshot, drop one here, or <button type="button" class="remember-images__pick">choose a file</button></span></span>
      <input type="file" class="remember-images__file" accept="image/*" multiple hidden />
      <ul class="remember-images__list" aria-label="Images to keep with this memory"></ul>
      <p class="remember-images__msg" role="status" aria-live="polite" hidden></p>
      <p class="remember-images__note" hidden>Kept with the memory, at screen size. Say in the words what an image shows: that's how it's found later. An image stays, so look at what else is in it first.</p>
    </div>
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
  const folderSelect = form.querySelector('select[name="folder"]');
  const replySearch = form.querySelector('#reply-search');
  const replyBox = form.querySelector('.reply-search');
  const replyResults = form.querySelector('.reply-results');
  const replyChosen = form.querySelector('.reply-chosen');

  const chosenFolder = () => folderSelect?.value || shown?.uuid || null;

  // Keep needs both a gist and content (spaces alone don't count).
  const updateSubmitState = () => {
    submitBtn.disabled = !contentInput.value.trim() || !gistInput.value.trim();
  };
  contentInput.addEventListener('input', updateSubmitState);

  // Autocorrect is off in the markup; a person can turn it on in Settings.
  api.getSettings().then((s) => {
    if (s?.autocorrect) [gistInput, contentInput].forEach((el) => el.setAttribute('autocorrect', 'on'));
  }).catch(() => {});

  // ---- Images ------------------------------------------------------------
  // Each is shown before anything is kept, with a way to take it off: an image
  // is hard to take back afterwards.
  const MAX_IMAGES = 5; // as pictures::MAX_PER_MEMORY
  const picked = []; // { file, url }
  const imageList = form.querySelector('.remember-images__list');
  const imageNote = form.querySelector('.remember-images__note');
  const imageFile = form.querySelector('.remember-images__file');
  const imagePick = form.querySelector('.remember-images__pick');
  const imageMsg = form.querySelector('.remember-images__msg');
  const drawImages = () => {
    imageList.innerHTML = picked.map((p, i) => `
      <li class="remember-image">
        <img src="${p.url}" alt="Image ${i + 1} to keep">
        <button type="button" class="remember-image__remove" data-remove="${i}" aria-label="Don't keep image ${i + 1}" title="Don't keep this image">
          <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true"><path d="M2.5 2.5l7 7m0-7l-7 7" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>
        </button>
      </li>`).join('');
    imageNote.hidden = !picked.length;
    // At the limit there is nothing more to choose.
    const full = picked.length >= MAX_IMAGES;
    imagePick.disabled = full;
    imagePick.title = full ? `A memory carries at most ${MAX_IMAGES} images` : '';
  };
  const sayImages = (text) => {
    imageMsg.textContent = text || '';
    imageMsg.hidden = !text;
  };
  // Takes what it can and says, beside the images, why anything was left out.
  // Returns whether there were files at all, so a paste of plain words is left alone.
  const addImages = (files) => {
    const all = [...files];
    if (!all.length) return false;
    const images = all.filter((f) => f.type.startsWith('image/'));
    const others = all.length - images.length;
    let over = 0;
    for (const file of images) {
      if (picked.length >= MAX_IMAGES) { over += 1; continue; }
      picked.push({ file, url: URL.createObjectURL(file) });
    }
    const said = [];
    if (others) said.push(others === 1 ? "That isn't an image, so it wasn't added. Only images can be kept with a memory." : `${others} of those aren't images, so they weren't added. Only images can be kept with a memory.`);
    if (over) said.push(`A memory carries at most ${MAX_IMAGES} images, so ${over === 1 ? "one wasn't" : `${over} weren't`} added. Keep the rest in a reply.`);
    sayImages(said.join(' '));
    drawImages();
    return true;
  };
  const clearImages = () => {
    picked.forEach((p) => URL.revokeObjectURL(p.url));
    picked.length = 0;
    sayImages('');
    drawImages();
  };
  const asDataUrl = (file) => new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(r.result);
    r.onerror = () => reject(new Error(`Couldn't read ${file.name || 'that image'}`));
    r.readAsDataURL(file);
  });
  imageList.addEventListener('click', (e) => {
    const b = e.target.closest('[data-remove]');
    if (!b) return;
    const [gone] = picked.splice(Number(b.dataset.remove), 1);
    if (gone) URL.revokeObjectURL(gone.url);
    sayImages('');
    drawImages();
  });
  imagePick.addEventListener('click', () => imageFile.click());
  imageFile.addEventListener('change', () => { addImages(imageFile.files); imageFile.value = ''; });
  // Pasting words works as it always did: this only takes a clipboard that holds an image.
  form.addEventListener('paste', (e) => {
    if (addImages(e.clipboardData?.files || [])) e.preventDefault();
  });
  form.addEventListener('dragover', (e) => {
    if ([...(e.dataTransfer?.types || [])].includes('Files')) { e.preventDefault(); form.classList.add('is-dropping'); }
  });
  form.addEventListener('dragleave', () => form.classList.remove('is-dropping'));
  form.addEventListener('drop', (e) => {
    form.classList.remove('is-dropping');
    // Always ours: a drop the form doesn't take must not reach the web view.
    e.preventDefault();
    addImages(e.dataTransfer?.files || []);
  });
  gistInput.addEventListener('input', updateSubmitState);

  // ---- Reply to ----------------------------------------------------------

  let reply = null; // { uuid, folder, gist } or null
  let picks = [];   // the memories the results list offers
  let active = -1;  // which of them the arrow keys are on
  let searchToken = 0;
  let searchTimer = null;

  const closeResults = () => {
    replyResults.hidden = true;
    replyResults.innerHTML = '';
    replySearch.setAttribute('aria-expanded', 'false');
    picks = [];
    active = -1;
  };

  const showResults = (items, note = '', heading = '') => {
    picks = items;
    active = items.length ? 0 : -1;
    const head = heading && items.length ? `<p class="reply-results__heading">${su.esc(heading)}</p>` : '';
    replyResults.innerHTML = items.length
      ? head + items.map((m, i) => `
        <button type="button" class="reply-result${i === active ? ' is-active' : ''}" role="option" data-i="${i}" tabindex="-1">
          <span class="reply-result__gist">${gistHtml(m.gist)}</span>
          ${m.excerpt ? `<span class="reply-result__excerpt">${su.esc(m.excerpt)}</span>` : ''}
          <span class="reply-result__meta">${su.esc(dateTime(m.created_at))} · ${su.esc(address(m.folder, m.uuid))}</span>
        </button>`).join('')
      : `<p class="reply-results__note">${su.esc(note || 'Nothing here matches.')}</p>`;
    replyResults.hidden = false;
    replySearch.setAttribute('aria-expanded', 'true');
  };

  const markActive = () => {
    replyResults.querySelectorAll('.reply-result').forEach((el, i) => el.classList.toggle('is-active', i === active));
    replyResults.querySelector('.reply-result.is-active')?.scrollIntoView({ block: 'nearest' });
  };

  // A keep wakes whoever reads it next, so the gist is the
  // doorbell: it says who it's for and what it asks. A reply already knows who.
  const gistHint = form.querySelector('.gist-hint');
  const setGistHint = () => {
    gistHint.textContent = reply
      ? '· your answer in a line. The memory you\'re replying to is who this wakes'
      : '· who it\'s for and what it asks, e.g. "Pen, please check the 0.620 score"';
  };

  const setReply = (m) => {
    reply = m ? { uuid: m.uuid, folder: m.folder, gist: m.gist || '' } : null;
    replyChosen.hidden = !reply;
    replyBox.hidden = !!reply;
    setGistHint();
    if (reply) {
      replyChosen.querySelector('.reply-chosen__gist').innerHTML = gistHtml(reply.gist);
      replyChosen.querySelector('.reply-chosen__gist').title = reply.gist || '';
      replyChosen.querySelector('.reply-chosen__id').textContent = address(reply.folder, reply.uuid);
    }
    replySearch.value = '';
    closeResults();
  };

  const pick = (i) => {
    const m = picks[i];
    if (!m) return;
    setReply(m);
    saveDraft();
    gistInput.focus();
  };

  // What the box offers: an address or ID opens that memory; words search the
  // chosen folder by meaning; nothing at all lists the newest in this folder.
  const search = async () => {
    const q = replySearch.value.trim();
    const folder = chosenFolder();
    const token = ++searchToken;
    let items = [];
    let note = '';
    let heading = '';
    try {
      const addr = q.match(ADDRESS);
      if (addr) {
        const target = await api.resolveAddress(q);
        if (!target) note = `No memory at ${q.toUpperCase()}.`;
        else if (!folders.some((f) => f.uuid === target.folder)) note = 'That memory is in a folder this window doesn\'t keep to.';
        else {
          const m = await api.getMemoryDetail(target.uuid, target.folder);
          if (m) items = [{ ...m, folder: target.folder }];
        }
      } else if (SHORT_ID.test(q)) {
        const m = await api.getMemoryDetail(q, folder);
        if (m) items = [{ ...m, folder }];
        else note = `No memory ${q.toUpperCase()} in this folder.`;
      } else if (q) {
        // Both ways at once, as Find does: exact first, for names and numbers
        // ("Dana", "2024"); then by meaning, for what you can't quite name.
        const scope = folder ? [folder] : null;
        const [exact, meaning] = await Promise.all([
          api.find(q, 'exact', null, 30, scope).catch(() => null),
          api.find(q, 'meaning', null, 8, scope),
        ]);
        // Both read the whole memory, not just the gist, so each row shows the
        // words it was found by: the matching line, or how the memory begins.
        const matched = new Map(); // memory → every line that matched, in order
        for (const l of exact?.lines || []) {
          if (!matched.has(l.uuid)) matched.set(l.uuid, []);
          matched.get(l.uuid).push(l.line);
        }
        const exactIds = [...matched.keys()].slice(0, 4);
        const seen = new Set(exactIds);
        const meaningIds = (meaning.memories || []).map((m) => m.uuid).filter((id) => !seen.has(id));
        const ids = [...exactIds, ...meaningIds].slice(0, 8);
        const details = await Promise.all(ids.map((id) => api.getMemoryDetail(id, folder).catch(() => null)));
        items = details.filter(Boolean).map((m) => {
          const body = su.bodyAfterGist(m.content, m.gist) || '';
          // A matching line from the body says more than the gist row above it.
          const hit = (matched.get(m.uuid) || []).find((line) => line.trim() !== (m.gist || '').trim());
          const excerpt = (hit || body).replace(/\s+/g, ' ').trim();
          return { ...m, folder, excerpt };
        });
      } else if (folder && folder === shown?.uuid) {
        items = (await api.getRecentMemories(6)).map((m) => ({ ...m, folder }));
        note = 'Nothing kept here yet.';
        const total = folders.find((f) => f.uuid === folder)?.memories || 0;
        if (total > items.length) heading = `Newest of ${total.toLocaleString()} · type to search them all`;
      } else {
        closeResults();
        return;
      }
    } catch (e) {
      note = `Couldn't search: ${e?.message || e}`;
    }
    if (token !== searchToken) return; // a newer search is on its way
    showResults(items, note, heading);
  };

  replySearch.addEventListener('input', () => {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(search, 220);
  });
  replySearch.addEventListener('focus', () => { if (!replySearch.value.trim()) search(); });

  // A chosen reply is a pulldown too: click it to pick a different memory.
  // Until a new one is picked, the old choice stands, and Esc or clicking
  // away brings it back. Only the × starts a new thread.
  const keepChoice = () => {
    replySearch.value = '';
    closeResults();
    setReply(reply);
  };
  const changeChoice = () => {
    replyChosen.hidden = true;
    replyBox.hidden = false;
    replySearch.focus();
  };
  replyChosen.addEventListener('click', (e) => {
    if (!e.target.closest('.reply-chosen__clear')) changeChoice();
  });
  replyChosen.addEventListener('keydown', (e) => {
    if ((e.key === 'Enter' || e.key === ' ' || e.key === 'ArrowDown') && e.target === replyChosen) {
      e.preventDefault();
      changeChoice();
    }
  });

  replySearch.addEventListener('blur', () => {
    // Let a click on a result land before the list goes.
    setTimeout(() => {
      if (replyBox.contains(document.activeElement)) return;
      if (reply) keepChoice();
      else closeResults();
    }, 150);
  });
  replySearch.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && reply) {
      e.preventDefault();
      keepChoice();
      replyChosen.focus();
      return;
    }
    if (replyResults.hidden) return;
    if (e.key === 'ArrowDown' && picks.length) { e.preventDefault(); active = (active + 1) % picks.length; markActive(); }
    else if (e.key === 'ArrowUp' && picks.length) { e.preventDefault(); active = (active - 1 + picks.length) % picks.length; markActive(); }
    else if (e.key === 'Enter' && !(e.metaKey || e.ctrlKey)) {
      // Enter here picks a memory; it doesn't send the form.
      e.preventDefault();
      if (active >= 0) pick(active);
    } else if (e.key === 'Escape') { e.preventDefault(); closeResults(); }
  });
  replyResults.addEventListener('mousedown', (e) => e.preventDefault()); // keep focus in the box
  // One highlighted row: the mouse moves it just as the arrow keys do.
  replyResults.addEventListener('mousemove', (e) => {
    const btn = e.target.closest('.reply-result');
    if (btn && Number(btn.dataset.i) !== active) { active = Number(btn.dataset.i); markActive(); }
  });
  replyResults.addEventListener('click', (e) => {
    const btn = e.target.closest('.reply-result');
    if (btn) pick(Number(btn.dataset.i));
  });
  replyChosen.querySelector('.reply-chosen__clear').addEventListener('click', () => {
    setReply(null);
    saveDraft();
    replySearch.focus();
  });

  // ---- Draft -------------------------------------------------------------

  const draft = readDraft();
  gistInput.value = draft.gist || '';
  contentInput.value = draft.content || '';
  // A choice made for this draft stands, if that folder is still there.
  if (folderSelect && folders.some((f) => f.uuid === draft.folder)) folderSelect.value = draft.folder;
  setGistHint();
  if (draft.replyTo && draft.replyTo.folder === chosenFolder()) setReply(draft.replyTo);
  updateSubmitState();

  function saveDraft() {
    writeDraft({ gist: gistInput.value, content: contentInput.value, folder: folderSelect?.value, replyTo: reply });
  }
  gistInput.addEventListener('input', saveDraft);
  contentInput.addEventListener('input', saveDraft);
  folderSelect?.addEventListener('change', () => {
    if (reply && reply.folder !== folderSelect.value) {
      setReply(null);
      status.textContent = 'A reply stays in the folder of the memory it answers, so this one will start a new thread.';
    }
    saveDraft();
  });

  // Focus where the writing starts: the gist, or the words of a reply.
  setTimeout(() => (reply ? contentInput : gistInput).focus(), 0);

  cancelBtn.addEventListener('click', () => {
    clearDraft();
    navigate('recent');
  });

  // ---- Keep --------------------------------------------------------------

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const gist = gistInput.value.trim();
    const content = contentInput.value.trim();
    if (!content || !gist) return;
    submitBtn.disabled = true;
    submitBtn.textContent = 'Keeping…';
    status.textContent = picked.length ? 'Keeping, and preparing the images…' : 'Keeping…';
    // Let the window draw the pressed button before the work starts.
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    const idle = () => { submitBtn.textContent = 'Keep'; };
    try {
      const chosen = chosenFolder();
      const parent = reply && reply.folder === chosen ? reply.uuid : null;
      const images = await Promise.all(picked.map((p) => asDataUrl(p.file)));
      const shortUuid = await api.rememberMemory(gist || null, content, chosen, parent, images);
      clearImages();
      const answered = parent ? `, a reply to ${String(parent).slice(0, 8).toUpperCase()}` : '';
      clearDraft();
      if (chosen && chosen !== shown?.uuid) {
        // It went elsewhere, so the list here won't show it. Say so, and stay.
        const name = folders.find((f) => f.uuid === chosen)?.name || 'that folder';
        status.textContent = `Kept in ${name} · ${shortUuid}${answered}`;
        gistInput.value = '';
        contentInput.value = '';
        folderSelect.value = shown.uuid;
        setReply(null);
        idle();
        updateSubmitState();
        gistInput.focus();
        return;
      }
      status.textContent = `Kept · ${shortUuid}${answered}`;
      // Navigate to Recent and select the new memory
      setTimeout(() => navigate('recent'), 300);
    } catch (err) {
      status.textContent = `Couldn't save: ${err?.message || err}`;
      idle();
      submitBtn.disabled = false;
    }
  });

  // Cmd-Enter / Ctrl-Enter submits from any field
  const submitOnHotkey = (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      if (!submitBtn.disabled) form.requestSubmit();
    }
  };
  folderSelect?.addEventListener('keydown', submitOnHotkey);
  replySearch.addEventListener('keydown', submitOnHotkey);
  gistInput.addEventListener('keydown', submitOnHotkey);
  contentInput.addEventListener('keydown', submitOnHotkey);
}

// stream-ui.js — the pieces the memory stream draws with: a memory's place
// in its tree, its words, and times. Nothing here reads a category or a
// speaker out of the words: those belong to the writer.

import { parseSqliteIso } from './format.js';

export function esc(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}


/**
 * A memory's body, without repeating its gist. A gist left blank is taken from
 * the first line, so showing both would say the same thing twice.
 */
export function bodyAfterGist(text, gist) {
  const body = String(text || '').trim();
  const same = (s) => String(s || '').replace(/\s+/g, ' ').trim().toLowerCase();
  const g = same(gist).replace(/(…|\.\.\.)$/, '');
  if (!g) return body;
  const nl = body.indexOf('\n');
  const first = nl < 0 ? body : body.slice(0, nl);
  if (same(first) !== g) return body;
  return nl < 0 ? '' : body.slice(nl + 1).trim();
}

/**
 * Where a memory sits in its folder's tree: a dot starts a thread, a hook
 * answers one. Structure only, from the parent the substrate records: a
 * folder holds a tree of any depth and width. Meaning is the writer's, in the words, so no colour or icon here
 * stands for a kind of memory. The menu bar draws the same two marks.
 */
export function node(isReply) {
  return isReply
    ? `<span class="snode snode--reply" title="A reply" aria-label="A reply"><svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M4 2.5v6.5a2 2 0 0 0 2 2h6" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/><circle cx="12.5" cy="11" r="1.7" fill="currentColor"/></svg></span>`
    : `<span class="snode snode--root" title="Starts a thread" aria-label="Starts a thread"><svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><circle cx="8" cy="8" r="3.5" fill="currentColor"/></svg></span>`;
}

export function timeShort(at) {
  const d = parseSqliteIso(at);
  return d ? d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' }) : '';
}

export function timeFull(at) {
  const d = parseSqliteIso(at);
  return d ? d.toLocaleString(undefined, { weekday: 'long', month: 'long', day: 'numeric', year: 'numeric', hour: 'numeric', minute: '2-digit' }) : '';
}

export function dayKey(at) {
  const d = parseSqliteIso(at);
  return d ? `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}` : '';
}

export function dayLabel(at) {
  const d = parseSqliteIso(at);
  if (!d) return '';
  const now = new Date();
  const start = (x) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((start(now) - start(d)) / 86400000);
  if (days === 0) return 'Today';
  if (days === 1) return 'Yesterday';
  const opts = { weekday: 'long', month: 'long', day: 'numeric' };
  if (d.getFullYear() !== now.getFullYear()) opts.year = 'numeric';
  return d.toLocaleDateString(undefined, opts);
}

/**
 * A memory's words as HTML: code kept verbatim, links that open in the
 * browser, station pointers (ABCD1234:EF567890) as chips, **bold**, breaks.
 */
export function renderText(content) {
  let s = esc(content);
  const keep = [];
  const hold = (html) => { keep.push(html); return `\u0000${keep.length - 1}\u0000`; };
  s = s.replace(/```(?:[a-z0-9]*\n)?([\s\S]*?)```/g, (_, code) => hold(`<pre class="scode">${code.replace(/^\n|\n$/g, '')}</pre>`));
  s = s.replace(/`([^`\n]+)`/g, (_, code) => hold(`<code>${code}</code>`));
  s = s.replace(/\bhttps?:\/\/[^\s<]+[^\s<.,;:!?)\]'"&]/g, (url) =>
    hold(`<a class="slink" href="#" data-href="${url}" title="${url}">${url}</a>`));
  s = s.replace(/\b([0-9A-F]{8}):([0-9A-F]{8})\b/g, (m) => hold(`<span class="sptr" title="a memory in a folder">${m}</span>`));
  s = s.replace(/\*\*([^*\n]+)\*\*/g, '<strong>$1</strong>');
  s = s.replace(/\n/g, '<br>');
  return s.replace(/\u0000(\d+)\u0000/g, (_, i) => keep[+i]);
}

// stream-ui.js — the pieces the memory stream draws with: a mark per
// category, the words of a memory, and times. The same hand as Geniuz
// Team's room (exchange-ui.js), without the people: Free's memories have no
// speakers, so a memory's category wears the colour a member would.

import { parseSqliteIso } from './format.js';

export function esc(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

const TINTS = ['#2F94A6', '#1A6E7C', '#E0A43B', '#CF5535', '#6E8B3D', '#7A5C99', '#B5654A', '#3E7CB1'];

export function tintOf(key) {
  let h = 2166136261;
  for (const c of String(key || '')) {
    h ^= c.charCodeAt(0);
    h = Math.imul(h, 16777619);
  }
  return TINTS[(h >>> 0) % TINTS.length];
}

/** The short tag before the gist's first separator, if it reads as one. */
export function splitGist(gist) {
  const g = String(gist || '');
  const at = ['|', ';', ':'].map((c) => g.indexOf(c)).filter((i) => i > 0 && i < 40);
  if (!at.length) return { tag: null, rest: g };
  const i = Math.min(...at);
  const tag = g.slice(0, i).trim();
  if (!tag || tag.split(/\s+/).length > 3) return { tag: null, rest: g };
  return { tag, rest: g.slice(i + 1).trim() };
}

/** A memory's mark: its category's first letter on the category's colour. */
export function mark(tag, size = 36) {
  const key = (tag || 'memory').toLowerCase();
  const letter = tag ? tag.trim()[0].toUpperCase() : '·';
  return `<span class="sm" style="--sm:${size}px;--st:${tintOf(key)}" aria-hidden="true">${esc(letter)}</span>`;
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
  s = s.replace(/\b([0-9A-F]{8}):([0-9A-F]{8})\b/g, (m) => hold(`<span class="sptr" title="a memory on a station">${m}</span>`));
  s = s.replace(/\*\*([^*\n]+)\*\*/g, '<strong>$1</strong>');
  s = s.replace(/\n/g, '<br>');
  return s.replace(/\u0000(\d+)\u0000/g, (_, i) => keep[+i]);
}

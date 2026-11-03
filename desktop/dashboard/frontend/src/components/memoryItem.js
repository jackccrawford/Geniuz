// memoryItem.js — one row in a memory list.

import { renderMd } from '../md.js';
import { ago } from '../format.js';
import { navigate } from '../store.js';
import { openMemory } from './memoryModal.js';

export function memoryItem(m) {
  const el = document.createElement('div');
  el.className = 'memory-item';
  const isThread = !!(m.parent_uuid || m.parent_id);
  const tagStr = (m.category ? [m.category.toLowerCase()] : m.tags || []).join(' · ');
  el.innerHTML = `
    <span class="m-marker ${isThread ? 'threaded' : ''}" aria-hidden="true"></span>
    <div class="m-body">
      <div class="m-gist">${renderMd(m.gist)}</div>
      <div class="m-meta">
        ${tagStr}
        ${isThread ? '<span style="color:var(--color-ink-senary)">·</span><span style="color:var(--color-ink-tertiary)">↳ follow-up</span>' : ''}
      </div>
    </div>
    <div class="m-time">${ago(m.created_at)}</div>
  `;
  el.addEventListener('click', () => {
    if (m.uuid) openMemory(m.uuid);
  });
  el.style.cursor = 'pointer';
  return el;
}

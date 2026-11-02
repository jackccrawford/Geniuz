// folderPanel.js — right-side panel: folder stats + Show in Finder.

import * as fmt from '../format.js';
import { openPath, getDataDir } from '../api.js';

export function folderPanel({ stats, version }) {
  const el = document.createElement('div');
  el.className = 'side-panel';

  el.innerHTML = `
    <div class="sp-title">Folder</div>
    <div style="margin-top:8px;">
      <div class="kv-row">
        <span class="kv-key">Total</span>
        <span class="kv-val">${fmt.number(stats.total_memories)} memories</span>
      </div>
      <div class="kv-row">
        <span class="kv-key">All searchable</span>
        <span class="kv-val" style="color:var(--color-good);">yes</span>
      </div>
      <div class="kv-row">
        <span class="kv-key">Last updated</span>
        <span class="kv-val" style="font-weight:500;">${fmt.ago(stats.last_write_iso)}</span>
      </div>
    </div>
    <button class="btn" id="showInFinderBtn" style="width:100%; justify-content:center; margin-top:10px; font-size:12px;">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="width:13px;height:13px;"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
      Show in Finder
    </button>
  `;

  el.querySelector('#showInFinderBtn').addEventListener('click', async () => {
    try {
      const dir = await getDataDir();
      await openPath(dir);
    } catch (e) {
      console.error('[geniuz] open data dir failed:', e);
    }
  });

  return el;
}

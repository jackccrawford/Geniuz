// placeholder.js — fallback for navigation targets that are intentionally
// not implemented yet. Most surfaces (recent, remember, detail, find, status,
// data, settings) ship a real mount; this file covers the few that are
// genuinely in-flight or stubbed for a follow-up.

const COPY = {
  add: {
    title: 'Add files',
    body: 'File ingestion flow coming next: pick a file from disk, Geniuz reads it, summarizes it into a memory, indexes it. For one-off memories use Remember in the sidebar; for bulk imports today, use `geniuz capture` from the CLI.',
  },
};

const GENERIC = {
  title: 'Not available',
  body: 'This screen is not implemented in this build. Use Keep or Find from the sidebar.',
};

export async function mount(container, surfaceId) {
  const info = COPY[surfaceId] || GENERIC;
  container.innerHTML = `
    <main class="main">
      <header class="main-header">
        <div>
          <h1>${escapeHtml(info.title)}</h1>
          <p class="mh-sub">Geniuz · v2.0.0</p>
        </div>
      </header>
      <div class="main-body" style="display:block;">
        <div class="empty-state" style="max-width:560px;margin:48px auto;text-align:left;">
          <p style="font-size:15px;line-height:1.6;color:var(--color-ink-secondary);">
            ${escapeHtml(info.body)}
          </p>
          <p style="font-size:13px;color:var(--color-ink-quaternary);margin-top:24px;">
            Click <strong>Recent</strong> in the sidebar to return to the dashboard.
          </p>
        </div>
      </div>
    </main>
  `;
}

function escapeHtml(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

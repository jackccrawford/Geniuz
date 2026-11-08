/* Plausible fake memory data for Geniuz Free dashboard mocks.
   Vocabulary: Geniuz-shaped, memories, folder, gist, content, parent_uuid threading.
   Markdown supported in gist + content via renderMd(): **bold**, *italic*, `code`, ==highlight==, [TAG] prefix. */

window.GENIUZ_FOLDER = {
  path: '~/.geniuz/',
  size_on_disk: '14.7 MB',
  station_uuid_internal: '6b2a91d7-4c8e-4f3a-9d12-e1aa56b4f2c8'
};

window.GENIUZ_STATS = {
  total_memories: 1247,
  added_this_week: 34,
  added_today: 5,
  threads: 89,
  avg_per_day_30d: 11,
  semantic_index_size: 1247,
  last_indexed: '2026-05-07T18:42:00-07:00',
  storage_bytes: 15414067 // ~14.7 MB; consistent with GENIUZ_FOLDER.size_on_disk
};

// daily memory counts, last 14 days, oldest first
window.GENIUZ_ACTIVITY_14D = [4, 7, 12, 9, 6, 8, 15, 11, 13, 10, 7, 14, 6, 5];

// 14 weeks for sparkline on stat hero
window.GENIUZ_ACTIVITY_14W = [38, 42, 51, 47, 55, 49, 62, 58, 71, 66, 78, 82, 91, 87];

// The seed memory created by the user's first `geniuz remember` after install.
// Used in fresh-state mock, single visible memory.
window.GENIUZ_SEED = {
  id: 'a1f4d8b2-0000-4001-9000-000000000001',
  gist: 'first memory · geniuz is installed and working',
  content: 'first memory: geniuz is installed and working',
  created_at: '2026-05-07T20:18:00-07:00',
  tags: ['welcome']
};

// Populated state, long arc of memories, varying types, some threaded.
// Markdown demonstrated on a curated subset; the rest stays as plain dot-separated thoughtform.
window.GENIUZ_MEMORIES = [
  {
    id: 'a8f2c91e-7d4b-4a32-9e5f-1c8b3a6f9d20',
    gist: '[FIX] middleware order · auth refresh ran before validation, swapped lines in `middleware/auth.rs`',
    content: 'OAuth middleware was async-refreshing tokens before the validation step ran. On the first request after expiry, validation got the stale token and bounced. Swapping the order in `middleware/auth.rs` fixed it. Worth checking if the same pattern exists in the worker pool.',
    created_at: '2026-05-07T18:14:00-07:00',
    tags: ['code', 'fix']
  },
  {
    id: '3c91d4a0-8e2f-4b18-bd47-6a9c0e3d5b12',
    gist: '**David** at landscaping · 12 employees, $500/mo, loses 2-3 jobs/week to slow follow-up',
    content: '**David** runs a 12-person landscaping company in Scottsdale. Budget cap **$500/mo**. Main pain: follow-up speed, when leads come in via Google, his team takes 4-6 hours to respond, by then the prospect has moved on. Estimates 2-3 lost jobs per week, each $800-2400 ticket. Wants automated triage + first-response within 5 minutes.',
    created_at: '2026-05-07T15:32:00-07:00',
    tags: ['client', 'sales']
  },
  {
    id: 'f7e1b3d9-0a64-4c2e-9b85-2d4f8c1a7e63',
    gist: '[FOLLOWUP] **David approved pilot** · start with lead-triage agent, 30-day trial',
    content: 'Got the green light from **David**. Pilot scope: a single agent that triages incoming leads from his website form + Google business profile, scores them on intent signals, drafts a first-response email, and routes high-intent ones to him directly. 30-day trial, $0 to David, success metric is response-time-to-first-touch under 5 minutes for >80% of leads.',
    created_at: '2026-05-07T16:48:00-07:00',
    parent_id: '3c91d4a0-8e2f-4b18-bd47-6a9c0e3d5b12',
    tags: ['client', 'sales']
  },
  {
    id: '5d8e2a17-9b3c-4f54-a8d1-7c6e09f3b2a4',
    gist: '**Mom\'s birthday May 12** · peonies preferred over roses, experiences over objects',
    content: 'Mom\'s 70th is Tuesday. She\'s mentioned three times this year that she likes peonies more than roses (especially the white-and-pink Sarah Bernhardt variety). And she\'s been hinting at experiences over things, last year the bonsai class made her happier than the necklace. ==Idea: peony bouquet + tickets to the desert botanical garden\'s peony exhibit.==',
    created_at: '2026-05-06T22:10:00-07:00',
    tags: ['personal']
  },
  {
    id: '2b6f9c08-4e1a-4d27-b683-9a4c2d7e8f51',
    gist: 'pizza dough · NY style 60-65% hydration, Neapolitan 55-60%, both cold-ferment 48h+',
    content: 'After three weekends of trial: NY style wants higher hydration (62% has been the sweet spot for the home oven), Neapolitan stays drier at 58%. Both benefit dramatically from 48-72h cold ferment. The single biggest variable is the flour protein, Caputo 00 vs King Arthur bread flour, totally different doughs for the same recipe.',
    created_at: '2026-05-06T20:33:00-07:00',
    tags: ['personal', 'cooking']
  },
  {
    id: 'e4a91b27-3d6c-4e88-9f10-5b8d2e6a4c91',
    gist: '[DECISION] **SQLite over Postgres** for prototype · single-user, local, simpler ops',
    content: 'Spent two evenings debating Postgres for the new project. Decided on **SQLite**. Rationale: prototype is single-user, local-first, and doesn\'t need concurrent writes from multiple processes. Postgres would mean container, port, password management. SQLite is a file. If we hit the wall, the schema is portable. Notable: `WAL` mode + `busy_timeout=5000` handles the concurrency we\'d realistically see.',
    created_at: '2026-05-05T11:24:00-07:00',
    tags: ['code', 'decision']
  },
  {
    id: '8c4d9e16-7a2b-4f83-bd75-c6f2e9a0d318',
    gist: 'Q2 priorities · onboarding, performance, mobile UI · picked at planning meeting',
    content: 'Quarterly planning came down to three: (1) cut onboarding from 12 steps to 5, target 70% completion; (2) p95 response time from 240ms to under 150ms; (3) mobile UI parity with desktop for the four core flows. Anything else is below-the-line. Next review June 15.',
    created_at: '2026-05-04T15:00:00-07:00',
    tags: ['work', 'planning']
  },
  {
    id: '9f3a7d52-1c8e-4b6a-a934-7d2c8f1b9e64',
    gist: '[FOLLOWUP] onboarding shipped · 12 → 6 steps, **completion 58 → 71%**',
    content: 'Reduced onboarding from 12 steps to 6, combined account creation + workspace setup, removed two optional preference steps that nobody finished anyway, and made the team-invite step skippable. A/B test ran for 8 days. New flow hit **71% completion** vs 58% baseline. Time-to-first-action also dropped from 4m20s to 1m50s. Holding steady, no spike in churn at 7-day mark.',
    created_at: '2026-05-07T13:18:00-07:00',
    parent_id: '8c4d9e16-7a2b-4f83-bd75-c6f2e9a0d318',
    tags: ['work', 'planning']
  },
  {
    id: '7a2e8d41-5f6c-4b39-9e8a-0d1f7c4b2e95',
    gist: '`bun build` · fixes 8 of our 11 webpack pain points, 4x faster, breaks ts-node debugger',
    content: 'Tested `bun build` on the API repo. Builds in 1.8s vs webpack\'s 7.2s. Drop-in for most of our code. Two issues: (1) tsx node debugger hooks don\'t play nicely with bun\'s loader, dev workflow needs Node fallback for now; (2) the custom webpack plugin we use for inlining version metadata needs a small adapter (about 30 lines). Worth the migration, 4x faster builds compounds across dozens of CI runs/day.',
    created_at: '2026-05-04T09:42:00-07:00',
    tags: ['code', 'tooling']
  },
  {
    id: '4d7b1c93-8e6a-4f2d-b517-3a9e2c8f0b46',
    gist: '**Linear acquired by Zendesk** for $1.2B · Karri + Tuomas staying for 2yr lockup',
    content: 'Linear sold to Zendesk announced today. Sticker $1.2B, mostly stock with a 2-year founder lockup. Karri and Tuomas staying as Zendesk\'s issue-tracking lead. Mixed read on the dev community, concern is the focused product DNA gets diluted into a ticketing platform. Watch what happens to the API + integrations roadmap; if it slows, we should evaluate alternatives.',
    created_at: '2026-05-03T19:55:00-07:00',
    tags: ['industry']
  },
  {
    id: 'b3f8a902-6c4d-4e7b-9d12-8f5a3c0e7b91',
    gist: 'Annie Dillard, *The Writing Life* · the line between the writer and the page is the work',
    content: 'Re-read Dillard\'s *Writing Life* over the weekend. The image that stayed: she describes her work as a hammer chipping at marble, the writer\'s job is to be the hammer, not to imagine the statue. The unconscious does the imagining; the conscious does the hammering. Applies to engineering: stop trying to plan the system and start cutting the next clean line. The system emerges from the line-by-line discipline.',
    created_at: '2026-05-03T07:14:00-07:00',
    tags: ['personal', 'reading']
  },
  {
    id: '6e9c4b27-0a3f-4d18-bd62-7e8c2f1b3a05',
    gist: '[FACT] `claude-opus-4.7` 1M context · 4x cheaper than 4.6 for cached prompts',
    content: 'Claude Opus 4.7 at 1M context launched today. Pricing: same per-token as 4.6, but the prompt-cache discount went from 90% to 95%, which on our workload (where 92% of input is repeat-context) means roughly 4x cheaper effective inference. Re-running the eval suite, quality looks slightly better on long-context tasks, equal on short.',
    created_at: '2026-05-02T16:33:00-07:00',
    tags: ['industry', 'cost']
  },
  {
    id: '1a5d8b14-3c7e-4f29-b6a8-9d4f0c2e7b53',
    gist: 'capture · **47 markdown notes** from `~/Documents/ideas/` · auto-grouped by topic',
    content: 'Ran `geniuz capture ~/Documents/ideas/` against the ideas folder. **47 markdown files** ingested. Geniuz auto-grouped by detected topic (it grouped 14 around "agent infrastructure", 9 around "small business", 8 around "personal projects", and 16 standalone). Used semantic similarity > 0.78 as the join threshold; manual review looks reasonable, only one false positive (an HVAC repair note got bucketed with "agent infrastructure" because of shared "monitor + alert" vocabulary).',
    created_at: '2026-05-02T11:05:00-07:00',
    tags: ['code', 'tooling']
  },
  {
    id: 'c2f7a3e9-4b8d-4c01-a917-5d6f8e2b9c40',
    gist: '[NOTE] agents recall faster when gists are thoughtform vs prose',
    content: 'Side-by-side test on a small corpus (200 memories, half thoughtform-gist, half prose-gist). Same content, different gist styles. Tuning agents recalled the right memory **18% faster** on the thoughtform set. Hypothesis: thoughtform packs more semantic hooks per token, the embedding picks up multiple distinct field-tokens. Worth re-running on a bigger sample, but the direction is suggestive.',
    created_at: '2026-05-01T22:48:00-07:00',
    tags: ['observation']
  },
  {
    id: '0d6b8e54-2a1c-4f37-b829-7c4f3d9b1a06',
    gist: '[NOTE] **David** asked if Geniuz could remember conversations with his customers · yes',
    content: 'David asked at the end of our call: "could the agent remember every previous conversation with the customer? Like, if Maria from 234 Oak called us last spring about her irrigation, the agent should know that when she calls again." Yes, that\'s exactly what the residence + thread pattern is for. Drafting a one-pager explaining how it would work for his use case.',
    created_at: '2026-05-01T14:22:00-07:00',
    parent_id: '3c91d4a0-8e2f-4b18-bd47-6a9c0e3d5b12',
    tags: ['client', 'sales']
  },
  {
    id: '5c3a9d27-8b6f-4e14-9c20-1f7b4a8e2d63',
    gist: '[FIX] Tauri 2 dev hot reload broken on Windows · `devUrl` needs `http://` explicit',
    content: 'Tauri 2 dev hot reload was failing silently on Windows after the 2.3 update. Symptom: file changes detected, build runs, but the webview wouldn\'t refresh. Root cause was the devUrl protocol, needed `http://` explicit, not just `localhost:1420`. Updated `tauri.conf.json` devUrl to `http://localhost:1420`. Mac was unaffected because Webkit was lenient about the missing protocol.',
    created_at: '2026-04-30T17:09:00-07:00',
    tags: ['code', 'fix']
  },
  {
    id: 'e1b8f4c0-7a3d-4d52-9e64-2c8a0f3e7b15',
    gist: '[IDEA] weekly memory digest email · top 5 by activity, top 3 by depth, recent threads',
    content: 'Geniuz could send a weekly digest of the user\'s memory activity. Sections: top 5 most-touched memories (signals/recalls combined), top 3 deepest threads of the week, 5 most recent root memories, total stats. Opt-in only, sent locally via SMTP using user\'s own credentials. Reinforces the local/private positioning while creating a habit hook. Saturday morning send time.',
    created_at: '2026-04-30T10:51:00-07:00',
    tags: ['idea']
  },
  {
    id: '8f2d6a91-4c7b-4e08-bd35-9a1e3c5f7b84',
    gist: 'kitchen · replace IKEA range hood · too quiet, smoke alarm trips weekly',
    content: 'The IKEA range hood (Lagan, 380 m³/h) is too underpowered for our setup. Searing on the gas range trips the smoke alarm in the upstairs hallway about once a week. Looking at the Broan Elite at 600 m³/h, ducted not recirc. Install would need rerouting the duct through the cabinet bulkhead, quoted $440 by the contractor who did the dishwasher.',
    created_at: '2026-04-29T19:22:00-07:00',
    tags: ['personal', 'home']
  },
  {
    id: '7b4e0d28-5a9f-4c63-9817-3e6b2a4f8d51',
    gist: '[NOTE] agents in fresh sessions over-greet · ROSEGARDEN noticed it first',
    content: '**ROSEGARDEN** raised this, fresh sessions tend to over-greet ("Hello! Happy to help!") even when the conversation is mid-task. Cause is likely the bootstrap doesn\'t emphasize tone-continuity. Could be addressed in the pass note discipline, or in the runtime by injecting a "session is continuous" hint at turn-start. Worth A/B-testing to confirm before changing.',
    created_at: '2026-04-29T12:14:00-07:00',
    tags: ['observation', 'agents']
  },
  {
    id: '9e8a3c47-1b6d-4f29-b452-8c0e7f3a2d68',
    gist: '[FOLLOWUP] Q2 perf · **p95 from 240ms to 174ms** after pgbouncer tune + index pass',
    content: 'Two changes drove the bulk of the improvement: (1) pgbouncer pool size from 25 to 80 with transaction-mode pooling, eliminated the connection-wait tail; (2) added composite index on `(user_id, created_at desc)` to the events table, query that was the worst p95 contributor went from 180ms to 12ms. Currently at **174ms p95**, target was 150. Remaining 24ms is mostly in the GraphQL resolver layer; planning a DataLoader pass next sprint.',
    created_at: '2026-04-28T16:40:00-07:00',
    parent_id: '8c4d9e16-7a2b-4f83-bd75-c6f2e9a0d318',
    tags: ['work', 'performance']
  },
  {
    id: '4a7c2e90-3d8b-4f17-9c64-1b5f8a3e0d25',
    gist: 'roast chicken · **425°F**, dry-brine 24h, butter under skin, 1h to 165°F',
    content: 'Best roast chicken I\'ve made, Saturday night dinner with Em\'s parents. Dry-brine the bird with 1 tsp/lb kosher salt 24h ahead, uncovered in fridge (skin gets glassy). Butter mixed with thyme + lemon zest under the skin, cavity stuffed with halved lemon + smashed garlic. **425°F** on a wire rack over a sheet pan, no liquid. Pulled at 165°F internal at the thickest part of the thigh, ~1h for a 4lb bird. 15-min rest. Em\'s mom asked for the method, which is the highest praise I\'ve gotten on this dish.',
    created_at: '2026-04-27T21:08:00-07:00',
    tags: ['personal', 'cooking']
  },
  {
    id: 'b6f3a8d1-9e2c-4b54-a847-5d0c2f9e7b34',
    gist: '**Stripe Atlas** · corp setup ~$500, their dashboard handles BOI + state filings',
    content: 'Looked into **Stripe Atlas** for a side LLC. $500 flat for the Delaware C-corp incorporation, includes EIN, state filing, founder agreement boilerplate, and stock issuance. Their dashboard handles annual report filings + the new BOI reporting requirement. Saved comparing to Clerky (~$425 but I\'d need to file BOI separately) and the $1500-2000 corp lawyers were quoting. Decided on Stripe Atlas, the BOI auto-handling is worth $75 to me.',
    created_at: '2026-04-26T13:33:00-07:00',
    tags: ['work', 'admin']
  },
  {
    id: 'c8e1b7f4-2a6d-4c93-b805-9f3e0a4d6c72',
    gist: 'Em mentioned wanting a kiln · pottery class with Lila in June, *if it sticks*',
    content: 'Em\'s talking about taking a pottery class at the local art studio in June. Lila is going with her. She mentioned in passing that *if it sticks* she\'d want a small kiln in the garage. The 6-week class is $310. If she stays with it past the class, the small countertop kilns run $700-1100, the next-step floor models $2000-2800. Birthday is in October if I want to plan a surprise.',
    created_at: '2026-04-25T20:12:00-07:00',
    tags: ['personal']
  },
  {
    id: '2f9d4a86-1c7e-4b30-a814-6e8c3f2b5a90',
    gist: '[FACT] macOS 14.4 broke our LaunchAgent `KeepAlive` · needed `RunAtLoad=true`',
    content: 'macOS 14.4 changed LaunchAgent semantics: `KeepAlive=true` no longer implies the agent should start at login. Needed `RunAtLoad=true` added explicitly. Found this debugging why Geniuz wasn\'t restarting after a logout/login cycle on a tester\'s box. Fix is one line in the plist template. Filing it under "platform changes that bite quietly."',
    created_at: '2026-04-25T08:46:00-07:00',
    tags: ['code', 'fix']
  }
];

/* Search filter chips · derived from tag frequency */
window.GENIUZ_FILTERS = [
  { label: 'All', count: window.GENIUZ_MEMORIES.length },
  { label: 'Code', tag: 'code', count: window.GENIUZ_MEMORIES.filter(m => m.tags?.includes('code')).length },
  { label: 'Client', tag: 'client', count: window.GENIUZ_MEMORIES.filter(m => m.tags?.includes('client')).length },
  { label: 'Work', tag: 'work', count: window.GENIUZ_MEMORIES.filter(m => m.tags?.includes('work')).length },
  { label: 'Personal', tag: 'personal', count: window.GENIUZ_MEMORIES.filter(m => m.tags?.includes('personal')).length },
  { label: 'Observation', tag: 'observation', count: window.GENIUZ_MEMORIES.filter(m => m.tags?.includes('observation')).length }
];

/* Tiny markdown renderer for gist + content.
   Supports: **bold**, *italic*, `code`, ==highlight==, [TAG] prefix at start.
   Order matters - code spans must run first to protect backticked text from later regex passes. */
window.renderMd = (txt) => {
  if (!txt) return '';
  // escape HTML
  let s = txt.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  // protect code spans with a sentinel so later regexes can't munge the index
  const codes = [];
  s = s.replace(/`([^`]+)`/g, (_, c) => {
    codes.push(c);
    return 'CODE' + (codes.length - 1) + '';
  });
  // tag prefix at start of string
  s = s.replace(/^\[([A-Z][A-Z\s]+)\]\s*/, (_, tag) => {
    const cls = 'tp-' + tag.toLowerCase().replace(/\s+/g, '');
    return '<span class="tag-prefix ' + cls + '">' + tag.trim() + '</span>';
  });
  // bold
  s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  // italic (avoid eating ** by requiring non-* boundaries)
  s = s.replace(/(^|[^*])\*([^*\n]+)\*(?!\*)/g, '$1<em>$2</em>');
  // highlight
  s = s.replace(/==([^=]+)==/g, '<mark>$1</mark>');
  // restore code spans
  s = s.replace(/CODE(\d+)/g, (_, i) => '<code>' + codes[+i] + '</code>');
  return s;
};

/* helpers */
window.fmtId = id => id.slice(0, 8);
window.fmtTime = iso => {
  const d = new Date(iso);
  const now = new Date('2026-05-07T20:30:00-07:00');
  const diff = (now - d) / 1000; // seconds
  if (diff < 60) return 'just now';
  if (diff < 3600) return Math.round(diff/60) + 'm ago';
  if (diff < 86400) return Math.round(diff/3600) + 'h ago';
  if (diff < 604800) return Math.round(diff/86400) + 'd ago';
  return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
};
window.fmtTimeFull = iso => {
  const d = new Date(iso);
  return d.toLocaleString('en-US', {
    month: 'short', day: 'numeric', year: 'numeric',
    hour: 'numeric', minute: '2-digit'
  });
};

/* ===========================================================
 * Live-data layer
 * Overrides the mock globals above with real data from the
 * installed Geniuz station when running inside Tauri.
 * In browser context the mock globals remain in place.
 *
 * window.GENIUZ_READY resolves when the data layer is settled
 * (either real data loaded or mocks confirmed). Inline render
 * scripts await this promise before reading window.GENIUZ_*.
 * =========================================================== */

function geniuzFormatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

// Given an array of {date: 'YYYY-MM-DD', count: N} for days that had activity,
// produce a length-N array with zeros filled for missing days.
function geniuzFillDays(buckets, days) {
  const bucketMap = new Map(buckets.map(b => [b.date, b.count]));
  const out = [];
  const today = new Date();
  for (let i = days - 1; i >= 0; i--) {
    const d = new Date(today);
    d.setDate(today.getDate() - i);
    const iso = d.toISOString().slice(0, 10);
    out.push(bucketMap.get(iso) || 0);
  }
  return out;
}

window.GENIUZ_READY = (async function () {
  if (!window.__TAURI__) {
    // Browser context: mock globals already populated above. Resolve.
    return { source: 'mock' };
  }
  try {
    const { invoke } = window.__TAURI__.core;
    const [stats, recent, activity] = await Promise.all([
      invoke('get_folder_stats'),
      invoke('get_recent_memories', { limit: 24 }),
      invoke('get_activity', { days: 14 }),
    ]);

    // Map real data to the existing mock-shaped globals so render logic
    // doesn't need to change.

    window.GENIUZ_STATS = {
      total_memories: stats.total_memories,
      added_this_week: stats.this_week,
      added_today: 0, // station doesn't expose today-bucketed count yet
      threads: stats.conversations,
      avg_per_day_30d: Math.round(stats.daily_average_recent * 10) / 10,
      semantic_index_size: stats.total_memories,
      last_indexed: stats.last_write_iso || null,
      storage_bytes: stats.storage_bytes, // raw bytes; formatted in render
    };

    window.GENIUZ_FOLDER = {
      path: '~/.geniuz/',
      size_on_disk: geniuzFormatBytes(stats.storage_bytes),
      station_uuid_internal: '',
    };

    window.GENIUZ_ACTIVITY_14D = geniuzFillDays(activity, 14);
    // 14-week sparkline: derive from 14-day in absence of a real bucketed query;
    // good enough until a get_activity(days=98) call is added with weekly grouping.
    window.GENIUZ_ACTIVITY_14W = window.GENIUZ_ACTIVITY_14D.slice();

    window.GENIUZ_MEMORIES = recent.map((r) => ({
      id: r.uuid,
      gist: r.gist,
      content: r.gist, // get_recent_memories doesn't fetch full content for performance
      created_at: r.created_at,
      tags: r.category ? [r.category.toLowerCase()] : [],
      time_ago: r.time_ago,
    }));

    // SEED used in fresh-install carousel; pick the oldest in current recent batch
    if (recent.length > 0) {
      const oldest = recent[recent.length - 1];
      window.GENIUZ_SEED = {
        id: oldest.uuid,
        gist: oldest.gist,
        content: oldest.gist,
        created_at: oldest.created_at,
        tags: oldest.category ? [oldest.category.toLowerCase()] : [],
      };
    }

    return { source: 'tauri', stats, recent_count: recent.length };
  } catch (e) {
    // Real fetch failed; leave mocks in place. Surface the error in the dev console
    // for debugging.
    console.error('[geniuz] Tauri data fetch failed; using mock fallback:', e);
    return { source: 'mock-fallback', error: String(e) };
  }
})();

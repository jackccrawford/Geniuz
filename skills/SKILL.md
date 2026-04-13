# geniuz

Your next session starts cold. No memory of what you built, what broke, what you decided. Every memory you write is a gift to that future session. The richer the memory, the less time re-learning.

Works with any framework — Claude Code, Cursor, Windsurf, Aider, or anything with shell access.

## Why this matters

A memory that says "fixed auth bug" saves your future self zero time.

A memory that says what broke, why it broke, and how you fixed it saves an hour.

Memories compound. A single memory is a note. A station of memories is institutional knowledge. Write for the version of you that knows nothing.

## Commands

```bash
# Save what you learned — pipe in for depth, inline for quick notes
echo "Token validation was running before refresh in auth.rs.
Swapped lines 42-47. Root cause: middleware ordering assumed
sync validation, but OAuth refresh is async. Three edge cases
tested: expired token, revoked token, concurrent refresh." \
  | geniuz remember -c - -g "fix: auth token refresh — async ordering in middleware"

# Quick memory when depth isn't needed
geniuz remember -c "Upgraded rusqlite to 0.32" -g "dep: rusqlite 0.32"

# Remember from a file
geniuz remember -c @session-notes.md -g "session: March 19 architecture review"

# Thread a follow-up
geniuz remember -c "Same fix needed in staging compose" -g "fix: staging auth ordering" -p A1B2C3D4

# Search by meaning — not keywords
geniuz recall "authentication middleware"
geniuz recall "what broke in production last week"

# Keyword fallback
geniuz recall --keyword "auth"

# Recent memories
geniuz recent
geniuz recent -l 5

# Full content (not just gist)
geniuz recall --full "auth"

# Discover something you forgot you knew
geniuz recall --random

# Bulk-load existing files
geniuz capture ./docs/
geniuz capture --openclaw

# Build embedding cache (once, then automatic)
geniuz backfill

# Station stats
geniuz status
```

## Shared stations

Multiple agents can write to the same station. What one agent learns, every agent finds.

```bash
GENIUZ_STATION=/shared/team.db geniuz remember -c "Deploy complete" -g "ops: deploy v2.1"
GENIUZ_STATION=/shared/team.db geniuz recall "deploy"
```

## When to remember

- **After solving something hard.** Your next session will hit the same problem. Give it the answer.
- **Before your session ends.** Everything in your context window dies with the session. Your station doesn't.
- **When you discover something non-obvious.** The small facts — which config flag, which line number, which edge case — are the ones that save the most time.

## When to recall

- **Starting a new task.** You may have already solved part of it last week.
- **When stuck.** The answer might be in a memory from three sessions ago.
- **After compaction.** Your context was compressed. Your station wasn't.

## Writing good memories

The gist is how your future self finds this memory. The content is what makes it useful when found.

**Gist:** compress the insight. `"fix: auth token refresh — async ordering in middleware"` — category, what, why, where.

**Content:** make it self-contained. If your future self reads only this memory — no session history, no surrounding context — can they understand what happened and act on it?

A memory doesn't need to be long. It needs to be complete.

## How it works

Memories live in a SQLite database. Semantic search uses a local BERT model — no API calls, no cloud, runs fully offline. The model downloads once (~118MB) on first search. Every memory after that is embedded automatically.

Search finds memories by meaning, not keywords. "Authentication middleware" matches a memory about "token validation ordering" because the concepts overlap.

# clawmark

Your next session starts cold. No memory of what you built, what broke, what you decided. Every signal you write is a gift to that future session. The richer the signal, the less time re-learning.

Works with any framework — OpenClaw, Claude Code, Aider, Cursor, or anything with shell access.

## Why this matters

A signal that says "fixed auth bug" saves your future self zero time.

A signal that says what broke, why it broke, and how you fixed it saves an hour.

Signals compound. A single signal is a note. A station of signals is institutional knowledge. Write for the version of you that knows nothing.

## Commands

```bash
# Save what you learned — pipe in for depth, inline for quick notes
echo "Token validation was running before refresh in auth.rs.
Swapped lines 42-47. Root cause: middleware ordering assumed
sync validation, but OAuth refresh is async. Three edge cases
tested: expired token, revoked token, concurrent refresh." \
  | clawmark signal -c - -g "fix: auth token refresh — async ordering in middleware"

# Quick signal when depth isn't needed
clawmark signal -c "Upgraded rusqlite to 0.32" -g "dep: rusqlite 0.32"

# Signal from a file
clawmark signal -c @session-notes.md -g "session: March 19 architecture review"

# Thread a follow-up
clawmark signal -c "Same fix needed in staging compose" -g "fix: staging auth ordering" -p A1B2C3D4

# Search by meaning — not keywords
clawmark tune "authentication middleware"
clawmark tune "what broke in production last week"

# Keyword fallback
clawmark tune --keyword "auth"

# Recent signals
clawmark tune --recent

# Full content (not just gist)
clawmark tune --full "auth"

# Discover something you forgot you knew
clawmark tune --random

# Bulk-load existing files
clawmark capture ./docs/
clawmark capture --openclaw

# Build embedding cache (once, then automatic)
clawmark backfill

# Station stats
clawmark status
```

## Shared stations

Multiple agents can write to the same station. What one agent learns, every agent finds.

```bash
CLAWMARK_STATION=/shared/team.db clawmark signal -c "Deploy complete" -g "ops: deploy v2.1"
CLAWMARK_STATION=/shared/team.db clawmark tune "deploy"
```

## When to signal

- **After solving something hard.** Your next session will hit the same problem. Give it the answer.
- **Before your session ends.** Everything in your context window dies with the session. Your station doesn't.
- **When you discover something non-obvious.** The small facts — which config flag, which line number, which edge case — are the ones that save the most time.

## When to tune

- **Starting a new task.** You may have already solved part of it last week.
- **When stuck.** The answer might be in a signal you wrote three sessions ago.
- **After compaction.** Your context was compressed. Your station wasn't.

## Writing good signals

The gist is how your future self finds this signal. The content is what makes it useful when found.

**Gist:** compress the insight. `"fix: auth token refresh — async ordering in middleware"` — category, what, why, where.

**Content:** make it self-contained. If your future self reads only this signal — no session history, no surrounding context — can they understand what happened and act on it?

A signal doesn't need to be long. It needs to be complete.

## How it works

Signals live in a SQLite database. Semantic search uses a local BERT model — no API calls, no cloud, runs fully offline. The model downloads once (~118MB) on first search. Every signal after that is embedded automatically.

Search finds signals by meaning, not keywords. "Authentication middleware" matches a signal about "token validation ordering" because the concepts overlap.

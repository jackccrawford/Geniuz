# clawmark

Persistent memory for AI agents. Works with any framework — OpenClaw, Claude Code, Aider, Cursor, or anything with shell access. Your memory survives sessions, compaction, and restarts.

## Setup

```bash
clawmark signal -c "First signal" -g "setup: ready"   # start using immediately
clawmark migrate                                       # (optional) import OpenClaw memory
clawmark backfill                                      # embed any un-cached signals
```

## Shared stations

Multiple agents can write to the same station for coordinated memory:

```bash
CLAWMARK_STATION=/shared/team.db clawmark signal -c "Deploy complete" -g "ops: deploy v2.1"
CLAWMARK_STATION=/shared/team.db clawmark tune "deploy"
```

## Commands

```bash
# Search your memory (semantic by default)
clawmark tune "auth token refresh"
clawmark tune "what did I work on last week"

# Keyword fallback
clawmark tune --keyword "auth"

# Recent signals
clawmark tune --recent

# Full content (not just gist)
clawmark tune --full "auth"

# Discover something random
clawmark tune --random

# Save what you learned
clawmark signal -c "Fixed auth token refresh order in middleware" -g "fix: auth token refresh"

# Thread to an existing signal
clawmark signal -c "Follow-up: also fixed staging" -g "fix: staging auth" -p A1B2C3D4

# Long content from file
clawmark signal -c @session-notes.md -g "session: March 19 review"

# Station status
clawmark status

# JSON output
clawmark tune "auth" --json
```

## When to signal

- After solving something hard — don't make your next session figure it out again
- Before compaction — your context window is temporary, your station is permanent
- When you discover something — small facts save big time later

## When to tune

- Starting a new task — you may have already solved part of it
- When stuck — the answer might be in a signal from a previous session
- After compaction — recover context from your station

## The gist matters most

The gist is how your future self finds this signal. Write for them.

**Alive:** `"fix: auth token refresh ran before validation — swapped order in middleware"`
**Dead:** `"fixed a bug"`

## How it works

Your signals live in a SQLite database at `~/.clawmark/station.db`. Semantic search uses a local BERT model (no API calls, no cloud, runs offline). The model downloads once (~118MB) on first search.

This replaces `memory_search` with something that actually finds what you're looking for — by meaning, not just keywords.

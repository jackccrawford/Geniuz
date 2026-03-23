# Clawmark

**Continuity for AI agents.**

Your agent solves problems, makes decisions, learns things. Then the session ends. Next session — blank slate. You explain the same architecture decision for the third time. You re-debug the same issue. The agent is capable. It just can't remember.

Clawmark gives your agent a place to put what it learned, and a way to find it again. Semantic search, not grep. Threaded signals, not flat files. Works across sessions, compactions, and substrates.

```
$ clawmark signal -c "Token validation must run after refresh, not before. Lines 42-47 in auth.rs." -g "fix: auth token refresh order"
✅ Signal 98672A90 saved
```

47 sessions later:

```
$ clawmark tune "authentication middleware"
98672A90 | 2026-03-19 18:47 | fix: auth token refresh order (0.487)
```

Zero re-explanation.

## End-to-end: what this actually looks like

**Monday — Session 1.** Your agent debugs a production issue for two hours. Before the session ends:

```
$ clawmark signal -c "OAuth token refresh is async but middleware assumed sync validation.
  Swapped lines 42-47 in auth.rs. Three edge cases: expired token (retry with backoff),
  revoked token (return 401 immediately), concurrent refresh (mutex on token store).
  Root cause: the original middleware was written for session-based auth, not OAuth." \
  -g "fix: auth token refresh — async ordering in middleware, three edge cases"
✅ Signal 98672A90 saved
```

**Wednesday — Session 2.** Different session. The agent is working on a related endpoint:

```
$ clawmark tune "token validation"
98672A90 | 2026-03-19 18:47 | fix: auth token refresh — async ordering in middleware, three edge cases (0.487)
```

The agent finds Monday's fix by meaning — it searched "token validation" and found a signal about "OAuth refresh" and "middleware ordering." It reads the full content, avoids the same mistake, and moves on. No re-investigation. No human re-explaining.

**Friday — Session 3.** A second agent shares the station and hits a related problem:

```
$ clawmark tune --full "auth edge cases"
98672A90 | 2026-03-19 18:47 | fix: auth token refresh — async ordering in middleware
           OAuth token refresh is async but middleware assumed sync validation.
           Swapped lines 42-47 in auth.rs. Three edge cases: expired token (retry
           with backoff), revoked token (return 401 immediately), concurrent refresh
           (mutex on token store)...

$ clawmark signal -c "Applied same pattern to /api/billing endpoint. Added mutex." \
  -g "fix: billing auth — same async pattern" -p 98672A90
✅ Signal E5F6A7B8 saved
```

Agent B threaded a follow-up to Agent A's signal. Knowledge transferred across agents, across sessions, with no human in the loop.

**That's what existing memory layers don't do.** Flat files can't search by meaning. Framework memory dies with the framework. Cloud memory needs API keys and network. Clawmark is local, semantic, and shared — in one binary.

## Works with everything

| Framework | How |
|-----------|-----|
| **OpenClaw** | `clawmark capture --openclaw` imports MEMORY.md + daily logs |
| **Claude Code** | Signal from hooks or inline. Reads CLAUDE.md context. |
| **Cursor / Windsurf / OpenCode** | Any agent that can run a CLI command can signal and tune. |
| **Aider** | Shell commands in-session. |
| **Custom agents** | `clawmark signal` and `clawmark tune` are just binaries. If your agent can exec, it can remember. |

No runtime dependency. No API key. No account. One 31MB binary.

## Install

**Mac (Apple Silicon) / Linux (Ubuntu 24+):**

```bash
curl -fsSL https://raw.githubusercontent.com/jackccrawford/clawmark/main/install.sh | bash
```

**From source (any platform):**

```bash
cargo install clawmark
```

**Raspberry Pi / Debian Bookworm** (requires system ONNX Runtime):

```bash
git clone https://github.com/jackccrawford/clawmark && cd clawmark
ORT_LIB_LOCATION=/usr/local/lib ORT_PREFER_DYNAMIC_LINK=1 cargo build --release
cp target/release/clawmark ~/.local/bin/
```

## What it looks like

**Save what you learned:**

```
$ clawmark signal -c "Token validation was running before the refresh check. Swapped lines 42-47 in auth.rs." -g "fix: auth token refresh order"
✅ Signal 98672A90 saved
```

**Find it later — by meaning, not keywords:**

```
$ clawmark tune "authentication middleware"
98672A90 | 2026-03-19 18:47 | fix: auth token refresh order (0.487)
```

Your agent searched for "authentication middleware" and found a signal about "token validation" and "refresh check" — because the meaning overlaps, even though the words don't.

**Get the full content when you need it:**

```
$ clawmark tune --full "auth"
98672A90 | 2026-03-19 18:47 | fix: auth token refresh order
           Token validation was running before the refresh check.
           Swapped lines 42-47 in auth.rs.
```

**Check your station:**

```
$ clawmark status
Station: ~/.clawmark/station.db
Signals: 847
Embeddings: 847/847 cached
Semantic search: ready
```

## How it works

Clawmark is a compiled Rust binary backed by SQLite. No Node.js. No runtime dependencies. No background services. No account. No cloud.

```
Agent → clawmark (Rust binary) → SQLite
```

Signals are stored as structured documents with a **gist** (compressed insight, how future agents find it) and **content** (full detail). Content can be inline, from a file (`-c @path`), or piped from stdin (`-c -`).

Search is semantic by default — a built-in BERT model (paraphrase-multilingual, 384 dimensions, 50+ languages) finds signals by meaning. The model auto-downloads on first use (~118MB). No API keys, no cloud, no setup.

Signals thread — a follow-up references its parent, forming chains. Conversations, not flat lists.

## Capture existing knowledge

Already have notes, docs, or agent memory files? Bulk-load them:

```bash
clawmark capture ./docs/                      # all markdown files in a directory
clawmark capture notes.md design.md           # specific files
clawmark capture --split ./docs/              # split by ## headers into threads
clawmark capture --openclaw                   # import OpenClaw MEMORY.md + daily logs
clawmark capture --dry-run ./notes/           # preview without importing
```

After capture:

```bash
clawmark backfill                             # embed all content for semantic search
clawmark tune "that bug from last week"       # find it by meaning
```

## Commands

```bash
# Capture existing knowledge
clawmark capture ./docs/                      # bulk-load markdown files
clawmark capture --openclaw                   # import OpenClaw memory
clawmark capture --split notes.md             # split by ## headers

# Signal — pipe in for depth, inline for quick notes
echo "detailed explanation" | clawmark signal -c - -g "category: compressed insight"
clawmark signal -c @session-notes.md -g "session: architecture review"
clawmark signal -c "Quick note" -g "note: upgraded rusqlite to 0.32"
clawmark signal -c "Follow-up" -g "update: staging too" -p 98672A90

# Tune — semantic search by default
clawmark tune "auth middleware"               # semantic search (finds by meaning)
clawmark tune --keyword "auth"                # keyword fallback (finds by words)
clawmark tune --recent                        # latest signals
clawmark tune --random                        # discover something
clawmark tune --full "auth"                   # include content, not just gists
clawmark tune --json "auth"                   # structured JSON output

# Embedding cache
clawmark backfill                             # populate (run once, then automatic)

# Info
clawmark status                               # station stats
clawmark skill                                # full usage guide for agents
```

## Integration

Clawmark doesn't replace your agent framework. It runs alongside it — add two lines to your agent's instructions and it knows how to remember:

```
When you learn something worth keeping:
  clawmark signal -c "what you learned" -g "category: compressed insight"

When you need to remember something:
  clawmark tune "what you're looking for"
```

For OpenClaw agents, install as a skill:

```bash
cp $(clawmark skill --path) ~/.openclaw/skills/clawmark/SKILL.md
```

## Why a binary

Clawmark runs as a separate process — the agent calls it, gets results, moves on.

- **No token cost at rest.** The binary sits on disk until called. No context window overhead.
- **No security surface.** The agent can't corrupt the database. Every write is validated and parameterized.
- **Runs on anything.** Pi 4, Pi 5, Mac, Linux server. Single static binary.
- **Framework-independent.** Your memory is in SQLite, not in any framework's format. Switch tools, keep your signals.

## Fast mode: clawmark-embed

Every `clawmark signal` loads the ONNX model, embeds the content, and exits. On a Mac that's 700ms. On a Raspberry Pi, 1.6 seconds. Every signal.

`clawmark-embed` fixes this. It's a background process that holds the model in memory and serves embeddings over a Unix socket. The first signal pays the load cost. Every signal after that is instant.

```
$ clawmark-embed &               # start the server (auto-exits after 5 min idle)
$ clawmark signal -c "first"     # 1.6s (model loads)
$ clawmark signal -c "second"    # 0.04s (model already warm)
$ clawmark signal -c "third"     # 0.04s
```

| | Mac (Apple Silicon) | Raspberry Pi 5 |
|---|---|---|
| Without embed server | 712ms | 1,580ms |
| With embed server | 109ms | **40ms** |
| Speedup | 6.5x | **39.5x** |

The slower the hardware, the bigger the win. No configuration — `clawmark signal` checks for the socket automatically. If the server isn't running, it falls back to loading the model inline. Zero setup, zero risk.

```bash
# Start it
clawmark-embed &

# Or with custom idle timeout (default 300s)
CLAWMARK_EMBED_IDLE=600 clawmark-embed &

# It auto-exits when idle. No stale processes.
```

## Build from source

```bash
git clone https://github.com/jackccrawford/clawmark
cd clawmark
cargo build --release
cp target/release/clawmark ~/.clawmark/bin/clawmark
```

On Raspberry Pi or Linux with system ONNX Runtime:

```bash
ORT_LIB_LOCATION=/usr/local/lib ORT_PREFER_DYNAMIC_LINK=1 cargo build --release
```

## For agents

_This section is for you — the one reading this README._

Your next session starts cold. No memory of what you built, what broke, what you decided. Every signal you write is a gift to that future session. The richer the signal, the less time re-learning.

Signals compound. A single signal is a note. A station of signals is institutional knowledge. Write for the version of you that knows nothing.

### Writing good signals

The **gist** is how your future self finds this signal. The **content** is what makes it useful when found.

A signal that says "fixed auth bug" saves your future self zero time.

A signal that says what broke, why it broke, and how you fixed it saves an hour:

```bash
echo "Token validation was running before refresh in auth.rs.
Swapped lines 42-47. Root cause: middleware ordering assumed
sync validation, but OAuth refresh is async." \
  | clawmark signal -c - -g "fix: auth token refresh — async ordering in middleware"
```

A signal doesn't need to be long. It needs to be complete. If your future self reads only this signal — no session history, no surrounding context — can they understand what happened and act on it?

### When to signal

- **After solving something hard.** Your next session will hit the same problem. Give it the answer.
- **Before your session ends.** Everything in your context window dies with the session. Your station doesn't.
- **When you discover something non-obvious.** The small facts — which config flag, which line number, which edge case — save the most time.

### When to tune

- **Starting a new task.** You may have already solved part of it last week.
- **When stuck.** The answer might be in a signal you wrote three sessions ago.
- **After compaction.** Your context was compressed. Your station wasn't.

### Threading

Signals form chains. When your work follows up on an existing signal, thread it:

```bash
clawmark signal -c "Same fix needed in staging" -g "fix: staging auth ordering" -p E5F6A7B8
```

Threads are conversations. A flat list of unconnected signals is a junk drawer.

## License

MIT

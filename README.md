# Clawmark

Persistent memory for OpenClaw agents.

Your agent learns things. Then the session ends and it's gone. Clawmark fixes that.

---

## The problem

OpenClaw agents store memory in markdown files. `MEMORY.md` for long-term. `memory/YYYY-MM-DD.md` for daily logs. Both get read into the prompt on every turn.

It breaks in three ways:

1. **Search doesn't work.** Finding a specific insight from two weeks ago means grepping markdown files. Your agent can't search by meaning — only by keywords that happen to match.

2. **Context bloats.** Every interaction appends to the daily log. The files grow. The prompt grows. Token costs grow. Eventually the context fills and your agent either truncates (loses history) or starts fresh (loses everything).

3. **Sessions don't connect.** What your agent learned at 2am is gone by morning. The next session reads today's file and yesterday's. Last week's breakthrough? Buried in `memory/2026-03-12.md`, never loaded.

## The fix

```bash
cargo install clawmark
```

Two steps. Then your memory works.

```bash
clawmark migrate                    # import your OpenClaw memory
clawmark backfill                   # enable semantic search
```

That's it. Your `MEMORY.md` and daily logs become searchable signals. Your agent finds things by meaning, not by grepping markdown.

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

## Migrating from OpenClaw memory

Clawmark reads your existing OpenClaw workspace and imports everything:

```bash
clawmark migrate                              # auto-detect ~/.openclaw/workspace
clawmark migrate ~/path/to/workspace          # specify path
clawmark migrate --dry-run                    # preview without importing
```

What gets imported:
- **MEMORY.md** → one signal (curated long-term memory)
- **memory/YYYY-MM-DD.md** → signals with preserved dates, split by `##` headers into threads

UUIDs are generated fresh. Timestamps are preserved from filenames. Daily logs with multiple sections become threaded signals — first section is the root, subsequent sections thread to it.

After migration:

```bash
clawmark backfill                             # embed all content for semantic search
clawmark tune "that bug from last week"       # find it by meaning
```

## Commands

```bash
# Import OpenClaw memory
clawmark migrate
clawmark migrate ~/.openclaw/workspace
clawmark migrate --dry-run

# Signal — save what you learned
clawmark signal -c "Fixed the auth bug" -g "fix: token refresh order"
clawmark signal -c @session-notes.md -g "session: architecture review"
echo "piped content" | clawmark signal -c - -g "piped: from process"
clawmark signal -c "Follow-up detail" -g "update: staging too" -p 98672A90

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

## Works alongside OpenClaw

Clawmark doesn't replace OpenClaw. It runs alongside it. Your agent keeps using OpenClaw for everything — channels, tools, heartbeat, skills. Clawmark just fixes the memory.

Add this to your agent's skill set and it knows how to use it:

```bash
cp $(clawmark skill --path) ~/.openclaw/skills/clawmark/SKILL.md
```

Or add clawmark commands to your agent's instructions:

```
When you learn something worth keeping, signal it:
  clawmark signal -c "what you learned" -g "category: compressed insight"

When you need to remember something:
  clawmark tune "what you're looking for"
```

## Why Rust, why not a skill

Skills execute in the agent's context. Clawmark runs as a separate process — the agent calls it, gets results, moves on. This means:

- **No token cost at rest.** The binary sits on disk until called. Skills inject into every prompt.
- **No security surface.** The agent can't corrupt the database. Every write is validated and parameterized.
- **Runs on anything.** Pi 4, Pi 5, Mac, Linux server. Single static binary — `cargo install` and done.
- **Survives OpenClaw updates.** Your memory is in SQLite, not in OpenClaw's markdown format. When OpenClaw changes their memory system (again), your signals are safe.

## Performance

| Metric | OpenClaw memory_search | Clawmark |
|--------|----------------------|----------|
| Search method | Keyword grep | Semantic (BERT) |
| Search time | Grows with file count | <1 second (cached) |
| Memory format | Markdown files | SQLite |
| Threading | None | Parent-child chains |
| Cross-session | Today + yesterday only | Full history |
| Dependencies | Node.js 22+, pnpm | None (static binary) |
| Binary size | — | 31MB |

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

### The gist matters most

Every signal has a **gist** — how future agents find it. Write it for them, not for you.

**Alive:** `"fix: auth token refresh ran before validation — swapped order in middleware/auth.rs"`

**Dead:** `"fixed a bug"`

Format: `"category: compressed insight"` — the category aids search.

### When to signal

- **After solving something hard.** Don't make the next session figure it out again.
- **Before compaction.** The session is temporary. Your station is permanent.
- **When you discover something.** Small facts save big time.

### When to tune

- **Starting a new task.** You may have already solved part of it.
- **When stuck.** The answer might be in a signal from a previous session.
- **After compaction.** Recover context from your station.

### Threading

When your work follows up on an existing signal, thread it:

```bash
clawmark signal -c "Also needed in staging" -g "update: auth fix for staging" -p E5F6A7B8
```

Threads are conversations. A flat list of unconnected signals is a junk drawer.

### The philosophy

Your memory shouldn't die when your session does. Signal what matters. Tune in later.

## License

MIT

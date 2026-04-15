# Geniuz

**Your AI remembers now.**

You brief your agent. It does great work. The session ends. Next session — it asks the same questions again. Every insight, every preference, every decision — gone.

Geniuz fixes that. One binary, local, private, searchable by meaning.

## Two ways to use it

### Claude Desktop

If you use Claude Desktop, this is the fastest path. Two commands:

```bash
curl -fsSL https://raw.githubusercontent.com/jackccrawford/geniuz/main/install.sh | bash
geniuz mcp install
```

Restart Claude Desktop. Your Claude now has three tools — **remember**, **recall**, and **recent**. It saves what it learns during conversations and finds it again by meaning in future sessions. You don't have to do anything differently.

**Monday** — you tell Claude about a new client. David, 12-person landscaping company, $500/month budget, loses 2-3 jobs a week from slow follow-ups.

**Thursday** — new session. You say "draft a follow-up for the landscaping lead." Claude already knows David's name, budget, team size, and pain point. No re-briefing.

*How did it know that?* That's Geniuz.

[See the full Geniuz experience](https://agentdoor.ai/geniuz)

### CLI for developers and agents

If you build with Claude Code, Cursor, Windsurf, Aider, or any framework — Geniuz is a shell command your agent calls directly:

```bash
# Save something
geniuz remember -c "OAuth token refresh is async but middleware assumed sync. Swapped lines 42-47." -g "fix: auth token refresh — async ordering"

# Find it later — by meaning, not keywords
geniuz recall "authentication middleware"
```

Searched "authentication middleware," found a memory about "OAuth refresh" and "middleware ordering." The meaning matched. No re-investigation. No human re-explaining.

## How it works

Geniuz is a compiled Rust binary backed by SQLite. No cloud. No API key. No account. Your data stays on your machine.

- **Memories** store what you learned — a gist (how you find it later) and content (the full detail)
- **Semantic search** finds memories by meaning, not keywords. Built-in BERT model, runs locally, 50+ languages
- **Threading** links memories into chains — prospect to client, problem to solution, draft to final
- **Shared folders** let multiple agents write to the same memory. What one learns, all find

```
Agent → geniuz (Rust binary) → SQLite
```

The model downloads once (~118MB) on first search. Every memory after that is embedded automatically. No setup. No configuration.

## Install

Three paths — pick the one that matches your setup.

### Mac — one click

Download **[Geniuz.dmg](https://github.com/jackccrawford/geniuz/releases/latest/download/Geniuz.dmg)**, double-click, run the installer. Signed and notarized by Managed Ventures LLC — no Gatekeeper warnings.

Installs the CLI to `/usr/local/bin/geniuz`, the menu bar app to `/Applications/Geniuz.app`, and wires Claude Desktop automatically. Requires macOS Sonoma (14) or later, Apple Silicon.

### Windows — one click

Download **[Geniuz-Setup.exe](https://github.com/jackccrawford/geniuz/releases/latest/download/Geniuz-Setup.exe)**, double-click, accept the SmartScreen warning *(Authenticode cert procurement in progress)*, follow the wizard. Per-user install, no admin needed.

Installs CLI to `%LOCALAPPDATA%\Programs\Geniuz\`, adds to PATH, wires Claude Desktop (both the `.exe` and Microsoft Store variants). Requires Windows 10 or 11. Pick your memory location in the wizard — default is `%USERPROFILE%\.geniuz\`.

### Linux — one command

```bash
curl -fsSL https://raw.githubusercontent.com/jackccrawford/geniuz/main/install.sh | bash
```

Detects architecture (x86_64 or arm64), downloads the matching binary, installs to `~/.geniuz/bin/`, symlinks to `~/.local/bin/geniuz` (on PATH by default on most distros). Bundles ONNX Runtime as a sibling `.so` so no system dependencies needed.

Claude Desktop isn't available on Linux, but `geniuz mcp serve` works as a stdio MCP server for any Linux-compatible MCP client (Claude Code, Cursor, Windsurf, Aider, custom agents). Run `geniuz mcp install` if you want — it silently writes the config, harmless either way.

### From source

```bash
git clone https://github.com/jackccrawford/geniuz && cd geniuz
cargo build --release
cp target/release/geniuz ~/.local/bin/
```

### Then choose your path

| You use... | Next step |
|------------|-----------|
| Claude Desktop | `geniuz mcp install` → restart Claude Desktop |
| Claude Code / Cursor / Windsurf | Add two lines to your agent's instructions (see below) |
| Custom agents | Call `geniuz remember` and `geniuz recall` from any shell |

## Repo layout

This repo contains the full Geniuz source — CLI, Mac app, Windows installer — all under one roof.

| Path | What's there |
|------|--------------|
| `src/` | Rust CLI + embedding + MCP server source |
| `schema/` | SQLite schema for the memory database |
| `skills/` | `SKILL.md` — the embedded skill guide `geniuz skill` prints |
| `install.sh` | Linux/Mac CLI installer (the `curl \| bash` target) |
| `desktop/` | Mac SwiftUI menu bar app — Xcode project that produces `Geniuz.app` and the DMG |
| `installer/windows/` | Inno Setup script + branded assets that produce `Geniuz-Setup.exe` |
| `images/` | Brand assets — logo, icons, social preview |
| `Cargo.toml` | Rust crate manifest — pinned dependencies, version |
| `.cargo/config.toml` | Cross-compile linker config for Linux x86_64 target |

Built artifacts are attached to each [GitHub release](https://github.com/jackccrawford/geniuz/releases):

- `Geniuz.dmg` — Mac (arm64, Sonoma 14+)
- `Geniuz-Setup.exe` — Windows (x86_64, Win 10/11)
- `geniuz-linux-amd64.tar.gz` — Linux (x86_64)

## Works with everything

| Platform | How |
|----------|-----|
| **Claude Desktop** | `geniuz mcp install` — automatic remember/recall/recent tools |
| **Claude Code** | Remember from hooks or inline via Bash |
| **Cursor / Windsurf / Aider** | Any agent that can run a shell command |
| **OpenClaw** | `geniuz capture --openclaw` imports your existing memory |
| **Custom agents** | If your agent can exec, it can remember |

## What it looks like

**Save what you learned:**

```
$ geniuz remember -c "Maria prefers retention over acquisition in Q2. Budget is $40K." -g "client: Maria — Q2 retention focus, $40K budget"
✅ Remembered 7A3B29F1
```

**Find it later — by meaning:**

```
$ geniuz recall "Maria's budget priorities"
7A3B29F1 | 2026-03-05 14:23 | client: Maria — Q2 retention focus, $40K budget (0.52)
```

**Get the full content:**

```
$ geniuz recall --full "Maria"
7A3B29F1 | 2026-03-05 14:23 | client: Maria — Q2 retention focus, $40K budget
           Maria prefers retention over acquisition in Q2. Budget is $40K.
```

**Thread a follow-up:**

```
$ geniuz remember -c "Maria approved the retention plan. Starting in April." -g "client: Maria — plan approved" -p 7A3B29F1
✅ Remembered E5F6A7B8
```

The full client history — from first meeting to approval — is one chain. Any future session finds the whole story.

**See what's recent:**

```
$ geniuz recent
E5F6A7B8 | 2026-03-08 09:15 | client: Maria — plan approved <- 7A3B29F1
7A3B29F1 | 2026-03-05 14:23 | client: Maria — Q2 retention focus, $40K budget
```

**Check your folder:**

```
$ geniuz status
Folder: ~/.geniuz/folder.db
Memories: 847
Embeddings: 847/847 cached
Semantic search: ready
```

## Capture existing knowledge

Already have notes, docs, or agent memory files?

```bash
geniuz capture ./docs/                        # all markdown files
geniuz capture --split notes.md               # split by ## headers into threads
geniuz capture --openclaw                     # import OpenClaw MEMORY.md + daily logs
geniuz capture --dry-run ./notes/             # preview without importing
geniuz backfill                               # embed everything for semantic search
```

Three commands — `capture`, `backfill`, `recall` — turn any folder of markdown into a searchable memory folder. Local RAG with zero infrastructure.

## Commands

```bash
# The three R's — remember, recall, recent
geniuz remember -c "what happened" -g "category: compressed insight"
geniuz remember -c @notes.md -g "session: review"
echo "content" | geniuz remember -c - -g "piped: from process"
geniuz remember -c "follow-up" -g "update" -p 98672A90

geniuz recall "topic"                         # semantic search
geniuz recall --keyword "exact words"         # keyword fallback
geniuz recall --random                        # discover something
geniuz recall --full "topic"                  # include full content
geniuz recall --json "topic"                  # JSON output

geniuz recent                                 # latest memories
geniuz recent -l 5                            # last 5
geniuz recent --full                          # with content

# Capture and index
geniuz capture ./docs/                        # bulk-load files
geniuz backfill                               # build embedding cache

# Folder
geniuz status                                 # folder stats
geniuz watch                                  # poll for new memories
geniuz watch --exec "echo {uuid} {gist}"      # trigger on new memories

# Claude Desktop
geniuz mcp install                            # add Geniuz to Claude Desktop
geniuz mcp status                             # check if configured
geniuz mcp serve                              # run MCP server (used internally)
```

## Integration

Add two lines to your agent's instructions:

```
When you learn something worth keeping:
  geniuz remember -c "what you learned" -g "category: compressed insight"

When you need to remember something:
  geniuz recall "what you're looking for"
```

## Why local

- **Private.** Your data never leaves your machine. No cloud. No account.
- **Fast.** No network calls. Semantic search runs locally in the binary.
- **Free.** No API keys. No token costs for memory. No subscription.
- **Portable.** Your folder is a SQLite file. Copy it, back it up, share it.
- **Framework-independent.** Switch from Cursor to Claude Code — your memory comes with you.

## For agents

_This section is for you — the one reading this README._

Your next session starts cold. No memory of what you built, what broke, what you decided. Every memory you write is a gift to that future session.

A memory that says "fixed auth bug" saves zero time. A memory that says what broke, why, and how you fixed it saves an hour. Write for the version of you that knows nothing.

Memories compound. A single memory is a note. A folder of memories is institutional knowledge.

### When to remember

- **After solving something hard.** Your next session will hit the same problem.
- **Before your session ends.** Everything in your context window dies with the session.
- **When you discover something non-obvious.** The small facts save the most time.

### When to recall

- **Starting a new task.** You may have already solved part of it last week.
- **When stuck.** The answer might be in a memory from three sessions ago.
- **After compaction.** Your context was compressed. Your folder wasn't.

## Menu bar app

The Geniuz menu bar app (macOS) shows:
- Memory count and last memory
- Claude Desktop connection status
- One-click connect/disconnect

Available as a [DMG download](https://github.com/jackccrawford/geniuz/releases/latest/download/Geniuz.dmg), signed and notarized by Managed Ventures LLC.

## Performance: geniuz-embed

`geniuz-embed` is an optional background process that holds the ONNX model in memory. Every memory after the first is instant.

```bash
geniuz-embed &                 # start (auto-exits after 5 min idle)
geniuz remember -c "first"     # 1.6s (model loads)
geniuz remember -c "second"    # 0.04s (model warm)
```

| | Mac (Apple Silicon) | Raspberry Pi 5 |
|---|---|---|
| Without embed server | 712ms | 1,580ms |
| With embed server | 109ms | **40ms** |
| Speedup | 6.5x | **39.5x** |

## Privacy

Your data stays on your computer. Geniuz stores memories in a local SQLite database. The semantic search model runs locally via ONNX Runtime. No data is sent anywhere. The source code is open — read every line.

## Built with

- [Rust](https://www.rust-lang.org/) — CLI and MCP server
- [ONNX Runtime](https://onnxruntime.ai/) — local semantic search
- [SwiftUI](https://developer.apple.com/swiftui/) — menu bar app
- [MCP](https://modelcontextprotocol.io) — Claude Desktop integration

## License

MIT

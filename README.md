<p align="center">
  <img src="images/brand-hero.png" alt="Geniuz · the Memory Wall" width="180" />
</p>

<h1 align="center">Geniuz</h1>

<p align="center"><em>Persistent memory for AI agents. Runs on your machine, MIT-licensed, free.</em></p>

<p align="center">
  <a href="https://geniuz.life">geniuz.life</a> ·
  <a href="https://github.com/jackccrawford/Geniuz/releases/latest">Download</a> ·
  <a href="https://github.com/mvara-ai">Built by mVara</a>
</p>

<p align="center">
  <img src="images/mac-installer.png" alt="The Geniuz macOS installer: drag Geniuz to Applications and start remembering." width="640" />
</p>

---

Your AI is sharp. It also starts every conversation from zero. The call you made by feel, the standard you set, the reason behind a decision: gone by morning, rebuilt from scratch tomorrow.

Geniuz fixes that. One install, local, private, searchable by meaning. The context compounds instead of resetting at every boundary. Use it from a dashboard, a terminal, a menubar, or directly from any AI agent.

![The Geniuz dashboard. Memory surface showing memories and threads, listed newest-first with category prefixes.](images/dashboard-memories.png)

> **New in 2.0:** Cross-platform Tauri dashboard, terminal UI (`geniuz tui`), Mac menubar+dashboard integration via `geniuz://` URL scheme. [See release notes →](https://github.com/jackccrawford/geniuz/releases/tag/v2.0.0)
>
> **Note:** Screenshots in this README are from a pre-2.0.5 build and will refresh with the next release. The current app uses the new Memory Wall brand mark across every surface.

**[Read the wiki](wiki/README.md)** for how Geniuz works, with a map of its parts and the reasons behind its design.

## Three surfaces, one memory

Geniuz is one Rust core with several ways to reach it. Pick what fits the moment.

### Dashboard

Browse memories, save new ones, search by meaning, see recent threads. Native vibrancy on macOS, system tray on Windows + Linux.

Surfaces: **Folders · Keep · Find · Archived · Status · Data & Export · Settings**.

### Terminal UI · `geniuz tui`

Same data, terminal-native. For developers and agents who live in a shell. Two-field compose for `/keep`, semantic search via `/find`, sort toggle via `/reorder`.

```
$ geniuz tui
```

![The Geniuz TUI showing the Recent view. Folder name, memory count, newest-first list with date, category prefix, and gist on each row. Slash commands across the footer.](images/tui-recent.png)

Drill into any memory with `/detail <prefix>` (or `/random` to surface one at random):

![The Geniuz TUI Detail view of a random memory. Header with UUID prefix, gist, timestamp, and the full markdown content rendered in the terminal.](images/tui-detail.png)

Refuses to launch from non-interactive callers (TTY guard) so agents don't accidentally lock up on the alternate-screen mode.

### Menubar (macOS) / System tray (Windows + Linux)

Ambient presence. Memory count, recent activity, one-click to the dashboard. The menubar app is dock-less on Mac (`LSUIElement`). It's a residence, not a window.

![The macOS menubar popover. Memory count, today's writes, thread count, most recent gist, Claude Desktop connection status, and a button to open the dashboard.](images/menubar.png)

### CLI for agents

Underneath everything, the `geniuz` command. Your agents call it from any shell, any framework.

```bash
geniuz keep -c "OAuth token refresh is async but middleware assumed sync. Swapped lines 42-47." -g "fix: auth token refresh — async ordering"
geniuz recall "authentication middleware"
```

Searched "authentication middleware," found a memory about "OAuth refresh" and "middleware ordering." The meaning matched. No re-investigation. No human re-explaining.

The full surface:

```bash
% geniuz --help
Start here: 'geniuz recall' to see what's in your folder.

GENIUZ: Your AI remembers now.

Persistent memory for AI agents. Keep what matters, recall it later.
Works with any agent framework — Claude Code, Cursor, Windsurf, Aider,
or anything that can run a shell command.

Usage: geniuz [OPTIONS] <COMMAND>

Commands:
  keep       Keep a memory — what you learned, decided, or discovered
  recall     Search your memories — semantic by default
  folder     See and manage your folders
  capture    Capture files or directories into a folder
  watch      Watch for new memories in real time
  backfill   Build embedding cache for semantic search
  skill      Show usage guide for agents
  status     Show stats for the folder in use
  tui        Launch the terminal UI for browsing your memories
  dashboard  Launch the graphical dashboard app
  mcp        MCP server for Claude Desktop — run, install, or check status
  settings   User settings — read or change preferences
  help       Print this message or the help of the given subcommand(s)

Options:
      --in <FOLDER>
          Use this folder for the command: a name or its 8-character ID

  -v, --version
          Show version information

  -h, --help
          Print help (see a summary with '-h')

Examples:
  geniuz tui                                 Browse memories in a terminal UI
  geniuz keep -c "Fixed the auth bug" -g "fix: token refresh"
  geniuz recall "auth"                       Semantic search
  geniuz recall                              Latest memories
  geniuz capture ./notes/                    Bulk-load markdown files
  geniuz backfill                            Build embedding cache

Folders: 'geniuz folder list' shows them, and --in <name> uses one for a command.
  Multiple agents can share a folder for shared memory.

Data: Kept in .geniuz in your home directory
  Override with GENIUZ_HOME to change where Geniuz keeps its data
  Override with GENIUZ_STATION for a specific memory.db file

Use "geniuz [command] --help" for more information.
```

---

## Why local

- **Private.** All your data is stored on your machine. No cloud. No account. No telemetry.
- **Fast.** Semantic search runs locally in the binary, with no round trip to anyone.
- **Free.** No API keys. No token costs for memory. No subscription.
- **Portable.** Each folder is a SQLite file you own. Move it with `geniuz folder move`, or back it up with Export.
- **Framework-independent.** Switch from Cursor to Claude Code. Your memory comes with you.

---

## Install

Pick the path that matches your setup.

### macOS · one click

Download **[Geniuz.dmg](https://github.com/jackccrawford/geniuz/releases/latest/download/Geniuz.dmg)**, double-click, drag to Applications. Signed and notarized by Managed Ventures LLC. No Gatekeeper warnings.

![The Geniuz DMG installer window. Drag the Geniuz icon onto the Applications shortcut to install.](images/dmg-installer.png)

One DMG installs three things: the **menubar app** (always-on), the **dashboard** (launched from the menubar's "Open Dashboard" or via `geniuz://`), and the **CLI** (bundled at `Geniuz.app/Contents/Resources/geniuz`).

Wire the CLI into your shell PATH if you want it on the command line:

```bash
sudo ln -sf /Applications/Geniuz.app/Contents/Resources/geniuz /usr/local/bin/geniuz
```


### Windows · one click

Download **[Geniuz-Setup.exe](https://github.com/jackccrawford/geniuz/releases/latest/download/Geniuz-Setup.exe)** Windows 11, per-user, no admin prompt. Signed via Azure Trusted Signing (Microsoft-rooted cert, no "unknown publisher" warning). For managed deployment it runs silently with `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART`.

![Geniuz on Windows 11. Claude Desktop with the Geniuz MCP server alongside the Dashboard, showing a memory written from Claude appearing live in the dashboard.](images/windows-dashboard.jpg)

After install, the dashboard runs as a system tray app. Left-click → window; right-click → menu.

*First-launch SmartScreen note: even with a valid signature, brand-new binaries can hit a "rarely downloaded" reputation gate. If you see "Windows protected your PC," click "More info" → "Run anyway." That's per-binary reputation, separate from cert trust.*

### Linux · one command (developer path)

```bash
curl -fsSL https://raw.githubusercontent.com/jackccrawford/geniuz/main/install.sh | bash
```

Detects your OS and architecture, downloads the matching CLI binary (with TUI built in), installs to `~/.geniuz/bin/`. Best for developers, fleet operators, and anyone using Claude Code, Cursor, Windsurf, Aider, or any agent framework that can run a shell command.

On Linux, if a graphical session is detected, the script also installs the dashboard package (`.deb` / `.rpm` / `.AppImage`, picked from your package manager) with one `sudo` prompt. Headless servers skip the dashboard step automatically. Opt out explicitly with `GENIUZ_NO_DASHBOARD=1`. On macOS, the script installs only the CLI. Use the DMG above for the dashboard + menubar.

### Linux platform notes

Supported architectures:

- **x86_64** (Ubuntu, Debian, Fedora, Arch; modern distros with glibc 2.34+)
- **arm64** (Raspberry Pi 5, Pi OS / Debian Bookworm+, NVIDIA Jetson, Ampere, AWS Graviton, Oracle Ampere)

The arm64 build bundles ONNX Runtime 1.22 as a sibling `.so` and wraps the CLI with an `LD_LIBRARY_PATH` script, so it runs cleanly on older-glibc systems like Pi OS Bookworm (glibc 2.36). The x86_64 build is a single static binary.

Claude Desktop isn't available on Linux, but `geniuz mcp serve` works as a stdio MCP server for any Linux-compatible MCP client (Claude Code, Cursor, Windsurf, Aider, custom agents). The dashboard ships as a `.deb` and `.AppImage` for desktop Linux; the TUI runs in any terminal.

### From source

```bash
git clone https://github.com/jackccrawford/geniuz && cd geniuz
cargo build --release --bin geniuz
cp target/release/geniuz ~/.local/bin/
```

To also build the dashboard locally:

```bash
cd desktop/dashboard
cargo tauri build
```

(Requires `tauri-cli`. Mac: nothing else. Linux: `libwebkit2gtk-4.1-dev build-essential libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`. Windows: WebView2 runtime, typically preinstalled on Win 10/11.)

### Then choose your path

| You use... | Next step |
|------------|-----------|
| Claude Desktop | `geniuz mcp install` → restart Claude Desktop |
| Claude Code / Cursor / Windsurf | Add two lines to your agent's instructions (see below) |
| Custom agents | Call `geniuz keep` and `geniuz recall` from any shell |
| Just want to see your memories | `geniuz tui` in any terminal, or open the dashboard from the menubar/tray |

---

## How it works

Geniuz is a compiled Rust binary backed by SQLite. No cloud. No API key. No account. Your data stays on your machine.

- **Memories** store what you learned: a gist (how you find it later) and content (the full detail)
- **Semantic search** finds memories by meaning, not keywords. Built-in BERT model, runs locally, 50+ languages
- **Threading** links memories into chains: prospect to client, problem to solution, draft to final
- **Folders** keep separate memories apart: a project, a knowledge base, personal notes. Multiple agents can share one, and what one learns, all find

```
   Dashboard ─┐
        TUI  ─┼─→ db::DatabaseManager ──→ memory.db (SQLite)
   Menubar  ─┤                                ↑
        CLI ─┘                                │
                                              ↓
                                       ONNX (BERT)
```

The model downloads once (~118MB), the first time anything is embedded. Every memory after that is embedded automatically. No setup. No configuration.

---

## Folders

Keep separate memories apart. Work and Home, one per project, a knowledge base, a place agents leave each other messages. Each folder is one directory holding one `memory.db`, and it carries its own name and description.

```bash
geniuz folder list                               # all of them; * is the one in use
geniuz folder create Work -d "Client projects. No passwords here."
geniuz keep --in Work -c "Acme prefers Friday reviews" -g "client: Acme"
geniuz recall --in Work "reviews"
geniuz recall --all "reviews"                    # every folder; each result shows its address
geniuz recall 13625F6B:3487B9B0                  # open that memory, whichever folder it's in
geniuz folder use Work                           # use it whenever none is named
```

Everything goes in your data directory unless you say otherwise. `geniuz folder create Notes --at ~/notes` makes one elsewhere, and `geniuz folder add <dir>` makes Geniuz look in a directory you choose. It never searches your disk, and it can only show what your file permissions let it read.

**Move a folder with `geniuz folder move`, never by copying it.** A copy of a live database can lose memories. And a folder can't be duplicated: its ID and the ID of every memory in it are permanent, so Geniuz refuses a second copy. For a backup, use Export in the dashboard.

`geniuz folder archive` hides a folder without deleting anything, and `geniuz folder restore` brings it back.

---

## Works with everything

| Platform | How |
|----------|-----|
| **Claude Desktop** | `geniuz mcp install` — automatic keep, recall and folders tools |
| **Claude Code** | Keep from hooks or inline via Bash; or use the TUI for browsing |
| **Cursor / Windsurf / Aider** | Any agent that can run a shell command |
| **OpenClaw** | `geniuz capture --openclaw` imports your existing memory |
| **Custom agents** | If your agent can exec, it can keep memories |
| **Just you** | Dashboard for browse/search/compose; TUI for the same in a terminal |

---

## What it looks like

<!-- SCREENSHOT_GALLERY: a 2x2 grid would be ideal here. Suggested:
     [Dashboard Memories]  [Dashboard Keep compose]
     [TUI recent list]     [TUI detail view]
-->

**Save what you learned (CLI):**

```
$ geniuz keep -c "Maria prefers retention over acquisition in Q2. Budget is $40K." -g "client: Maria — Q2 retention focus, $40K budget"
✅ Kept 7A3B29F1
```

**Find it later, by meaning:**

```
$ geniuz recall "Maria's budget priorities"
7A3B29F1 | 2026-03-05 14:23 | client: Maria — Q2 retention focus, $40K budget (0.52)
```

**Browse interactively:**

```
$ geniuz tui
```

Opens a terminal UI with `/recent`, `/keep`, `/find`, `/reorder`, `/random`, `/detail`, `/help`. Press `/` to start typing a command; `↑`/`↓` to navigate; `Enter` to open detail; `Ctrl-C` to exit.

**Thread a follow-up:**

```
$ geniuz keep -c "Maria approved the retention plan. Starting in April." -g "client: Maria — plan approved" -p 7A3B29F1
✅ Kept E5F6A7B8
```

The full client history, from first meeting to approval, is one chain. Any future session finds the whole story.

---

## Capture existing knowledge

Already have notes, docs, or agent memory files?

```bash
geniuz capture ./docs/                        # all markdown files
geniuz capture --split notes.md               # split by ## headers into threads
geniuz capture --openclaw                     # import OpenClaw MEMORY.md + daily logs
geniuz capture --dry-run ./notes/             # preview without importing
geniuz backfill                               # embed everything for semantic search
```

Three commands (`capture`, `backfill`, `recall`) turn any folder of markdown into a searchable memory folder. Local RAG with zero infrastructure.

---

## Commands

```bash
# Keep and recall
geniuz keep -c "what happened" -g "category: compressed insight"
geniuz keep -c @notes.md -g "session: review"
echo "content" | geniuz keep -c - -g "piped: from process"
geniuz keep -c "follow-up" -g "update" -p 98672A90

geniuz recall "topic"                         # semantic search
geniuz recall "topic" --since 7d              # only the past week
geniuz recall "reviews by Cubic"              # Capitalized names come first
geniuz recall --grep Cubic --grep PR          # matching lines, every term
geniuz recall --thread 98672A90               # a whole thread, root first
geniuz recall --keyword "exact words"         # keyword fallback
geniuz recall --random                        # discover something
geniuz recall --full "topic"                  # include full content
geniuz recall --json "topic"                  # JSON output

geniuz recall                                 # latest memories
geniuz recall -l 5                            # last 5
geniuz recall --full                          # with content

# Interactive
geniuz tui                                    # terminal UI (requires a TTY)

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

---

## Integration

Add two lines to your agent's instructions:

```
When you learn something worth keeping:
  geniuz keep -c "what you learned" -g "category: compressed insight"

When you need to remember something:
  geniuz recall "what you're looking for"
```

---

## Architecture

Geniuz exposes four user surfaces plus the MCP server for agents:

- **Dashboard** (Tauri) — for browse, search, compose, settings. Cross-platform.
- **TUI** (`geniuz tui`) — same for terminal users and agents that prefer ratatui over GUI.
- **Menubar / system tray** — always-on ambient presence. macOS menubar is rich (stats hero + Recent list + Open Dashboard); Windows + Linux tray is a menu.
- **CLI** — for scripting and direct agent integration. Every subcommand supports `--json` for procedural callers.
- **MCP** — `geniuz mcp serve` is the stdio MCP server. Used by Claude Desktop, Claude Code, and any MCP client.

All surfaces go through one library: `db::DatabaseManager`. Same write path, same read path, same invariants. The dashboard's "Keep" button and the CLI's `geniuz keep` end up at the same SQL `INSERT`.

**The schema is the idea, so borrow it.** One table: a UUID, a JSON payload, a parent, a time. Rows are added and never changed. Tailor it to your own work, and make something updatable only where a concrete need says so.

**Write through Geniuz, not the file.** Triggers keep memories immutable in the file itself, but an embedding for every memory and a folder's identity are kept by the library's write path. To read or write programmatically, use MCP or the CLI.

There is no HTTP server in Geniuz. Procedural software speaks MCP. This keeps the storage layer free to evolve and the invariants centralized in one place.

---

## Repo layout

This repo contains the full Geniuz source (CLI, TUI, dashboard, Mac menubar app, Windows installer) all under one roof.

| Path | What's there |
|------|--------------|
| `src/` | Rust CLI + TUI + embedding + MCP server source |
| `src/tui.rs` | Terminal UI (ratatui + crossterm) |
| `src/folders.rs` | Folders: finding them, moving them, refusing duplicates |
| `schema/` | SQLite schema for the memory database |
| `skills/` | `SKILL.md` — the embedded skill guide `geniuz skill` prints |
| `install.sh` | Linux/Mac CLI installer (the `curl \| bash` target) |
| `desktop/Geniuz/` | macOS menubar app (SwiftUI) — Xcode project |
| `desktop/dashboard/` | Tauri cross-platform dashboard — Rust + HTML/CSS/JS |
| `desktop/ship.sh` | Mac release pipeline (build → inject CLI + dashboard → sign → notarize → DMG) |
| `installer/windows/` | Inno Setup script + Trusted Signing scripts |
| `images/` | Brand assets — logo, icons, social preview |
| `Cargo.toml` | Rust crate manifest — pinned dependencies, version |

Built artifacts are attached to each [GitHub release](https://github.com/jackccrawford/geniuz/releases):

- `Geniuz.dmg` — Mac (arm64, Sonoma 14+) — menubar + dashboard + CLI
- `Geniuz-Setup.exe` — Windows installer (Inno Setup, x86_64, Windows 11) — CLI, MCP server and the dashboard with system tray.
- `geniuz-linux-amd64.tar.gz` — Linux x86_64 CLI + TUI
- `geniuz-linux-arm64.tar.gz` — Linux arm64 CLI + TUI (Pi 5 compatible)
- `Geniuz_<version>_amd64.deb` / `Geniuz-<version>-1.x86_64.rpm` / `Geniuz_<version>_amd64.AppImage` — Linux x86_64 dashboard
- Linux arm64 dashboard for Pi 5 is on the 2.0.5 queue (pending CI extension)

---

## For agents

_This section is for you, the one reading this README._

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

### Meaning finds, exact recalls

Search by meaning to discover; search exactly to recall. Every answer ends
with `names seen:` and `pointers:` (the next hops) and a `searched:` line
that says what was searched, so an empty answer carries its scope. When a
question has a time in it, add `--since 7d`. When you know the name, use
`--grep Name`. When you've found one piece of a conversation, read the rest
with `--thread ID`.

### When to use the TUI

`geniuz tui` only launches with an interactive terminal. Agents calling from a subprocess get a clean refusal with exit code 2. If you want browsing in code, use `geniuz recall`, or the MCP tools. They're shell-friendly.

---

## Performance: geniuz-embed

`geniuz-embed` is an optional background process that holds the ONNX model in memory. Every memory after the first is instant.

```bash
geniuz-embed &                 # start (auto-exits after 5 min idle)
geniuz keep -c "first"     # 1.6s (model loads)
geniuz keep -c "second"    # 0.04s (model warm)
```

| | Mac (Apple Silicon) | Raspberry Pi 5 |
|---|---|---|
| Without embed server | 712ms | 1,580ms |
| With embed server | 109ms | **40ms** |
| Speedup | 6.5x | **39.5x** |

---

## Privacy

Your data stays on your computer. Geniuz stores memories in a local SQLite database. The semantic search model runs locally via ONNX Runtime. Geniuz sends your memories nowhere, with one exception you control: if the built-in model fails to load and `GENIUZ_EMBED_URL` points at another machine, text goes there to be embedded. What you recall through an AI tool goes wherever that tool sends it. The source code is open. Read every line.

---

## Built with

- [Rust](https://www.rust-lang.org/) — CLI, TUI, MCP server, dashboard backend
- [ratatui](https://ratatui.rs/) + [crossterm](https://github.com/crossterm-rs/crossterm) — terminal UI
- [Tauri 2](https://tauri.app/) — cross-platform dashboard (window-vibrancy on macOS, system tray on Windows + Linux)
- [ONNX Runtime](https://onnxruntime.ai/) — local semantic search
- [SwiftUI](https://developer.apple.com/swiftui/) — macOS menubar app
- [MCP](https://modelcontextprotocol.io) — Claude Desktop integration
- [Azure Trusted Signing](https://learn.microsoft.com/en-us/azure/trusted-signing/) — Windows code signing

## Learn more

- **Website**: [geniuz.life](https://geniuz.life)
- **Built by**: [Jack C. Crawford](https://github.com/jackccrawford)
- **Discussions**: [GitHub Discussions](https://github.com/jackccrawford/Geniuz/discussions)
- **Issues**: [GitHub Issues](https://github.com/jackccrawford/Geniuz/issues)
- **License**: [MIT](LICENSE)

## License

MIT

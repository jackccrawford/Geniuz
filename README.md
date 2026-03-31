# Geniuz

**Your Claude remembers now.**

Geniuz gives Claude Desktop persistent memory. Preferences, decisions, project context — all searchable by meaning. Close Claude today, pick up where you left off tomorrow.

## Install

### Mac — one click

Download [Geniuz.dmg](https://github.com/jackccrawford/Geniuz/releases/latest/download/Geniuz.dmg), open it, run the installer. Signed and notarized by Managed Ventures LLC.

Restart Claude Desktop. Done.

### Linux / Mac CLI

```sh
curl -fsSL https://raw.githubusercontent.com/jackccrawford/clawmark/main/install.sh | sh
clawmark mcp install
```

Restart Claude Desktop.

## What it does

Geniuz connects to Claude Desktop through [MCP](https://modelcontextprotocol.io) (Model Context Protocol) and gives your Claude three tools:

| Tool | What it does |
|------|-------------|
| **remember** | Saves something worth keeping — a decision, a preference, a client detail. Happens naturally during conversation. |
| **recall** | Searches everything by meaning, not keywords. "What do I know about David?" finds landscaping notes even if you never used those words. |
| **recall_recent** | Shows the most recent memories. Perfect for picking up where you left off. |

## How it works

- **Semantic search** — built-in ONNX model (paraphrase-multilingual-MiniLM-L12-v2, 384-dim) finds memories by meaning
- **Local SQLite** — everything stored in `~/.geniuz/station.db` on your computer
- **No cloud, no account, no API keys** — nothing leaves your machine
- **MCP server** — Claude Desktop launches `geniuz mcp serve` as a stdio subprocess

## CLI

Geniuz also works from the command line:

```sh
geniuz signal -c "Client prefers email over phone" -g "preference: communication"
geniuz tune "client preferences"
geniuz tune --recent
geniuz status
geniuz mcp status
```

## Menu bar app

The Geniuz menu bar app (macOS) shows:
- Memory count and last signal
- Claude Desktop connection status
- One-click connect/disconnect

Available on the Mac App Store and as a DMG download.

## Privacy

Your data stays on your computer. Geniuz stores memories in a local SQLite database. The semantic search model runs locally via ONNX Runtime. No data is sent anywhere. The source code is open — read every line.

## Built with

- [Rust](https://www.rust-lang.org/) — CLI and MCP server
- [ONNX Runtime](https://onnxruntime.ai/) — local semantic search
- [SwiftUI](https://developer.apple.com/swiftui/) — menu bar app
- [MCP](https://modelcontextprotocol.io) — Claude Desktop integration

## License

MIT

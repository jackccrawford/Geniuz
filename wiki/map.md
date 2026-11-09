# Map

The parts of Geniuz and how they connect. There is no server: each part opens the same files, and everything stays on your computer.

## Surfaces

The ways in, for you and for your AI.

| Part | What it does | Guardrail | Source |
|---|---|---|---|
| CLI | Every command, for people, scripts and agents. | `capture` refuses files that are not prose unless you pass `--force`. | [`src/cli.rs`](../src/cli.rs), [`src/main.rs`](../src/main.rs) |
| Dashboard | The desktop app. Browse, keep and find across your folders, pin the ones you use most, and change your shortcuts. | A window stays with the folder it shows, so a draft can't land somewhere else. Looking at a folder doesn't change your default. | [`desktop/dashboard`](../desktop/dashboard) |
| MCP | How your AI reaches Geniuz: `keep`, `recall` and `folders`, plus guidance on when to use them. | A wrong folder name is an error, never a different folder. | [`src/mcp.rs`](../src/mcp.rs) |
| Menubar | Your folders and recent memories, always one click away, and Settings for updates. On Windows and Linux a tray icon opens the dashboard and its pages instead. | The Mac menubar only ever reads your memories. | [`desktop/Geniuz`](../desktop/Geniuz), [`desktop/dashboard`](../desktop/dashboard) |
| Terminal UI | Browse and compose in a terminal. | Refuses to start without a real terminal. | [`src/tui.rs`](../src/tui.rs) |

## Core

Where memories are kept and found.

| Part | What it does | Guardrail | Source |
|---|---|---|---|
| Embed helper | Keeps the model ready between calls. | Local socket only. Exits after five idle minutes. | [`src/embed_server.rs`](../src/embed_server.rs), [`src/embed_client.rs`](../src/embed_client.rs) |
| Embedding | Turns text into numbers that capture its meaning, with ONNX or Ollama. | Rejects stored embeddings of the wrong size when reading them. | [`src/embedding.rs`](../src/embedding.rs) |
| Folders | Finds, moves, pins and archives folders, and decides which one a command uses. | A duplicate is refused, and nothing is ever deleted. | [`src/folders/`](../src/folders/) |
| Library | Reads, writes and every kind of search. | An ambiguous ID prefix is an error, never a guess. | [`src/db.rs`](../src/db.rs), [`src/dig.rs`](../src/dig.rs), [`src/window.rs`](../src/window.rs) |
| Store | One SQLite file per folder. Tables: `memories`, `memory_embeddings`, `embedding_meta`, `_meta`. View: `memory_chains`. | Triggers refuse update, delete and replace on memories. | [`schema/schema.sql`](../schema/schema.sql) |

## Outside

Reached only when needed, and never with your memories by default.

| Part | What it does | Guardrail | Source |
|---|---|---|---|
| GitHub | Where the Mac menubar checks for a newer release. | Sends nothing from your folders. Daily, weekly or only when you ask. | [`desktop/Geniuz`](../desktop/Geniuz) |
| Hugging Face | Where the model is downloaded from. | Once, then local. | [`src/embedding.rs`](../src/embedding.rs) |
| Ollama | A second way to embed, by default on your own machine. | Used only if the built-in model fails to load. | [`src/embedding.rs`](../src/embedding.rs) |

## Connections

| From | To | How |
|---|---|---|
| CLI | Folders | finds the folder for each command |
| CLI | Library | all commands |
| Dashboard | Folders | lists, switches and pins |
| Dashboard | Library | app commands |
| Embedding | Embed helper | socket first |
| Embedding | Hugging Face | model download |
| Embedding | Ollama | as a fallback |
| Folders | Store | reads identity without changing anything |
| Library | Embedding | embeds text |
| Library | Store | transactional writes |
| MCP | Folders | finds the folder for each call |
| MCP | Library | tool calls |
| Menubar | CLI | lists the folders |
| Menubar | GitHub | update check |
| Menubar | Store | read-only SQL |
| Terminal UI | Library | browse and compose |

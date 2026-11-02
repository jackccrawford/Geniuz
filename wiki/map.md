# Map

The parts of Geniuz and how they connect. There is no server: each surface opens the same file.

## Surfaces

The ways in.

| Part | What it does | Guardrail | Source |
|---|---|---|---|
| CLI | Every command, for people, scripts and agents. | `capture` refuses files that are not prose unless you pass `--force`. | [`src/cli.rs`](../src/cli.rs), [`src/main.rs`](../src/main.rs) |
| MCP | A stdio server with three tools: `remember`, `recall`, `recall_recent`. | Falls back to keyword search if search by meaning fails or finds nothing. | [`src/mcp.rs`](../src/mcp.rs) |
| Terminal UI | Browse and compose in a terminal. | Refuses to start without a real terminal. | [`src/tui.rs`](../src/tui.rs) |
| Dashboard | The desktop app, for browsing, search and settings. | Opens and reveals files only under your home folder. | [`desktop/dashboard`](../desktop/dashboard) |
| Menubar | Always-on stats on a Mac, and a tray menu on Windows and Linux. | The Mac menubar opens the database read-only. | [`desktop/Geniuz`](../desktop/Geniuz), [`desktop/dashboard`](../desktop/dashboard) |

## Core

Where memories are kept and found.

| Part | What it does | Guardrail | Source |
|---|---|---|---|
| Library | Reads, writes and every kind of search. | An ambiguous ID prefix is an error, never a guess. | [`src/db.rs`](../src/db.rs), [`src/dig.rs`](../src/dig.rs), [`src/window.rs`](../src/window.rs) |
| Store | One SQLite file. Tables: `memories`, `memory_embeddings`, `embedding_meta`, `_meta`. View: `memory_chains`. | Triggers refuse update, delete and replace on memories. | [`schema/schema.sql`](../schema/schema.sql) |
| Embedding | Turns text into numbers, with ONNX or Ollama. | Rejects stored embeddings of the wrong size when reading them. | [`src/embedding.rs`](../src/embedding.rs) |
| Embed helper | Keeps the model ready between calls. | Local socket only. Exits after five idle minutes. | [`src/embed_server.rs`](../src/embed_server.rs), [`src/embed_client.rs`](../src/embed_client.rs) |

## Outside

Reached only when needed.

| Part | What it does | Guardrail | Source |
|---|---|---|---|
| Hugging Face | Where the model is downloaded from. | Once, then local. | [`src/embedding.rs`](../src/embedding.rs) |
| Ollama | A second way to embed, by default on your own machine. | Used only if the built-in model fails to load. | [`src/embedding.rs`](../src/embedding.rs) |

## Connections

| From | To | How |
|---|---|---|
| CLI | Library | all commands |
| MCP | Library | tool calls |
| Terminal UI | Library | browse and compose |
| Dashboard | Library | app commands |
| Menubar | Store | read-only SQL |
| Library | Store | transactional writes |
| Library | Embedding | embeds text |
| Embedding | Embed helper | socket first |
| Embedding | Hugging Face | model download |
| Embedding | Ollama | as a fallback |

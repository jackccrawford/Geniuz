# How it works

Geniuz is a Rust core and one SQLite file. Everything else is a way of reaching them.

## What a memory is

A memory has two parts: a **gist**, which is how you find it later, and **content**, which is the full detail. It can also have a **parent**, which links it to an earlier memory to form a thread.

Memories are stored in the `memories` table of [`schema/schema.sql`](../schema/schema.sql). Database triggers refuse any update, delete or replace, including from other tools that open the file. Changing the past takes a deliberate step: dropping the trigger.

## When you remember

1. The command reads your text, which can be inline, from a file (`-c @notes.md`) or piped in (`-c -`).
2. [`signal_with_backend`](../src/db.rs) refuses empty content and creates an ID. If you named a parent, it looks it up. An unknown parent does not stop the write: the memory is saved as the start of a new thread.
3. The text is turned into an embedding, a list of numbers that represents its meaning. Geniuz asks the small background helper first, then falls back to loading the model itself.
4. If embedding fails, the memory is still saved and can be found by keyword until it is embedded later.
5. The memory and its embedding are written in one transaction.

## When you recall

`geniuz recall "topic"` searches by meaning: it compares your query with stored embeddings and ranks the closest. A few details make that work better:

- **Names come first.** Capitalized names in a query are matched exactly and put at the top, because searching by meaning alone tends to miss them.
- **Memories not yet embedded still count.** They are found by keyword and given a share of the results.
- **Time windows.** `--since 7d` limits results to a period.

Other kinds of recall skip meaning entirely:

| Flag | What it does |
|---|---|
| `--grep Cubic --grep PR` | Memories holding every term, showing each line with any of them |
| `--thread 98672A90` | A whole thread, starting at its root |
| `--keyword "exact words"` | Keyword search |
| `--random` | Something you may have forgotten |

The reason is in [`src/dig.rs`](../src/dig.rs): "Semantic search is for discovery; exact search is for recall".

## The model

Search by meaning runs locally through ONNX Runtime, using a BERT sentence model that covers more than 50 languages. It downloads once from Hugging Face, about 118 MB, into `~/.geniuz/models/`, the first time anything is embedded. That includes your first `remember`. Loading a model is slow, so a helper called `geniuz-embed` keeps it ready over a local socket and exits after five idle minutes.

If the built-in model fails to load, Geniuz tries an embedding service instead: Ollama on your own machine by default. See [Using it](using-it.md#settings).

## Your data stays on your machine

When Geniuz runs, it stores all your data locally, in a file on your computer. There is no account, no cloud and no telemetry.

For completeness, Geniuz reaches the network in a few small ways, and none of them sends your memories:

- **Hugging Face:** to download the model, once.
- **GitHub:** when you run the install script or download a release.

Geniuz itself sends memory text to another address in only one case: if the built-in model fails to load and `GENIUZ_EMBED_URL` points at another machine, text goes there to be embedded. By default that address is your own machine. What you recall through an AI tool goes wherever that tool sends it.

# How it works

Your AI saves what matters as you work, and finds it again in the next conversation. Underneath, Geniuz is a Rust core and SQLite files, and everything else is a way of reaching them.

## How your AI knows to use it

When an AI tool connects, Geniuz tells it what to do: recall at the start of a conversation, and before finishing a task, keep what a future session would need and could not get from the files, such as decisions and why, people, preferences and constraints. This is part of the [MCP server](../src/mcp.rs). Tool descriptions alone weren't enough: in our tests, agents ignored them until they had this guidance.

## What a memory is

A memory has two parts: a **gist**, which is how you find it later, and **content**, which is the full detail. It can also have a **parent**, which links it to an earlier memory to form a thread.

Memories are stored in the `memories` table of [`schema/schema.sql`](../schema/schema.sql). Database triggers refuse any update, delete or replace, including from other tools that open the file. Changing the past takes a deliberate step: dropping the trigger.

## What a folder is

A folder is a directory holding one `memory.db`. Its identity is inside that file: a UUID, a name, a description and when it was made, in the `_meta` table of [`schema/schema.sql`](../schema/schema.sql). So a folder can be renamed, or moved anywhere, and is still the same folder. The first one is `~/.geniuz/memory.db`, called "My memory", and it stays where it is. Pinned folders come first in every list.

The same file serves many uses: a project, a standard, a knowledge base, personal notes, what an agent keeps between sessions, messages between agents. There are no folder types. The description says what a folder is for, and the prompt for writing one is "What should whoever comes here next know?"

Geniuz looks in two places: its data directory, and directories you added on purpose. It reads each one level down and never searches your disk. That decides what Geniuz shows. Your file permissions decide who can read, so a file it can't read is simply left out. The rules are in [`src/folders.rs`](../src/folders.rs).

A command uses the first of these that applies: `--in <name>`, the `GENIUZ_DB` file path, a `.geniuz-folder` file in this directory or a parent, the folder chosen with `geniuz folder use`, then "My memory". A name that matches nothing is an error, so a typo can't send a memory somewhere else. In the dashboard, Keep has a pulldown for the folder, and an open window stays with the folder it shows.

### One of each

A folder's UUID is permanent, and so is the UUID of every memory in it, and addresses like `13625F6B:3487B9B0` rely on both. So a folder can be moved but not copied. `folder add` refuses a directory holding a folder already here. If a copy turns up later, every copy is blocked until one is moved, because nothing can tell which is the original, and using the wrong one would split the history in two. Names are unique too, ignoring case.

`folder move` is the safe way to relocate one. SQLite keeps recent writes in a `-wal` file beside the database, so copying a live `memory.db` by hand can lose them. Moving flushes that file and holds off writers first. Across drives it also checks the new copy's integrity, identity and memory count, closes the original to further writes, then moves it to `archive/` for you to delete when you're satisfied. If a move across drives is interrupted, Geniuz finishes or undoes it the next time it runs, and waits if the original is out of reach and the copy can't be shown to be final. Links to folders aren't moved: move the folder itself.

`folder archive` moves a folder into `archive/` beside itself, one level below where Geniuz reads. Nothing is deleted, and `folder restore` puts it back.

## When you keep

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

Two more forms reach across folders. `recall --all "topic"` searches every folder and shows each result with its address. `recall 13625F6B:3487B9B0` opens that memory in full, whichever folder it is in. In the dashboard, Find can search any selection of folders, and an address in a memory's text can be clicked.

The reason is in [`src/dig.rs`](../src/dig.rs): "Semantic search is for discovery; exact search is for recall".

## The model

Search by meaning runs locally through ONNX Runtime, using a BERT sentence model that covers more than 50 languages. It downloads once from Hugging Face, about 130 MB, into `~/.geniuz/models/`, the first time anything is embedded. That includes your first `keep`. Loading a model is slow, so a helper called `geniuz-embed` keeps it ready over a local socket and exits after five idle minutes.

If the built-in model fails to load, Geniuz tries an embedding service instead: Ollama on your own machine by default. See [Using it](using-it.md#settings).

## Your data stays on your machine

When Geniuz runs, it stores all your data locally, in a file on your computer. There is no account, no cloud and no telemetry.

For completeness, Geniuz reaches the network in a few small ways, and none of them sends your memories:

- **Hugging Face:** to download the model, once.
- **GitHub:** when you run the install script or download a release.

Geniuz itself sends memory text to another address in only one case: if the built-in model fails to load and `GENIUZ_EMBED_URL` points at another machine, text goes there to be embedded. By default that address is your own machine. What you recall through an AI tool goes wherever that tool sends it.

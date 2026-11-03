# Glossary

| Term | Meaning |
|---|---|
| Memory | One entry that Geniuz will not change: a gist, content, and optionally a parent. |
| Gist | The short line you find a memory by. |
| Content | The full detail of a memory. |
| Thread | Memories linked by parent, such as a problem and its solution. |
| Parent | The earlier memory that a memory replies to. |
| Folder | A directory holding one `memory.db`, with its own permanent UUID, name and description. |
| Address | Two short IDs that open one memory from anywhere: `13625F6B:3487B9B0`, folder then memory. |
| Allow list | The directories you added, where Geniuz looks for folders besides its own data directory. |
| Archive | Hiding a folder by moving it into `archive/`. Nothing is deleted, and it can be restored. |
| Station | The older name for a folder. It survives in `GENIUZ_STATION` and in `station.db`, which Geniuz still reads. |
| Embedding | Numbers representing a memory's meaning, used to search by meaning. |
| Backfill | Embedding memories that do not have one yet. |
| Capture | Loading existing files into memory in bulk. |
| Recall | Searching your memories, by meaning or exactly. |
| Harvest | What a recall's results point onward to, read from those memories. |
| MCP | Model Context Protocol, a standard way for AI tools to call other programs. |
| Embed helper | `geniuz-embed`, which keeps the model loaded between calls. |

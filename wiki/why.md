# Why it is built this way

## Memories are never edited

"Memories are immutable", says [`schema/schema.sql`](../schema/schema.sql), "the log is the truth". Triggers enforce it in the file itself, so no tool can quietly rewrite the past. To correct something, you add a memory that replies to it, and both remain.

The cost is that a mistake stays on the record. In return, what an AI remembers can always be traced.

## No server

"There is no HTTP server in Geniuz Free", says the [README](../README.md#architecture). There is nothing to host, keep running or secure, and tools reach Geniuz by running it or through MCP. That also leaves the storage free to change, which is why the SQLite file is not a public interface.

## Local by default

Your memory is a file on your computer. You can copy it, back it up or read the code that touches it, and no account or API key stands between you and it.

## Meaning and exact search, together

Search by meaning finds things you cannot quite name, and misses things you can: a person, a date, an exact phrase. [`src/dig.rs`](../src/dig.rs) says it this way: "Semantic search is for discovery; exact search is for recall". So Geniuz has both, and puts names first when a query contains them.

## Independent of any AI tool

A memory that belongs to one AI product is lost when you leave it. Geniuz works with anything that can run a command or speak MCP, so your memory comes with you.

## Small on purpose

"Geniuz is a small substrate by design", says [`CONTRIBUTING.md`](../CONTRIBUTING.md). New features have to justify themselves against that.

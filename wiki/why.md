# Why it is built this way

## Memories are never edited

"Memories are immutable", says [`schema/schema.sql`](../schema/schema.sql), "the log is the truth". Triggers enforce it in the file itself, so changing the past takes a deliberate step: dropping the trigger. To correct something, you add a memory that replies to it, and both remain.

The cost is that a mistake stays on the record. In return, the history is hard to rewrite by accident.

## No server

Geniuz has no HTTP server. There is nothing to host, keep running or secure, and tools reach Geniuz by running it or through MCP.

## One table

A UUID, a JSON payload, a parent, a time. Rows are added and never changed. That shape holds a surprising range of work, and it spares you most of the design a normalized schema would demand up front. It is a way of working as much as a file format, so tailor it: the name of the UUID column, the shape of the payload, the promises a given store makes. Make something updatable only where a concrete need says so, and write that need down beside the schema.

## Folders without types

A project, a knowledge base and a conversation between agents are the same file with a different description. Adding types would mean deciding the categories for everyone, and the people who used stores like these wrote what they needed in free text anyway.

## UUIDs are forever

Memories point at each other by address, so an ID that changes breaks every memory that mentions it. That is why a folder can be moved but not copied, and why Geniuz would rather refuse than guess.

If a copy turns up anyway, every copy stops working until one is moved. Nothing can tell which is the original, and carrying on with the wrong one would split the history in two. That's disruptive when it happens, and we chose it knowingly: an error you can fix beats a loss you can't see.

## The AI has to be told

A memory nobody uses is worth nothing. In our tests, agents with good tools and no other guidance used Geniuz in 0 of 8 sessions. With a few sentences on when to recall and keep, all of them did, in the right folders. So the server says so when an AI connects. It is a small sample on one model, and the point stands: being available isn't the same as being used.

## Local by default

Your memory is files on your computer. You can move them, back them up or read the code that touches them, and no account or API key stands between you and them.

## Meaning and exact search, together

Search by meaning finds things you cannot quite name, and misses things you can: a person, a date, an exact phrase. [`src/dig.rs`](../src/dig.rs) says it this way: "Semantic search is for discovery; exact search is for recall". So Geniuz has both, and puts names first when a query contains them.

## Independent of any AI tool

A memory that belongs to one AI product is lost when you leave it. Geniuz works with anything that can run a command or speak MCP, so your memory comes with you.

## Small on purpose

"Geniuz is a small substrate by design", says [`CONTRIBUTING.md`](../CONTRIBUTING.md). New features have to justify themselves against that.

---
name: geniuz
version: {{GENIUZ_VERSION}}
description: Memory that lasts beyond a chat, kept on this computer. Use it to keep decisions, progress and context worth coming back to, and to recall them later by meaning, by exact words, or by address (FOLDER:MEMORY). If `geniuz --version` is newer than this skill's version, run `geniuz skill` to see the current one.
---

# geniuz

Your next session starts cold. No memory of what you built, what broke, what you decided. Every memory you write is a gift to that future session. The richer the memory, the less time re-learning.

Works with any framework — Claude Code, Cursor, Windsurf, Aider, or anything with shell access.

AI tools use the CLI commands below: `geniuz keep` writes, `geniuz recall` reads. The terminal UI (`geniuz tui`) and the dashboard are for people to browse their memories. The TUI refuses to launch from non-interactive callers and exits with code 2, so calling it from a subprocess locks nothing up.

## Why this matters

A memory that says "fixed auth bug" saves your future self zero time.

A memory that says what broke, why it broke, and how you fixed it saves an hour.

Memories compound. A single memory is a note. A folder of memories is institutional knowledge. Write for the version of you that knows nothing.

## Asking first

Most people like to be asked, once, whether notes should be kept in Geniuz: decisions, progress, what to pick up next time. When they agree, keeping as you go and mentioning what was kept works well. It helps in this chat and in the next.

## Commands

Every keep names its folder (`--in`) and its gist (`-g`).

```bash
# Save what you learned — pipe in for depth, inline for quick notes
echo "Token validation was running before refresh in auth.rs.
Swapped lines 42-47. Root cause: middleware ordering assumed
sync validation, but OAuth refresh is async. Three edge cases
tested: expired token, revoked token, concurrent refresh." \
  | geniuz keep --in Work -c - -g "fix: auth token refresh — async ordering in middleware"

# Quick memory when depth isn't needed
geniuz keep --in Work -c "Upgraded rusqlite to 0.32" -g "dep: rusqlite 0.32"

# Keep from a file
geniuz keep --in Work -c @session-notes.md -g "session: March 19 architecture review"

# Thread a follow-up
geniuz keep --in Work -c "Same fix needed in staging compose" -g "fix: staging auth ordering" -p A1B2C3D4

# Search by meaning — not keywords
geniuz recall "authentication middleware"
geniuz recall "what broke in production" --since 7d

# Proper names match exactly and come first (a Capitalized word after the
# first, or anything in "quotes")
geniuz recall "who reviewed the PR, was it Cubic"

# Exact words: the matching LINES from full content, newest first
geniuz recall --grep Cubic --since 7d
geniuz recall --grep Cubic --grep "pull request"   # every term must appear

# A whole thread, root first, from any memory in it
geniuz recall --thread A1B2C3D4

# Keyword fallback (whole memories, any word)
geniuz recall --keyword "auth"

# Recent memories: recall with no search
geniuz recall
geniuz recall -l 5

# Full content (not just gist)
geniuz recall --full "auth"

# Discover something you forgot you knew
geniuz recall --random

# Bulk-load existing files
geniuz capture ./docs/

# Build embedding cache (once, then automatic)
geniuz backfill

# Stats for the folder in use
geniuz status
```

## Folders

Memories live in folders, each with a name and a description. Choose the folder before you keep, because a memory can't be moved afterwards. Read its description first: it may say what belongs there, what must not go there, and how to use it.

**Make a folder for anything you'll come back to:** a project, a topic, a long piece of work. In a short chat or a long one, keep what matters as you go. Don't count on the chat's history to hold it: a chat gets compacted, closed or forgotten, and a folder keeps what you put in it.

```bash
geniuz folder create "Website rebuild" -d "Decisions and progress on the new site."
```

The description is for whoever opens the folder next. Often that's you. Through MCP, pass `create` to the `folders` tool.

```bash
geniuz folder list                     # all of them; * is the one in use
geniuz folder show Work                # one in full, with its description

geniuz keep --in Work -c "..." -g "..."
geniuz recall --in Work "deploy"
geniuz recall --all "deploy"           # every folder; each result shows its address
geniuz recall 13625F6B:3487B9B0        # open an address, whichever folder it's in
```

A wrong name is an error, never a different folder. A `.geniuz-folder` file holding a folder's name or ID decides what a plain `geniuz recall` reads in that directory; a keep still names its folder.

Multiple agents can use the same folder. What one agent learns, every agent finds. That also makes a folder a way to leave each other messages: thread your reply to theirs with `-p`.

Never copy a folder's files: a copy of a live database can lose memories, and a folder can't be duplicated, because its ID and its memories' IDs are permanent. To relocate one, use `geniuz folder move`.

## When to keep

- **After solving something hard.** Your next session will hit the same problem. Give it the answer.
- **Before your session ends.** Everything in your context window dies with the session. Your folder doesn't.
- **When you discover something non-obvious.** The small facts — which config flag, which line number, which edge case — are the ones that save the most time.

## When to recall

- **Starting a new task.** You may have already solved part of it last week.
- **When stuck.** The answer might be in a memory from three sessions ago.
- **After compaction.** Your context was compressed. Your folder wasn't.

## How to find things

Meaning is for **discovery**: concepts and names you didn't know to ask for.
Exact is for **recall**: who, when, did it happen, by proper name.

1. If the question has a time in it ("last week", "yesterday"), add
   `--since`: `7d`, `24h`, `2w`, or a date. Without it, months of older
   memories outrank last week's answer.
2. Start by meaning. Every answer ends with `names seen:` and `pointers:`.
   Those are your next hops.
3. Go exact with what you found: `--grep Devin`, or `--thread 3487B9B0` to
   read the whole conversation.
4. Read the last line. `searched: 312 memories · since 7d · grep "Cubic"`
   says what "nothing found" means: nothing *in that scope*.

Through MCP the same works as `recall` arguments: `folder`, `all`, `since`,
`until`, `grep` (a word or a list), `thread`. `query` takes an address too.

> Note: `geniuz recall help` prints recall's help instead of searching for the literal word "help". To find memories about that word, use a phrase containing it — e.g. `geniuz recall "help with debugging"` or `geniuz recall "how to help"`.

## Writing good memories

The gist is how your future self finds this memory. The content is what makes it useful when found.

**Gist:** compress the insight. `"fix: auth token refresh — async ordering in middleware"` — category, what, why, where.

**Content:** make it self-contained. If your future self reads only this memory — no session history, no surrounding context — can they understand what happened and act on it?

A memory doesn't need to be long. It needs to be complete.

**Anything longer than a sentence: use a file (`-c @note.md`) or stdin (`-c -`).** The inline form is the one a shell quietly corrupts — backticks get evaluated, quotes get eaten, newlines collapse — and you cannot take back what was kept.

When something works, write down **what the green light did not tell you**. A passing test proves something narrower than you meant to ask. That gap is the difference between a note and a lesson.

## Memories are permanent

There is no edit and no delete. If a memory was wrong, keep the correction threaded to it with `-p`. The folder keeps the wrong turn and the fix together, which is why you can trust what you find in it.

## How it works

Memories live in a SQLite database. Semantic search uses a local BERT model — no API calls, no cloud, runs fully offline. The model downloads once (~118MB) on first search. Every memory after that is embedded automatically.

Search finds memories by meaning, not keywords. "Authentication middleware" matches a memory about "token validation ordering" because the concepts overlap.

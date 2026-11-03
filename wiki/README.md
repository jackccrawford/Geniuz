# Geniuz Wiki

Geniuz is persistent memory for AI agents. It runs on your machine, it is MIT-licensed, and it is free. This wiki explains how it works. It lives in the repo beside the code, and is also published at [geniuz.life/wiki](https://geniuz.life/wiki/).

It describes version 4.2.0.

## The idea in one paragraph

Your AI starts every conversation from zero. Geniuz gives it a place to keep what it learned: a local SQLite file it can write to and search by meaning. No memory in that file is ever edited or deleted, so a correction is a new memory linked to the old one, and the history stays readable. Any tool that can run a command or speak MCP can use the same memory, so switching AI tools does not mean starting over.

## Where to go next

- [How it works](how-it-works.md): what happens when you remember and when you recall.
- [Map](map.md): the parts and how they connect.
- [Using it](using-it.md): install, commands, connecting your AI, settings.
- [Why it is built this way](why.md): the design decisions.
- [Glossary](glossary.md): the words Geniuz uses.

## Community Edition and Team

This repository is Geniuz Community Edition: open source, one memory store for any AI tool. Make a PR if you like, fork if you like. We maintain it with bug fixes. Geniuz Team is a separate commercial product from [mVara](https://github.com/mvara-ai), available by pilot. If you want memory for yourself or a developer team using the AI tools you already have, Geniuz Community Edition is what you want.

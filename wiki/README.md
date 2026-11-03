# Geniuz Wiki

Geniuz is persistent memory for AI agents. It runs on your machine, it is MIT-licensed, and it is free. This wiki explains how it works. It lives in the repo beside the code, and is also published at [geniuz.life/wiki](https://geniuz.life/wiki/).

It describes version 5.0.0.

## The idea in one paragraph

Your AI starts every conversation from zero. Geniuz gives it a place to keep what it learned: local SQLite files it can write to and search by meaning, organized into folders. No memory is ever edited or deleted, so a correction is a new memory linked to the old one, and the history stays readable. Any tool that can run a command or speak MCP can use the same memory, so switching AI tools does not mean starting over.

## Where to go next

- [How it works](how-it-works.md): what happens when you keep and when you recall.
- [Map](map.md): the parts and how they connect.
- [Using it](using-it.md): install, commands, connecting your AI, settings.
- [Why it is built this way](why.md): the design decisions.
- [Glossary](glossary.md): the words Geniuz uses.

# Using it

## Install

| You have | Do this |
|---|---|
| A Mac | Download [Geniuz.dmg](https://github.com/jackccrawford/geniuz/releases/latest/download/Geniuz.dmg) and drag it to Applications. To use `geniuz` in a terminal, use **Copy Install Command** in the menubar. |
| Windows 11 | Download [Geniuz-Setup.exe](https://github.com/jackccrawford/geniuz/releases/latest/download/Geniuz-Setup.exe). It installs per user, with no admin prompt. |
| Linux | Run the command below, or see the [releases](https://github.com/jackccrawford/geniuz/releases/latest) for `.deb`, AppImage and tarballs (x86_64 and arm64). |

```bash
curl -fsSL https://raw.githubusercontent.com/jackccrawford/geniuz/main/install.sh | bash
```

The script installs to `~/.geniuz/bin/`. To build from source, see the [README](../README.md#from-source).

## Connect your AI

| You use | Next step |
|---|---|
| Claude Desktop | `geniuz mcp install`, then restart Claude Desktop |
| Claude Code, Cursor, Windsurf | `geniuz skill` prints instructions to give your agent |
| Your own agents | Call `geniuz remember` and `geniuz recall` from a shell, or run `geniuz mcp serve` |

## Commands

```bash
geniuz remember -c "what happened" -g "category: short summary"
geniuz remember -c @notes.md -g "session: review"
echo "content" | geniuz remember -c - -g "piped: from a process"
geniuz remember -c "follow-up" -g "update" -p 98672A90

geniuz recall "topic"
geniuz recall "topic" --since 7d
geniuz recall --grep Cubic --grep PR
geniuz recall --thread 98672A90
geniuz recall --keyword "exact words"
geniuz recall --random
geniuz recall --full "topic"
geniuz recall --json "topic"

geniuz recent -l 5
geniuz tui
geniuz dashboard
geniuz capture ./docs/
geniuz backfill
geniuz status
geniuz watch
geniuz mcp install
geniuz mcp status
geniuz mcp serve
geniuz settings list
```

**Note on `geniuz watch --exec`.** Your command runs through the shell with your own rights. `{gist}`, `{content}` and `{json}` reach it as shell variables, so put them in double quotes, not single. Shell syntax in a memory does not run.

**Do not store secrets.** A memory cannot be removed through Geniuz.

## Where your data lives

Everything is under `~/.geniuz/`: `memory.db`, `models/`, `settings.json` and `bin/`. To back up your memories, quit anything using Geniuz, then copy `memory.db`. Its schema is not a public interface and may change, so read and write through Geniuz.

## Settings

| Variable | What it does |
|---|---|
| `GENIUZ_HOME` | Where Geniuz keeps its files |
| `GENIUZ_STATION` | Use a different memory file, for the CLI and MCP |
| `GENIUZ_MODELS_PATH` | Where the model is stored |
| `GENIUZ_EMBED_URL`, `GENIUZ_EMBED_MODEL` | The embedding service to try if the built-in model fails to load |
| `GENIUZ_EMBED_SOCKET`, `GENIUZ_EMBED_SPAWN`, `GENIUZ_EMBED_IDLE` | Control the embed helper |

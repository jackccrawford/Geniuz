# Using it

## Install

| You have | Do this |
|---|---|
| A Mac | Download [Geniuz.dmg](https://github.com/jackccrawford/geniuz/releases/latest/download/Geniuz.dmg) and drag it to Applications. To use `geniuz` in a terminal, use **Settings → Copy Install Command** in the menubar. |
| Windows 11 | Download [Geniuz-Setup.exe](https://github.com/jackccrawford/geniuz/releases/latest/download/Geniuz-Setup.exe). It installs per user, with no admin prompt. |
| Linux | Run the command below, or see the [releases](https://github.com/jackccrawford/geniuz/releases/latest) for `.deb`, AppImage and tarballs (x86_64 and arm64). |

```bash
curl -fsSL https://raw.githubusercontent.com/jackccrawford/geniuz/main/install.sh | bash
```

The script installs to `~/.geniuz/bin/`. To build from source, see [below](#from-source).

### From source

```bash
git clone https://github.com/jackccrawford/geniuz && cd geniuz
cargo build --release --bin geniuz
cp target/release/geniuz ~/.local/bin/
```

To build the dashboard as well: `cd desktop/dashboard && cargo tauri build`.

## Connect your AI

| You use | Next step |
|---|---|
| Claude Desktop | Connects by itself when Geniuz opens. Restart Claude Desktop once. To connect by hand: `geniuz mcp install` |
| Claude Code, Cursor, Windsurf | `geniuz skill` prints instructions to give your agent |
| Your own agents | Call `geniuz keep` and `geniuz recall` from a shell, or run `geniuz mcp serve` |

Then keep talking. Your AI saves what matters as you go. Tomorrow, in a new chat, ask it what you told it today.

## Commands

```bash
geniuz keep -c "what happened" -g "category: short summary"
geniuz keep -c @notes.md -g "session: review"
echo "content" | geniuz keep -c - -g "piped: from a process"
geniuz keep -c "follow-up" -g "update" -p 98672A90

geniuz recall "topic"
geniuz recall "topic" --since 7d
geniuz recall --grep Cubic --grep PR
geniuz recall --thread 98672A90
geniuz recall --keyword "exact words"
geniuz recall --random
geniuz recall --full "topic"
geniuz recall --json "topic"

geniuz recall
geniuz recall -l 5
geniuz recall --all "topic"
geniuz recall 13625F6B:3487B9B0
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

`geniuz remember` still works as another name for `keep`, and `geniuz recent` for `recall` with no search. `keep` and `recall` take `--in <folder>`, as does every other command that reads or writes memories.

## Folders

```bash
geniuz folder list
geniuz folder list --archived
geniuz folder show Work
geniuz folder create Work -d "Client projects. No passwords here."
geniuz folder create Notes --at ~/notes
geniuz folder use Work
geniuz folder rename Work Clients
geniuz folder describe Clients "Client projects, and who owns them."
geniuz folder add /some/directory
geniuz folder remove /some/directory
geniuz folder move Clients --to /some/directory
geniuz folder archive Clients
geniuz folder restore Clients
geniuz folder pin Clients
geniuz folder unpin Clients
```

In the dashboard, folders are in the sidebar and in the Folders menu. Click the pin beside a name to keep it at the top of every list. Keep has a pulldown for choosing a folder, and Find can search any selection of them. To give a project its own folder, put the folder's name in a `.geniuz-folder` file in the project.

Editing a description keeps the old wording. Your first folder, "My memory", can be renamed, but it can't be moved or archived, because other files live beside it.

## Keyboard shortcuts

In the dashboard. Change any of these under Settings, and the menu follows at once.

| Keys, on a Mac | What they do |
|---|---|
| ⌘N | Keep |
| ⌘K | Find |
| ⌘0 | Memories in this folder |
| ⌘1 to ⌘9 | Your first nine folders, in sidebar order (not changeable) |
| ⇧⌘N | New folder |
| ⌘B | Show or hide the sidebar |
| ⇧⌘S | Status |
| ⌘, | Settings |
| ⇧⌘E | Export this folder |
| ⌘R | Refresh |

On Windows and Linux, use Ctrl in place of ⌘.

## Good to know

**Note on `geniuz watch --exec`.** Your command runs through the shell with your own rights. `{gist}`, `{content}` and `{json}` reach it as shell variables, so put them in double quotes, not single. Shell syntax in a memory does not run.

**Do not store secrets.** A memory cannot be removed through Geniuz.

## Where your data lives

Everything is under `~/.geniuz/` unless you moved it: `memory.db`, one directory per folder you made, `archive/`, `models/`, `settings.json` and `bin/`. A few small files hold your choices: `current`, `added-folders`, `pinned-folders` and `shortcuts.json`.

**To back up**, use Export under Data & Export in the dashboard, which saves the folder in use as one complete file. Time Machine and similar tools are fine too. A backup is a file to keep, not a folder to use: it holds the same permanent IDs, so Geniuz refuses it while the original is present.

**Do not copy a folder by hand to use it elsewhere.** A copy of a live database can lose recent memories, and copies aren't supported. Use `geniuz folder move`.

Read the schema freely, and write through Geniuz, which keeps embeddings and identity right.

## Settings

| Variable | What it does |
|---|---|
| `GENIUZ_HOME` | Where Geniuz keeps its files |
| `GENIUZ_STATION` | Use one specific memory file, for the CLI and MCP |
| `GENIUZ_MODELS_PATH` | Where the model is stored |
| `GENIUZ_EMBED_URL`, `GENIUZ_EMBED_MODEL` | The embedding service to try if the built-in model fails to load |
| `GENIUZ_EMBED_SOCKET`, `GENIUZ_EMBED_SPAWN`, `GENIUZ_EMBED_IDLE` | Control the embed helper |

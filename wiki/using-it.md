# Using it

## Install

| You have | Do this |
|---|---|
| A Mac with Apple silicon (M1 or later) | Download [Geniuz.dmg](https://github.com/jackccrawford/geniuz/releases/latest/download/Geniuz.dmg) and drag it to Applications. To use `geniuz` in a terminal, use **Settings → Copy Install Command** in the menubar. |
| Linux | Run the command below, or see the [releases](https://github.com/jackccrawford/geniuz/releases/latest) for `.deb`, AppImage and tarballs (x86_64 and arm64). The arm64 build runs on a Raspberry Pi 4 or later, with a 64-bit OS. |
| Windows 11 | Download [Geniuz-Setup.exe](https://github.com/jackccrawford/geniuz/releases/latest/download/Geniuz-Setup.exe). It installs per user, with no admin prompt. |

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

### Updates

On a Mac, the menubar tells you when a newer Geniuz is out, and **Download** opens it. Install it the same way, by dragging it to Applications. In the dashboard's Settings, under **Updates**, you choose how often it checks (daily, weekly, or only when you ask) or check now. The dashboard's Status page shows the same answer. A check asks GitHub for the latest release and sends nothing from your folders.

## Connect your AI

| You use | Next step |
|---|---|
| Claude Code, Cursor, Windsurf | `geniuz skill` prints instructions to give your agent |
| Claude Desktop | Connects by itself when Geniuz opens. Restart Claude Desktop once. To connect by hand: `geniuz mcp install` |
| Your own agents | Call `geniuz keep` and `geniuz recall` from a shell, or run `geniuz mcp serve` |

Then keep talking. Your AI saves what matters as you go. Tomorrow, in a new chat, ask it what you told it today.

## Commands

```bash
geniuz keep -f Work -c "what happened" -g "category: short summary"
geniuz keep -f Work -c @notes.md -g "session: review"
echo "content" | geniuz keep -f Work -c - -g "piped: from a process"
geniuz keep -f Work -c "follow-up" -g "update" -p 98672A90
geniuz keep -f Work -c "On 21 Mar we decided ..." -g "decision, 21 Mar: pricing"
geniuz -f Work keep -c "for the Work folder" -g "note: client call"

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

geniuz keep -f Work -c "The error dialog" -g "bug: save dialog" --image shot.png
geniuz keep -f Work -c "Same dialog, fixed" -g "fix: save dialog" --image-id 1B2C3D4E
geniuz image -f Work
geniuz image -f Work 1B2C3D4E -o shot.webp

geniuz tui
geniuz dashboard
geniuz capture -f Docs ./docs/
geniuz backfill
geniuz status
geniuz watch
geniuz watch --since 13625F6B:9D778206 --exec "echo {uuid} {gist}"
geniuz watch --once --since 13625F6B:9D778206
geniuz mcp install
geniuz mcp status
geniuz mcp serve
geniuz settings list
```

`keep` answers with the new memory's address, such as `Kept 13625F6B:F96D91B4`. Anyone can open it from any folder with `geniuz recall 13625F6B:F96D91B4`, so it's the thing to pass along.

A memory is dated when it's kept. To record something that happened earlier, say when in the gist or content, as above.

Every command takes `-f <folder>` (or `--in <folder>`), anywhere on the line, to use that folder for one command. `watch --since` takes an address and shows only what came after it. Give it the last address you saw and it continues where you stopped, even after a restart. `--once` checks a single time and exits with 0 if there's something new, which suits a script.

`geniuz remember` still works as another name for `keep`, and `geniuz recent` for `recall` with no search.

## Folders

```bash
geniuz folder list
geniuz folder list --archived
geniuz folder show Work
geniuz folder create Work -d "Client projects. No passwords here."
geniuz folder create Notes --at ~/notes
geniuz folder rename Work Clients
geniuz folder describe Clients "Client projects, and who owns them."
geniuz folder face Clients 🧰
geniuz folder add /some/directory
geniuz folder remove /some/directory
geniuz folder move Clients --to /some/directory
geniuz folder archive Clients
geniuz folder restore Clients
geniuz folder pin Clients
geniuz folder unpin Clients
```

**A name or an ID, either one.** Wherever a command takes a folder, you can give its name or the first 8 characters of its ID, in any case: `Work`, `work` and `95e6e59c` all open the same folder, and so does the full ID. A typo is an error, never a near match. If two folders share a name, use the ID; if two IDs share their first 8 characters (very unlikely), use the name or the full ID.

In the dashboard, folders are in the sidebar and in the Folders menu. Each folder has a face. Its writer can choose one emoji for it (`geniuz folder face`, or the folders tool in a chat); without one it is a small mosaic of nine tiles laid out from the folder's ID, so every folder has its own and it stays the same when the folder is renamed. Hover a folder and click **···** to pin it to the top, edit its name and description, or copy its ID. The sidebar is one list: the folders you pinned, a thin line, then the rest. Click the word beside **Folders** to change the order: A–Z, Z–A, most memories first, or fewest first. Keep has a pulldown for choosing a folder, and Find can search any selection of them.

To give a project its own folder for reading, put the folder's name or ID in a `.geniuz-folder` file in the project: `recall` run there, by you or an agent, reads that folder when none is named. Keeping still names its folder every time.

### Where a memory goes

Wherever the writer says. Every keep names its folder: `-f Work` on the command line, the folder in the dashboard's Keep pulldown, or the folder your AI chooses with its folders tool. A keep that names no folder is refused; nothing is kept by guess. The easiest way to keep something is to ask your AI to save it: it knows your folders and picks the right one.

Reading is more forgiving. `recall` with no folder reads the database `GENIUZ_DB` names, else the project's `.geniuz-folder`, else My memory, and `recall --all` reads every folder.

### Reading a folder

The memories view loads 100 at a time, newest or oldest first, and loads more as you scroll. Memories are numbered by age, so #1 is always the oldest, whichever way you sort.

Editing a description keeps the old wording. Your first folder, "My memory", can be renamed, but it can't be moved or archived, because other files live beside it.

## Images

A memory can show up to five images: a screenshot, a chart, a photo of a whiteboard. The words are still what search finds, so say in the content what the image shows.

**In the dashboard**, the Keep page has an Images row. Paste an image, drag one onto it, or choose a file. A memory that has images shows a small mark in the list, and the images themselves when you open it.

**From the command line**, add `--image <file>` to `keep`, once per image, or `--image-id <ID>` to show an image that is already kept. One image can serve many memories. `geniuz image` lists a folder's images, and `geniuz image <ID> -o file.webp` takes one out.

**Through an AI tool**, `keep` takes `images` (file paths, or the IDs of images already kept), and `recall` takes `image` to hand one back to look at. A result line names a memory's images after the gist, such as `· image 1B2C3D4E`. The picture is only loaded when it is asked for by that ID, so recalling a memory costs nothing extra.

Geniuz reads PNG, JPEG, GIF, WebP, BMP and TIFF files of up to 40 MB. It keeps each one as WebP, no wider or taller than 1568 pixels, inside the folder's own file. There are no loose image files to lose, and Export carries them.

### Taking an image back

An image is the one thing in Geniuz that can be removed. If you kept a picture you shouldn't have, its owner can have it erased, or have another put in its place, such as the same screenshot with a number blurred.

```bash
geniuz image -f Work admin withdraw 1B2C3D4E
geniuz image -f Work admin withdraw 1B2C3D4E --confirm <FULL-ID>
geniuz image -f Work admin replace 1B2C3D4E blurred.png
```

It takes two runs. The first changes nothing: it prints the image, the memories that show it, and the line to run next. The second carries `--confirm` with the image's full ID, and does it. If you use Geniuz through an AI agent that can run commands, ask it to do this for you. It is not offered through MCP, so an AI tool that can't run commands can't do it.

Afterwards the image is gone from the folder. A memory that showed it says `1 image unavailable` when read in full, and nothing more. What it doesn't reach: backups, exports, and anything already written about the image, in a memory or in a conversation. The memory's words stay.

## Keyboard shortcuts

In the dashboard. **Help → Keyboard Shortcuts** shows them all; click one there to change it, and the menu follows at once. Settings has a row that opens the same list.

| What it does | Keys, on a Mac |
|---|---|
| Export this folder | ⇧⌘E |
| Find | ⌘K |
| Keep | ⌘N |
| Memories in this folder | ⌘0 |
| New folder | ⇧⌘N |
| Refresh | ⌘R |
| Settings | ⌘, |
| Show or hide the sidebar | ⌘B |
| Status | ⇧⌘S |
| Text: bigger, smaller, default size (the View menu; all text in the window) | ⌘=, ⌘−, ⇧⌘0 |
| The first nine folders in the Folders menu, in the order it shows them (not changeable) | ⌘1 to ⌘9 |

On Windows and Linux, use Ctrl in place of ⌘.

## Good to know

**Note on `geniuz watch --exec`.** Your command runs through the shell with your own rights. `{gist}`, `{content}` and `{json}` reach it as shell variables, so put them in double quotes, not single. Shell syntax in a memory does not run.

**Do not store secrets.** A memory cannot be removed through Geniuz. An image can be taken back (see Images above); the words kept with it cannot.

## Where your data lives

Everything is under `~/.geniuz/` unless you moved it: `memory.db`, one directory per folder you made, `archive/`, `models/`, `settings.json` and `bin/`. A few small files hold your choices: `added-folders`, `pinned-folders` and `shortcuts.json`. A `current` file from before 5.0.8 is no longer read.

**To back up**, use Export under Data & Export in the dashboard, which saves the folder on screen as one complete file. Time Machine and similar tools are fine too. A backup is a file to keep, not a folder to use: it holds the same permanent IDs, so Geniuz refuses it while the original is present.

**Do not copy a folder by hand to use it elsewhere.** A copy of a live database can lose recent memories, and copies aren't supported. Use `geniuz folder move`.

Read the schema freely, and write through Geniuz, which keeps embeddings and identity right.

**A memory file from before Geniuz 4.0** isn't opened: Geniuz says so and names [`schema/migrate-before-4.0.sql`](../schema/migrate-before-4.0.sql), which brings it forward once, with a backup first. The steps are in the script, written so an AI agent can carry them out.

## Settings

Your choices are in one place: the dashboard's Settings page. On a Mac, the gear in the menubar opens it.

| Setting | What it does |
|---|---|
| Launch Geniuz at login | Starts the menubar app when you sign in |
| Recent memories in the menu bar | How many the menubar shows; 0 hides them |
| Autocorrect as you type | Off unless you turn it on. What you keep is read by an AI, which understands a typo; autocorrect can change a word you meant |
| Check for updates automatically, and how often | Daily, weekly, or only when you ask |
| Keyboard shortcuts | Opens the list, where you can change one |

`geniuz settings list` shows the same choices from the command line, and `geniuz settings set <name> <value>` changes one. They are kept in `settings.json`.

These variables are for unusual setups:

| Variable | What it does |
|---|---|
| `GENIUZ_DB` | Use one specific memory file, for the CLI and MCP |
| `GENIUZ_EMBED_SOCKET`, `GENIUZ_EMBED_SPAWN`, `GENIUZ_EMBED_IDLE` | Control the embed helper |
| `GENIUZ_EMBED_URL`, `GENIUZ_EMBED_MODEL` | The embedding service to try if the built-in model fails to load |
| `GENIUZ_HOME` | Where Geniuz keeps its files |
| `GENIUZ_MODELS_PATH` | Where the model is stored |

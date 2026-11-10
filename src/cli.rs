use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "geniuz",
    version,
    about = "Your AI remembers now",
    long_about = "GENIUZ: Your AI remembers now.\n\nYour AI starts every conversation from zero. Geniuz keeps what matters,\nthe calls you made and the reasons behind them, on your own computer,\nwhere any AI you use can find it: Claude Desktop through MCP, or any\nagent that can run a command. A memory is never edited, so the record holds.",
    before_help = "Start here: 'geniuz recall' shows what's in your folder.",
    after_help = "Examples:\n  geniuz keep -f Work -c \"Chose the smaller vendor: faster support\" -g \"decision: vendor\"\n  geniuz recall \"vendor\"                 Search by meaning\n  geniuz recall --grep Acme              Literal text: every line that has it\n  geniuz recall 13625F6B:3487B9B0        Open one memory by its address\n  geniuz recall --all \"pricing\"          Every folder at once\n  geniuz capture -f Docs ./policies/     Keep a folder of documents\n\nFolders: 'geniuz folder list' shows them; -f <name> uses one for a command.\n  A project, a team, an agent: each can have its own, or share one.\n\nData: everything stays in .geniuz in your home directory.\n  GENIUZ_HOME moves it; GENIUZ_DB names one memory file.\n\nUse \"geniuz <command> --help\" for more."
)]
pub struct Cli {
    // -f and --folder as well: agents name a folder on most calls, so it gets a short
    // flag. --full keeps its long form only.
    /// Use this folder for the command: a name or its 8-character ID
    #[arg(short = 'f', long = "in", alias = "folder", global = true, value_name = "FOLDER")]
    pub in_folder: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    pub fn build() -> clap::Command {
        let cmd = <Self as clap::CommandFactory>::command();
        let styles = clap::builder::styling::Styles::plain();
        let mut cmd = cmd.styles(styles.clone()).disable_version_flag(true);
        for sub in cmd.get_subcommands_mut() {
            *sub = sub.clone().styles(styles.clone());
        }
        cmd.arg(
            clap::Arg::new("version")
                .short('v').long("version")
                .action(clap::ArgAction::Version)
                .help("Show version information")
        )
    }
}

#[derive(Subcommand)]
pub enum Command {
    /// Keep a memory: what you learned, decided, or discovered
    #[command(
        alias = "remember",
        arg_required_else_help = true,
        after_help = "Examples:\n  geniuz keep -f Work -c \"Token refresh order\" -g \"fix: auth token refresh\"\n  geniuz keep -f LIMEN -c @notes.md -g \"session: review\"\n  echo \"content\" | geniuz keep -f Work -c - -g \"piped: from process\"\n\nThe folder and the gist are required: name where it goes, and write the gist for whoever finds it next. `geniuz folder list` lists the folders."
    )]
    Keep {
        /// Content to save
        #[arg(short, long)]
        content: String,

        /// The short line it's found by: write it for whoever reads it next (required)
        #[arg(short, long)]
        gist: String,

        /// Thread to a parent memory (UUID prefix)
        #[arg(short, long)]
        parent: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Search your memories, by meaning unless you ask for exact words
    #[command(
        after_help = "Examples:\n  geniuz recall \"auth token\"                  Semantic search\n  geniuz recall --keyword \"auth\"               Keyword fallback\n  geniuz recall --random                       Discover something\n  geniuz recall --full \"auth\"                   Include full content\n  geniuz recall --since 7d \"reviewers\"          Only the past week\n  geniuz recall --since 2026-09-01 --until 2026-09-15\n  geniuz recall --thread 17DB510B              The whole conversation, oldest first\n  geniuz recall --grep Cubic --since 7d        Lines naming Cubic, past week\n\nTip: Run 'geniuz backfill' first to enable semantic search."
    )]
    Recall {
        /// Search query, or an address like 13625F6B:3487B9B0 to open that memory
        query: Option<String>,

        /// Search every folder. Each result shows the address that opens it
        #[arg(long, conflicts_with_all = ["thread", "grep", "random"])]
        all: bool,

        /// Show the whole thread this memory belongs to (a UUID, prefix or address):
        /// up to its root, then everything under it, oldest first
        #[arg(long, value_name = "ID", conflicts_with_all = ["query", "random", "keyword"])]
        thread: Option<String>,

        /// Exact match over full content, case-blind: show the matching
        /// LINES. Repeat for more terms; every term must appear in a memory.
        /// Combines with --since/--until and --thread
        #[arg(long, value_name = "TERM", conflicts_with_all = ["query", "random", "keyword"])]
        grep: Vec<String>,

        /// Discover a random memory
        #[arg(long)]
        random: bool,

        /// Force keyword search (skip semantic)
        #[arg(short, long)]
        keyword: bool,

        /// Include full content (no short form: -f names the folder)
        #[arg(long)]
        full: bool,

        /// Only memories from this time on: 24h, 7d, 2w, a date (2026-09-23),
        /// a local time (2026-09-23 14:00) or RFC 3339
        #[arg(long, value_name = "WHEN")]
        since: Option<String>,

        /// Only memories before this time (same forms; a date includes that whole day)
        #[arg(long, value_name = "WHEN")]
        until: Option<String>,

        /// Max results [default: 20; 100 with --thread; 50 lines with --grep]
        #[arg(short, long)]
        limit: Option<usize>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show recent memories (same as `recall` with no search)
    #[command(
        hide = true,
        after_help = "Examples:\n  geniuz recent                              Latest 20 memories\n  geniuz recent -l 5                         Latest 5\n  geniuz recent --full                       Include full content\n  geniuz recent --since 24h                  Everything from the last day\n  geniuz recent --json                       JSON output"
    )]
    Recent {
        /// Max results
        #[arg(short, long, default_value = "20")]
        limit: usize,

        /// Only memories from this time on: 24h, 7d, 2w, a date (2026-09-23),
        /// a local time (2026-09-23 14:00) or RFC 3339
        #[arg(long, value_name = "WHEN")]
        since: Option<String>,

        /// Only memories before this time (same forms; a date includes that whole day)
        #[arg(long, value_name = "WHEN")]
        until: Option<String>,

        /// Include full content (no short form: -f names the folder)
        #[arg(long)]
        full: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// See and manage your folders
    #[command(subcommand, alias = "folders")]
    Folder(FolderCommand),

    /// Capture files or directories into a folder
    #[command(
        arg_required_else_help = true,
        after_help = "Examples:\n  geniuz capture -f Docs notes.md                      Single file\n  geniuz capture -f Docs ./docs/                       All .md files in directory\n  geniuz capture -f Docs *.md                          Shell glob\n  geniuz capture -f Docs --split notes.md              Split by ## headers\n  geniuz capture -f Docs --gist-prefix \"docs:\" a.md    Prefix all gists\n  geniuz capture -f Docs --dry-run ./notes/            Preview without importing\n\nEach file becomes a memory. With --split, each ## section becomes\na threaded memory under the file's root."
    )]
    Capture {
        /// Files or directories to capture
        paths: Vec<String>,

        /// Split files by ## headers into threaded memories
        #[arg(long)]
        split: bool,

        /// Prefix for auto-generated gists
        #[arg(long)]
        gist_prefix: Option<String>,

        /// Preview what would be captured without writing
        #[arg(long)]
        dry_run: bool,

        /// Capture non-prose file extensions (.html, .js, .css, .json, etc.)
        /// Default: only .md/.markdown/.txt/.rst/.org are accepted.
        /// Non-prose files produce noisy embeddings that pollute semantic recall.
        #[arg(long)]
        force: bool,
    },

    /// Watch for new memories in real time
    #[command(
        after_help = "Examples:\n  geniuz watch                              Poll every 5 minutes\n  geniuz watch --interval 60                Poll every 60 seconds\n  geniuz watch --since 13625F6B:9D778206    Only memories after this one\n  geniuz watch --exec \"echo {uuid} {gist}\"  Run command on each new memory\n  geniuz watch --once --since 9D778206      Check once and exit\n\nPlaceholders for --exec:\n  {uuid}       UUID (short)\n  {gist}       Gist\n  {content}    Full content\n  {created_at} Timestamp\n  {parent}     Parent UUID (empty if none)\n  {json}       Full memory as JSON"
    )]
    Watch {
        /// Poll interval in seconds (default: 300 = 5 minutes)
        #[arg(short, long, default_value = "300")]
        interval: u64,

        /// Only show memories after this one (a UUID or an address FOLDER:MEMORY)
        #[arg(short, long)]
        since: Option<String>,

        /// Run command for each new memory (supports {uuid}, {gist}, {content}, {created_at}, {parent}, {json}; memory text arrives as a shell variable, so use double quotes, not single)
        #[arg(short, long)]
        exec: Option<String>,

        /// Check once and exit (exit code 0 = new memories, 1 = none); needs --since
        #[arg(long)]
        once: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Make memories without an embedding searchable by meaning
    #[command(
        after_help = "Embeds all content using ONNX (paraphrase-multilingual-MiniLM-L12-v2).\nFirst run downloads the model (~118MB). Subsequent runs only process new memories."
    )]
    Backfill,

    /// Show usage guide for agents
    Skill,

    /// Pick a gist from a note's own text: the sentence closest in meaning to the whole.
    /// Hidden: a building block for the window, not yet a promise
    #[command(hide = true)]
    Gist {
        /// The note. Leave it out, or give -, to read it from standard input
        text: Vec<String>,
    },

    /// Show stats for the folder in use
    Status,

    /// Launch the terminal UI for browsing your memories
    #[command(
        after_help = "An interactive terminal app for navigating your memories.\nRequires an interactive terminal (refuses to launch from scripts/agents)."
    )]
    Tui,

    /// Launch the graphical dashboard app
    #[command(
        alias = "gui",
        after_help = "Launches the Geniuz Dashboard window. The dashboard is the\ngraphical companion to the CLI: the same memories, in a window with\nyour folders in a sidebar, and pages for Memories, Keep, Find,\nStatus, Data & Export, and Settings.\n\nResolves the dashboard binary from known install locations per\nplatform. Errors with a helpful message if the dashboard isn't\ninstalled (e.g. CLI-only install via curl install.sh)."
    )]
    Dashboard,

    /// Connect your AI through MCP: run the server, install it, or check it
    #[command(subcommand)]
    Mcp(McpCommand),

    /// Your settings: read or change them
    #[command(subcommand)]
    Settings(SettingsCommand),
}

#[derive(Subcommand)]
pub enum FolderCommand {
    /// List your folders. The one in use is marked with *
    List {
        /// List archived folders instead
        #[arg(long)]
        archived: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show one folder in full (the one in use if none is named)
    Show {
        /// Name or 8-character ID
        name: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Create a folder
    #[command(after_help = "Examples:\n  geniuz folder create Work\n  geniuz folder create Clients -d \"One thread per client. No passwords here.\"\n  geniuz folder create Notes --at ~/notes")]
    Create {
        /// Its name
        name: String,

        /// What should whoever comes here next know?
        #[arg(short, long, default_value = "")]
        description: String,

        /// Directory to create it in, which is also added to your list
        #[arg(long, value_name = "DIR")]
        at: Option<PathBuf>,
    },

    /// Make Geniuz look in a directory for folders. Refuses duplicates
    Add {
        /// The directory
        path: PathBuf,
    },

    /// Stop looking in an added directory. Its files aren't touched
    Remove {
        /// The directory
        path: PathBuf,
    },

    /// Removed in 5.0.8 (there is no default folder). Kept, hidden, so it can say what to do instead
    #[command(hide = true)]
    Use {
        /// Ignored
        name: Option<String>,
        /// Ignored
        #[arg(long)]
        none: bool,
    },

    /// Rename a folder. Its ID and addresses don't change
    Rename {
        /// Name or 8-character ID
        name: String,
        /// The new name
        new_name: String,
    },

    /// Choose the face shown beside a folder's name in the window: one emoji. Leave it out to clear it
    #[command(after_help = "Example:\n  geniuz folder face Work 🧰\n\nA folder's own writer chooses its face. Without one, the window shows a small mosaic that is the folder's alone.")]
    Face {
        /// Name or 8-character ID
        name: String,
        /// One emoji
        face: Option<String>,
    },

    /// Change what a folder says about itself. Earlier wording is kept
    Describe {
        /// Name or 8-character ID
        name: String,
        /// What should whoever comes here next know?
        description: String,
    },

    /// Move a folder safely. Use this rather than copying the files: a copy of a live database can lose memories
    Move {
        /// Name or 8-character ID
        name: String,
        /// Directory to move it to
        #[arg(long, value_name = "DIR")]
        to: PathBuf,
    },

    /// Archive a folder: hide it and keep it. Nothing is deleted; `folder restore` brings it back
    Archive {
        /// Name or 8-character ID
        name: String,
    },

    /// Keep a folder at the top of every list
    Pin {
        /// Name or 8-character ID
        name: String,
    },

    /// Stop keeping a folder at the top
    Unpin {
        /// Name or 8-character ID
        name: String,
    },

    /// Bring an archived folder back
    Restore {
        /// Name or 8-character ID
        name: String,
    },
}

#[derive(Subcommand)]
pub enum SettingsCommand {
    /// Show all settings as `key = value` lines (or JSON with --json)
    List {
        /// Output as JSON (used by GUIs that prefer structured input)
        #[arg(long)]
        json: bool,
    },

    /// Print a single setting's value (for scripting)
    Get {
        /// Setting key (launch_at_login, recent_memories_count)
        key: String,
    },

    /// Set a single setting and save
    #[command(after_help = "Examples:\n  geniuz settings set recent_memories_count 10\n\nValid keys:\n  launch_at_login          true | false\n  autoupdate_enabled       true | false\n  update_check_frequency   daily | weekly | manual\n  recent_memories_count    non-negative integer")]
    Set {
        /// Setting key
        key: String,
        /// New value
        value: String,
    },

    /// Print the path to the settings file
    Path,
}

#[derive(Subcommand)]
pub enum McpCommand {
    /// Run as an MCP server over stdio (how Claude Desktop starts Geniuz)
    Serve,

    /// Install Geniuz into Claude Desktop config
    #[command(
        after_help = "Adds Geniuz as an MCP server in Claude Desktop's config file.\nAfter running this, restart Claude Desktop to activate.\n\nYour Claude will have three new tools: keep, recall, folders.\n\nUse --env KEY=VALUE to embed environment variables in the MCP entry:\nthe Windows installer uses this to pass GENIUZ_HOME so the sandboxed\nClaude knows where the user's data directory lives."
    )]
    Install {
        /// Embed an environment variable in the MCP server entry. Repeatable.
        /// Example: --env GENIUZ_HOME=C:\Users\you\.geniuz
        #[arg(long, value_name = "KEY=VALUE")]
        env: Vec<String>,
    },

    /// Check if Geniuz is configured in Claude Desktop
    Status,
}

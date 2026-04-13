use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "geniuz",
    version,
    about = "Your AI remembers now",
    long_about = "GENIUZ: Your AI remembers now.\n\nPersistent memory for AI agents. Three R's: remember, recall, recent.\nWorks with any agent framework — Claude Code, Cursor, Windsurf, Aider,\nor anything that can run a shell command.",
    before_help = "Start here: 'geniuz recent' to see what's in your station.",
    after_help = "Examples:\n  geniuz remember -c \"Fixed the auth bug\" -g \"fix: token refresh\"\n  geniuz recall \"auth\"                       Semantic search\n  geniuz recent                              Latest memories\n  geniuz capture ./notes/                    Bulk-load markdown files\n  geniuz backfill                            Build embedding cache\n\nStation: Defaults to ~/.geniuz/station.db\n  Override: GENIUZ_STATION=/path/to/station.db geniuz remember ...\n  Multiple agents can share a station for shared memory.\n\nUse \"geniuz [command] --help\" for more information."
)]
pub struct Cli {
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
    /// Save a memory — what you learned, decided, or discovered
    #[command(
        arg_required_else_help = true,
        after_help = "Examples:\n  geniuz remember -c \"Fixed the auth bug\"\n  geniuz remember -c \"Token refresh order\" -g \"fix: auth token refresh\"\n  geniuz remember -c @notes.md -g \"session: review\"\n  echo \"content\" | geniuz remember -c - -g \"piped: from process\"\n\nTip: The gist is how future agents find this memory. Write for them."
    )]
    Remember {
        /// Content to save
        #[arg(short, long)]
        content: String,

        /// Compressed insight — how future agents find this
        #[arg(short, long)]
        gist: Option<String>,

        /// Thread to a parent memory (UUID prefix)
        #[arg(short, long)]
        parent: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Search your memories — semantic by default
    #[command(
        after_help = "Examples:\n  geniuz recall \"auth token\"                  Semantic search\n  geniuz recall --keyword \"auth\"               Keyword fallback\n  geniuz recall --random                       Discover something\n  geniuz recall --full \"auth\"                   Include full content\n\nTip: Run 'geniuz backfill' first to enable semantic search."
    )]
    Recall {
        /// Search query
        query: Option<String>,

        /// Discover a random memory
        #[arg(long)]
        random: bool,

        /// Force keyword search (skip semantic)
        #[arg(short, long)]
        keyword: bool,

        /// Include full content
        #[arg(short, long)]
        full: bool,

        /// Max results
        #[arg(short, long, default_value = "20")]
        limit: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show recent memories
    #[command(
        after_help = "Examples:\n  geniuz recent                              Latest 20 memories\n  geniuz recent -l 5                         Latest 5\n  geniuz recent --full                       Include full content\n  geniuz recent --json                       JSON output"
    )]
    Recent {
        /// Max results
        #[arg(short, long, default_value = "20")]
        limit: usize,

        /// Include full content
        #[arg(short, long)]
        full: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Capture files or directories into your station
    #[command(
        arg_required_else_help = true,
        after_help = "Examples:\n  geniuz capture notes.md                              Single file\n  geniuz capture ./docs/                               All .md files in directory\n  geniuz capture *.md                                  Shell glob\n  geniuz capture --split notes.md                      Split by ## headers\n  geniuz capture --gist-prefix \"docs:\" a.md            Prefix all gists\n  geniuz capture --openclaw ~/.openclaw/workspace      Import OpenClaw memory\n  geniuz capture --dry-run ./notes/                    Preview without importing\n\nEach file becomes a memory. With --split, each ## section becomes\na threaded memory under the file's root."
    )]
    Capture {
        /// Files or directories to capture (not used with --openclaw)
        paths: Vec<String>,

        /// Import an OpenClaw workspace
        #[arg(long, conflicts_with = "paths")]
        openclaw: Option<Option<String>>,

        /// Split files by ## headers into threaded memories
        #[arg(long)]
        split: bool,

        /// Prefix for auto-generated gists
        #[arg(long)]
        gist_prefix: Option<String>,

        /// Preview what would be captured without writing
        #[arg(long)]
        dry_run: bool,
    },

    /// Watch for new memories in real time
    #[command(
        after_help = "Examples:\n  geniuz watch                              Poll every 5 minutes\n  geniuz watch --interval 60                Poll every 60 seconds\n  geniuz watch --since 9D778206             Only memories after this UUID\n  geniuz watch --exec \"echo {uuid} {gist}\"  Run command on each new memory\n  geniuz watch --once                       Check once and exit\n\nPlaceholders for --exec:\n  {uuid}       UUID (short)\n  {gist}       Gist\n  {content}    Full content\n  {created_at} Timestamp\n  {parent}     Parent UUID (empty if none)\n  {json}       Full memory as JSON"
    )]
    Watch {
        /// Poll interval in seconds (default: 300 = 5 minutes)
        #[arg(short, long, default_value = "300")]
        interval: u64,

        /// Only show memories after this UUID
        #[arg(short, long)]
        since: Option<String>,

        /// Run command for each new memory (supports {uuid}, {gist}, {content}, {created_at}, {parent}, {json} placeholders)
        #[arg(short, long)]
        exec: Option<String>,

        /// Check once and exit (exit code 0 = new memories, 1 = none)
        #[arg(long)]
        once: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Build embedding cache for semantic search
    #[command(
        after_help = "Embeds all content using ONNX (paraphrase-multilingual-MiniLM-L12-v2).\nFirst run downloads the model (~118MB). Subsequent runs only process new memories."
    )]
    Backfill,

    /// Show usage guide for agents
    Skill,

    /// Show station stats
    Status,

    /// MCP server for Claude Desktop — run, install, or check status
    #[command(subcommand)]
    Mcp(McpCommand),
}

#[derive(Subcommand)]
pub enum McpCommand {
    /// Run as MCP server (stdio transport — used by Claude Desktop internally)
    Serve,

    /// Install Geniuz into Claude Desktop config
    #[command(
        after_help = "Adds Geniuz as an MCP server in Claude Desktop's config file.\nAfter running this, restart Claude Desktop to activate.\n\nYour Claude will have three new tools: remember, recall, recent."
    )]
    Install,

    /// Check if Geniuz is configured in Claude Desktop
    Status,
}

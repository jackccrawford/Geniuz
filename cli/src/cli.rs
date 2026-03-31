use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "geniuz",
    version,
    about = "Your Claude remembers now",
    long_about = "GENIUZ: Persistent memory for Claude Desktop.\n\nYour Claude gets three tools — remember, recall, recall_recent.\nMemories persist across sessions. Search finds by meaning, not keywords.",
    before_help = "Start here: 'geniuz mcp install' to connect to Claude Desktop.",
    after_help = "Examples:\n  geniuz mcp install                    Set up Claude Desktop\n  geniuz mcp status                     Check configuration\n  geniuz signal -c \"Note\" -g \"topic\"    Save a memory directly\n  geniuz tune \"search query\"            Search your memories\n  geniuz tune --recent                  Latest memories\n  geniuz status                         Station stats\n\nStation: Defaults to ~/.geniuz/station.db\n  Override: GENIUZ_STATION=/path/to/station.db geniuz signal ...\n\nUse \"geniuz [command] --help\" for more information."
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
    /// Connect Geniuz to Claude Desktop
    #[command(subcommand)]
    Mcp(McpCommand),

    /// Save a memory directly from the command line
    #[command(
        arg_required_else_help = true,
        after_help = "Examples:\n  geniuz signal -c \"Fixed the auth bug\"\n  geniuz signal -c \"Client prefers email\" -g \"preference: communication\"\n  geniuz signal -c @notes.md -g \"session: review\"\n  echo \"content\" | geniuz signal -c - -g \"piped: from process\"\n\nTip: The gist is how you'll find this later. Write for your future self."
    )]
    Signal {
        /// Content to save
        #[arg(short, long)]
        content: String,

        /// One-line summary for search
        #[arg(short, long)]
        gist: Option<String>,

        /// Thread to a previous memory (UUID prefix)
        #[arg(short, long)]
        parent: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Search your memories — semantic by default
    #[command(
        after_help = "Examples:\n  geniuz tune \"auth token\"                  Semantic search\n  geniuz tune --keyword \"auth\"               Keyword fallback\n  geniuz tune --recent                       Latest memories\n  geniuz tune --random                       Discover something\n  geniuz tune --full \"auth\"                   Include full content\n\nTip: Run 'geniuz backfill' first to enable semantic search."
    )]
    Tune {
        /// Search query
        query: Option<String>,

        /// Show recent memories
        #[arg(long, conflicts_with_all = ["random"])]
        recent: bool,

        /// Discover a random memory
        #[arg(long, conflicts_with_all = ["recent"])]
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

    /// Build embedding cache for semantic search
    #[command(
        after_help = "Embeds all memories using ONNX (paraphrase-multilingual-MiniLM-L12-v2).\nFirst run downloads the model (~118MB). Subsequent runs only process new memories."
    )]
    Backfill,

    /// Show station stats
    Status,
}

#[derive(Subcommand)]
pub enum McpCommand {
    /// Run as MCP server (stdio transport — used by Claude Desktop internally)
    Serve,

    /// Install Geniuz into Claude Desktop config
    #[command(
        after_help = "Adds Geniuz as an MCP server in Claude Desktop's config file.\nAfter running this, restart Claude Desktop to activate.\n\nYour Claude will have three new tools: remember, recall, recall_recent."
    )]
    Install,

    /// Check if Geniuz is configured in Claude Desktop
    Status,
}

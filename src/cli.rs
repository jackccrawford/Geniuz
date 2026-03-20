use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "clawmark",
    version,
    about = "Continuity for AI agents",
    long_about = "CLAWMARK: Continuity for AI agents.\n\nSemantic search across sessions. Works with any agent framework:\nOpenClaw, Claude Code, Aider, Cursor, LangChain, custom agents —\nanything that can run a shell command.\n\nMultiple agents can share a station for coordinated continuity.",
    after_help = "Examples:\n  clawmark signal -c \"Fixed the auth bug\" -g \"fix: token refresh\"\n  clawmark tune \"auth\"                       Semantic search\n  clawmark tune --recent                     Latest signals\n  clawmark capture ./notes/                  Bulk-load markdown files\n  clawmark capture --openclaw                Import OpenClaw memory\n  clawmark backfill                          Build embedding cache\n\nStation: Defaults to ~/.clawmark/station.db\n  Override: CLAWMARK_STATION=/path/to/shared.db clawmark signal ...\n  Multiple agents can write to the same station for shared memory.\n\nUse \"clawmark [command] --help\" for more information."
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
    /// Capture files or directories into your station
    #[command(
        arg_required_else_help = true,
        after_help = "Examples:\n  clawmark capture notes.md                              Single file\n  clawmark capture ./docs/                               All .md files in directory\n  clawmark capture *.md                                  Shell glob\n  clawmark capture --split notes.md                      Split by ## headers\n  clawmark capture --gist-prefix \"docs:\" a.md            Prefix all gists\n  clawmark capture --openclaw ~/.openclaw/workspace      Import OpenClaw memory\n  clawmark capture --dry-run ./notes/                    Preview without importing\n\nEach file becomes a signal. With --split, each ## section becomes\na threaded signal under the file's root signal.\n\nWith --openclaw, reads MEMORY.md and memory/YYYY-MM-DD.md files,\npreserving timestamps and threading daily sections."
    )]
    Capture {
        /// Files or directories to capture (not used with --openclaw)
        paths: Vec<String>,

        /// Import an OpenClaw workspace
        #[arg(long, conflicts_with = "paths")]
        openclaw: Option<Option<String>>,

        /// Split files by ## headers into threaded signals
        #[arg(long)]
        split: bool,

        /// Prefix for auto-generated gists
        #[arg(long)]
        gist_prefix: Option<String>,

        /// Preview what would be captured without writing
        #[arg(long)]
        dry_run: bool,
    },

    /// Transmit a signal to your station
    #[command(
        arg_required_else_help = true,
        after_help = "Examples:\n  clawmark signal -c \"Fixed the auth bug\"\n  clawmark signal -c \"Token refresh order\" -g \"fix: auth token refresh\"\n  clawmark signal -c @notes.md -g \"session: review\"\n  echo \"content\" | clawmark signal -c - -g \"piped: from process\"\n\nTip: The gist is how future agents find this signal. Write for them."
    )]
    Signal {
        /// Content to transmit
        #[arg(short, long)]
        content: String,

        /// Compressed insight — how future agents find this
        #[arg(short, long)]
        gist: Option<String>,

        /// Thread to a parent signal (UUID prefix)
        #[arg(short, long)]
        parent: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Search your station — semantic by default
    #[command(
        after_help = "Examples:\n  clawmark tune \"auth token\"                  Semantic search\n  clawmark tune --keyword \"auth\"               Keyword fallback\n  clawmark tune --recent                       Latest signals\n  clawmark tune --random                       Discover something\n  clawmark tune --full \"auth\"                   Include full content\n\nTip: Run 'clawmark backfill' first to enable semantic search."
    )]
    Tune {
        /// Search query
        query: Option<String>,

        /// Show recent signals
        #[arg(long, conflicts_with_all = ["random"])]
        recent: bool,

        /// Discover a random signal
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
        after_help = "Embeds all signal content using ONNX (paraphrase-multilingual-MiniLM-L12-v2).\nFirst run downloads the model (~118MB). Subsequent runs only process new signals."
    )]
    Backfill,

    /// Show usage guide for agents
    Skill,

    /// Show station stats
    Status,
}

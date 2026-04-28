mod audit;
mod commands;
mod sandbox;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "cato",
    about = "Portable sandbox for secure command execution",
    version,
    after_help = "Examples:\n  cato init                    Create .cato.toml in current project\n  cato run                     Enter sandboxed shell\n  cato run -- npm test         Run single command in sandbox\n  cato tool add node git       Register tools for sandbox\n  cato secret put API_KEY      Store a secret\n  cato status                  Check sandbox readiness\n  cato audit                   View sandbox event log"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create .cato.toml in the current directory
    Init {
        /// Minimal: secrets and keys only
        #[arg(long)]
        minimal: bool,

        /// Strict: default + read-only infra + scope containment
        #[arg(long)]
        strict: bool,

        /// Overwrite existing .cato.toml
        #[arg(long)]
        force: bool,
    },

    /// Enter sandboxed shell (current directory = workspace)
    Run {
        /// Run single command instead of interactive shell
        #[arg(last = true)]
        command: Vec<String>,

        /// Ephemeral: changes don't persist (copies workspace)
        #[arg(long)]
        ephemeral: bool,
    },

    /// Show sandbox readiness for current project
    Status,

    /// Manage tool binaries for sandbox
    Tool {
        #[command(subcommand)]
        action: ToolAction,
    },

    /// Manage secrets for sandbox
    Secret {
        #[command(subcommand)]
        action: SecretAction,
    },

    /// View the sandbox audit log
    Audit {
        /// Number of entries to show
        #[arg(short = 'n', long, default_value = "20")]
        count: usize,

        /// Follow new entries in real-time
        #[arg(short, long)]
        follow: bool,

        /// Filter entries (matches any field)
        #[arg(short = 'q', long)]
        filter: Option<String>,

        /// Show entries from all projects (default: current project only)
        #[arg(short, long)]
        all: bool,
    },
}

#[derive(Subcommand)]
enum ToolAction {
    /// Register a tool binary for sandbox use
    Add {
        /// Tool name (e.g., node, python3, git)
        name: String,
        /// Explicit path to binary
        #[arg(long)]
        path: Option<String>,
    },
    /// List registered tools
    List,
    /// Remove a registered tool
    Remove {
        /// Tool name
        name: String,
    },
}

#[derive(Subcommand)]
enum SecretAction {
    /// Store a secret value
    Put {
        /// NAME or NAME=value
        name_value: String,
        /// Store as project-scoped override (uses current directory)
        #[arg(long)]
        project: bool,
    },
    /// List stored secrets (names only, values masked)
    List,
    /// Remove a stored secret
    Remove {
        /// Secret name
        name: String,
        /// Remove project-scoped override
        #[arg(long)]
        project: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { minimal, strict, force } => commands::init::run(minimal, strict, force),
        Commands::Run { command, ephemeral } => {
            let cmd = if command.is_empty() { None } else { Some(command) };
            commands::run::run(cmd, ephemeral);
        },
        Commands::Tool { action } => match action {
            ToolAction::Add { name, path } => commands::tool::add(&name, path.as_deref()),
            ToolAction::List => commands::tool::list(),
            ToolAction::Remove { name } => commands::tool::remove(&name),
        },
        Commands::Secret { action } => match action {
            SecretAction::Put { name_value, project } => commands::secret::put(&name_value, project),
            SecretAction::List => commands::secret::list(),
            SecretAction::Remove { name, project } => commands::secret::remove(&name, project),
        },
        Commands::Status => commands::status::run(),
        Commands::Audit { count, follow, filter, all } => {
            commands::audit::run(count, follow, filter.as_deref(), all)
        }
    }
}

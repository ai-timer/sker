use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod commands;

#[derive(Parser)]
#[command(name = "sker", version, about = "Offline-first project policy tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Compile {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },
    Check {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long)]
        ci: bool,
        #[arg(long, requires = "ci")]
        sarif: Option<PathBuf>,
        #[arg(long, requires = "ci")]
        json: Option<PathBuf>,
        #[arg(long)]
        base: Option<String>,
        #[arg(long)]
        credential: Option<PathBuf>,
    },
    Consult {
        intent: String,
        paths: Vec<String>,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },
    Search {
        query: String,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },
    Attest {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long, default_value = "{}")]
        semantic: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Propose {
        entry: PathBuf,
        rationale: String,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },
    Replay {
        from: String,
        to: String,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },
    Init {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },
    Doctor {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },
    Baseline {
        #[command(subcommand)]
        operation: BaselineCommand,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },
    Mcp {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },
    Hook {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long)]
        hard_block: bool,
    },
}
#[derive(Subcommand)]
enum BaselineCommand {
    Freeze,
    List,
    Update,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(error) = commands::run(cli.command).await {
        eprintln!("sker: {error:#}");
        std::process::exit(2);
    }
}

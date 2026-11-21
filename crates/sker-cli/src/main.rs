use clap::Parser;
use sker_cli::Cli;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(error) = sker_cli::run(cli.command).await {
        eprintln!("sker: {error:#}");
        std::process::exit(2);
    }
}

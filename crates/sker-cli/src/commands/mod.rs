mod baseline;
mod check;
mod compile;
mod doctor;
mod init;
mod interaction;
mod support;

use crate::Command;
use anyhow::Result;

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Compile { repo } => compile::run(&repo),
        Command::Check {
            repo,
            ci,
            sarif,
            json,
            base,
            credential,
        } => check::run(
            &repo,
            ci,
            sarif.as_deref(),
            json.as_deref(),
            base.as_deref(),
            credential.as_deref(),
        ),
        Command::Consult {
            intent,
            paths,
            repo,
        } => interaction::consult(&repo, &intent, &paths),
        Command::Search { query, repo } => interaction::search(&repo, &query),
        Command::Attest {
            repo,
            semantic,
            output,
        } => interaction::attest(&repo, &semantic, output.as_deref()),
        Command::Propose {
            entry,
            rationale,
            repo,
        } => interaction::propose(&repo, &entry, &rationale),
        Command::Replay { from, to, repo } => interaction::replay(&repo, &from, &to),
        Command::Init { repo } => init::run(&repo),
        Command::Doctor { repo } => doctor::run(&repo),
        Command::Baseline { operation, repo } => baseline::run(&repo, operation),
        Command::Mcp { repo } => interaction::mcp(&repo),
        Command::Hook { repo, hard_block } => interaction::hook(&repo, hard_block),
    }
}

pub use support::{atomic_json, bundle, create_bundle, find_repo, load_baseline, now};

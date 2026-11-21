mod baseline;
mod check;
mod compile;
mod doctor;
mod init;
mod interaction;
mod support;
pub mod telemetry;

use crate::Command;
use anyhow::Result;

pub async fn run(command: Command) -> Result<()> {
    match command {
        Command::Compile {
            repo,
            playbook,
            out,
            update_lock,
        } => compile::run(&repo, playbook.as_deref(), out.as_deref(), update_lock),
        Command::Check {
            bundle,
            repo,
            ci,
            sarif,
            json,
            base,
            credential,
        } => check::run(
            &repo,
            bundle.as_deref(),
            ci,
            sarif.as_deref(),
            json.as_deref(),
            base.as_deref(),
            credential.as_deref(),
        ),
        Command::Consult {
            bundle,
            intent,
            named_paths,
            pos_intent,
            pos_paths,
            repo,
        } => {
            let actual_intent = intent.or(pos_intent).unwrap_or_default();
            let actual_paths = if !named_paths.is_empty() {
                named_paths
            } else {
                pos_paths
            };
            interaction::consult(&repo, bundle.as_deref(), &actual_intent, &actual_paths)
        }
        Command::Search {
            bundle,
            query,
            pos_query,
            repo,
        } => {
            let actual_query = query.or(pos_query).unwrap_or_default();
            interaction::search(&repo, bundle.as_deref(), &actual_query)
        }
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
        Command::Mcp { repo } => interaction::mcp(&repo).await,
        Command::Hook { repo, hard_block } => interaction::hook(&repo, hard_block),
    }
}

pub use support::{atomic_json, bundle, find_repo, load_baseline, now};

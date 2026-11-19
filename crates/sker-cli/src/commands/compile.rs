use super::{atomic_json, find_repo};
use anyhow::Result;
use std::path::Path;

pub fn run(path: &Path) -> Result<()> {
    let repo = find_repo(path)?;
    let rules = super::support::load_rules(&repo)?;
    let b = sker_compiler::compile_repository_rules(rules, env!("CARGO_PKG_VERSION").into())?;
    std::fs::create_dir_all(repo.join(".sker"))?;
    atomic_json(&repo.join(".sker/bundle.json"), &b)?;
    sker_compiler::publish_bundle(&b, repo.join(".sker/bundle.rkyv"))?;
    let _ = sker_compiler::write_projections(&b, &repo);
    println!(
        "compiled {} rules; sha256 {}",
        b.rules.len(),
        b.bundle_sha256
    );
    Ok(())
}

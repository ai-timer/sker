use super::{atomic_json, create_bundle};
use anyhow::Result;
use std::{fs, path::Path};
pub fn run(path: &Path) -> Result<()> {
    let repo = super::find_repo(path)?;
    let b = create_bundle(&repo)?;
    fs::create_dir_all(repo.join(".sker"))?;
    atomic_json(&repo.join(".sker/bundle.json"), &b)?;
    let bytes = sker_schema::archive_bundle(&b)?;
    let tmp = repo.join(".sker/bundle.rkyv.tmp");
    fs::write(&tmp, &bytes)?;
    fs::rename(tmp, repo.join(".sker/bundle.rkyv"))?;
    println!(
        "compiled {} rules; sha256 {}",
        b.rules.len(),
        b.bundle_sha256
    );
    Ok(())
}

use super::{atomic_json, bundle, find_repo, load_baseline, now};
use crate::BaselineCommand;
use anyhow::{Context, Result};
use sker_schema::{AnalysisRequest, DiffAnalyzer, DiffEntry, DiffStatus};
use std::{fs, path::Path, process::Command};
pub fn run(path: &Path, op: BaselineCommand) -> Result<()> {
    let repo = find_repo(path)?;
    let file = repo.join(".sker/baseline.json");
    let mut items = load_baseline(&repo)?;
    match op {
        BaselineCommand::List => println!("{}", serde_json::to_string_pretty(&items)?),
        BaselineCommand::Freeze | BaselineCommand::Update => {
            let b = bundle(&repo)?;
            let output = Command::new("git")
                .current_dir(&repo)
                .args(["ls-files", "-z"])
                .output()?;
            if !output.status.success() {
                anyhow::bail!("git ls-files failed; baseline requires a repository")
            };
            let mut diff = Vec::new();
            for raw in output.stdout.split(|b| *b == 0).filter(|p| !p.is_empty()) {
                let path = String::from_utf8_lossy(raw).into_owned();
                let content = fs::read(repo.join(&path))
                    .with_context(|| format!("reading tracked file {path}"))?;
                let lines = content.split(|b| *b == b'\n').count() as u32;
                diff.push(DiffEntry {
                    path,
                    status: DiffStatus::Added,
                    old_mode: None,
                    new_mode: Some(0o100644),
                    old_blob: vec![],
                    new_blob: content,
                    added_lines: (1..=lines).collect(),
                    modified_lines: vec![],
                });
            }
            let req = AnalysisRequest {
                repository_root: repo.to_string_lossy().into_owned(),
                base_revision: "<empty-tree>".into(),
                head_revision: "WORKTREE".into(),
                diff,
                baseline: items.clone(),
                now: now(),
            };
            let report = sker_analyzer::Analyzer.analyze(&b, &req)?;
            if matches!(op, BaselineCommand::Freeze) {
                items.clear()
            }
            for f in report.findings {
                if f.track == sker_schema::RuleTrack::Deterministic {
                    items.push(sker_schema::BaselineItem {
                        repository_path: f.path,
                        rule_id: f.rule_id,
                        syntax_anchor: None,
                        symbol: None,
                        normalized_fragment: f.message,
                        line_hint: Some(f.start_line),
                        exception: None,
                        audit: vec![format!("frozen at {}", now())],
                    });
                }
            }
            items.sort_by(|a, b| a.identity().cmp(&b.identity()));
            items.dedup_by(|a, b| a.identity() == b.identity());
            fs::create_dir_all(file.parent().unwrap())?;
            atomic_json(&file, &items)?;
            println!(
                "wrote {} stable baseline anchors to {}",
                items.len(),
                file.display()
            );
        }
    }
    Ok(())
}

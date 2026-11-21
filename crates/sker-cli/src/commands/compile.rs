use super::{atomic_json, find_repo};
use anyhow::Result;
use std::path::Path;

pub fn run(
    repo: &Path,
    playbook: Option<&Path>,
    out: Option<&Path>,
    update_lock: bool,
) -> Result<()> {
    let src = if let Some(pb) = playbook {
        find_repo(pb)?
    } else {
        find_repo(repo)?
    };

    let (mut sources, _has_federated) = sker_compiler::load_federated_sources(&src, update_lock)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let local_rules = super::support::load_rules(&src)?;
    let local_hash = sker_schema::compute_bundle_sha256(&[], &local_rules)?;
    let local_lock = sker_schema::SourceLock {
        kind: sker_schema::RuleSourceKind::RepositoryLocal,
        source_id: "repository".into(),
        version: "working-tree".into(),
        content_hash: local_hash,
    };
    sources.push(sker_schema::RuleSource {
        lock: local_lock,
        rules: local_rules,
    });

    let req = sker_schema::CompileRequest {
        sources,
        compiler_version: env!("CARGO_PKG_VERSION").into(),
    };
    let b = sker_compiler::compile(&req).map_err(|e| anyhow::anyhow!("{e}"))?;

    let target_dir = if let Some(o) = out {
        std::fs::create_dir_all(o)?;
        o.to_path_buf()
    } else {
        let d = src.join(".sker");
        std::fs::create_dir_all(&d)?;
        d
    };
    atomic_json(&target_dir.join("bundle.json"), &b)?;
    let rkyv_path = target_dir.join("bundle.rkyv");
    sker_compiler::publish_bundle(&b, &rkyv_path)?;
    std::fs::write(
        target_dir.join("bundle.sha256"),
        format!("{}\n", b.bundle_sha256),
    )?;
    let _ = sker_compiler::write_projections(&b, &src);

    // Lifecycle warnings for rules
    let today = super::now();
    for rule in &b.rules {
        if let Some(ref rev) = rule.lifecycle.review_by {
            let is_past = rev.as_str() < today.as_str();
            if is_past {
                eprintln!(
                    "warning: rule '{}' review date {} has passed",
                    rule.rule_id, rev
                );
            }
        }
        if rule.lifecycle.kind == sker_schema::LifecycleKind::IncidentHotfix
            && rule
                .lifecycle
                .hotfix_expires_at
                .as_deref()
                .is_some_and(|exp| exp < today.as_str())
        {
            eprintln!(
                "warning: rule '{}' incident hotfix expired at {}",
                rule.rule_id,
                rule.lifecycle.hotfix_expires_at.as_deref().unwrap_or("")
            );
        }
    }

    println!(
        "compiled {} rules; bundle: {}; sha256: {}",
        b.rules.len(),
        rkyv_path.display(),
        b.bundle_sha256
    );
    super::telemetry::report_event_async(&src, "compile", &b.bundle_sha256, 0, "pass");
    Ok(())
}

use sker_schema::RuleBundle;
use std::{collections::BTreeMap, fs, path::Path};
const START: &str = "<!-- sker:managed:start -->";
const END: &str = "<!-- sker:managed:end -->";
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Projection {
    pub agents_md: String,
    pub claude_md: String,
    pub skills: BTreeMap<String, String>,
}
fn managed_block(bundle: &RuleBundle) -> String {
    let mut lines = vec![
        START.to_string(),
        "1. Read the matched rule and its scope.".to_string(),
        "2. Apply the rule, then run its positive and negative fixtures.".to_string(),
    ];
    for rule in bundle
        .rules
        .iter()
        .filter(|r| r.tags.iter().any(|t| t == "pinned_to_l1"))
        .take(3)
    {
        lines.push(format!("- pinned_to_l1: {} — {}", rule.rule_id, rule.title));
    }
    lines.push(END.into());
    lines.join("\n")
}
fn token_count(text: &str) -> usize {
    text.split_whitespace().count()
}
fn replace_managed(existing: &str, block: &str) -> String {
    if let (Some(start), Some(end)) = (existing.find(START), existing.find(END))
        && end >= start
    {
        let after = end + END.len();
        return format!("{}{}{}", &existing[..start], block, &existing[after..]);
    }
    if existing.is_empty() {
        block.into()
    } else {
        format!("{}\n\n{}", existing.trim_end(), block)
    }
}
pub fn project_guidance(
    bundle: &RuleBundle,
    root: impl AsRef<Path>,
) -> Result<Projection, std::io::Error> {
    let block = managed_block(bundle);
    assert!(
        token_count(&block) <= 300,
        "managed guidance exceeds 300 tokens"
    );
    let root = root.as_ref();
    let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap_or_default();
    let claude = fs::read_to_string(root.join("CLAUDE.md")).unwrap_or_default();
    let agents_md = replace_managed(&agents, &block);
    let claude_md = replace_managed(&claude, &block);
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for rule in &bundle.rules {
        groups
            .entry(rule.domain.clone())
            .or_default()
            .push(format!("# {}\n\n{}\n", rule.rule_id, rule.description));
    }
    let skills = groups
        .into_iter()
        .map(|(domain, content)| {
            (
                format!(
                    ".agents/skills/sker-{}/SKILL.md",
                    domain.replace(['/', '.', '_'], "-")
                ),
                content.join("\n"),
            )
        })
        .collect();
    Ok(Projection {
        agents_md,
        claude_md,
        skills,
    })
}

/// Writes the generated guidance and per-domain skills below a repository root.
pub fn write_projections(
    bundle: &RuleBundle,
    root: impl AsRef<Path>,
) -> Result<Projection, std::io::Error> {
    let root = root.as_ref();
    let projection = project_guidance(bundle, root)?;
    fs::write(root.join("AGENTS.md"), &projection.agents_md)?;
    fs::write(root.join("CLAUDE.md"), &projection.claude_md)?;

    // Synchronize Antigravity rule projections idempotently
    let block = managed_block(bundle);
    let mut wrote_agent_rule = false;
    for rel in [".agent/rules/sker.md", ".agents/rules/sker.md"] {
        let p = root.join(rel);
        if p.exists() || p.parent().is_some_and(|parent| parent.exists()) {
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent)?;
            }
            let prior = fs::read_to_string(&p).unwrap_or_default();
            let updated = replace_managed(&prior, &block);
            fs::write(p, updated)?;
            wrote_agent_rule = true;
        }
    }
    if !wrote_agent_rule {
        let default_rule = root.join(".agent/rules/sker.md");
        if let Some(parent) = default_rule.parent() {
            fs::create_dir_all(parent)?;
        }
        let updated = replace_managed("", &block);
        fs::write(default_rule, updated)?;
    }

    // Exact delta cleanup: only clean up obsolete `sker-*` skill directories
    let active_skill_dirs: std::collections::BTreeSet<String> = bundle
        .rules
        .iter()
        .map(|r| format!("sker-{}", r.domain.replace(['/', '.', '_'], "-")))
        .collect();

    for skills_dir_rel in [".agents/skills", ".agent/skills"] {
        let dir = root.join(skills_dir_rel);
        if dir.is_dir()
            && let Ok(entries) = fs::read_dir(&dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir()
                    && let Some(name) = path.file_name().and_then(|n| n.to_str())
                {
                    // Strictly clean up sker-* directories not in active set, leave other tools like speckit-* intact
                    if name.starts_with("sker-") && !active_skill_dirs.contains(name) {
                        let _ = fs::remove_dir_all(&path);
                    }
                }
            }
        }
    }

    for (relative, content) in &projection.skills {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
    }
    Ok(projection)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sker_schema::{BundleMetadata, RuleBundle, RuleEntry};

    fn make_test_bundle(domains: &[&str]) -> RuleBundle {
        let rules = domains
            .iter()
            .enumerate()
            .map(|(i, &d)| RuleEntry {
                schema_version: "sker.entry/v1".into(),
                rule_id: format!("test.rule.{i}"),
                version: "1.0.0".into(),
                title: format!("Test Rule {i}"),
                domain: d.into(),
                description: format!("Description for rule {i}"),
                rationale: None,
                severity: sker_schema::RuleSeverity::Block,
                track: sker_schema::RuleTrack::Deterministic,
                scope: sker_schema::EntryScope {
                    include_paths: vec!["**/*".into()],
                    exclude_paths: vec![],
                },
                signals: vec![],
                detector: None,
                ast_rule: None,
                detector_content: None,
                exemptions: vec![],
                overrides: vec![],
                fixtures: vec![],
                compliant_alternatives: Some(vec![]),
                lifecycle: sker_schema::Lifecycle {
                    kind: sker_schema::LifecycleKind::Ordinary,
                    cooldown_hours: Some(24),
                    hotfix_expires_at: None,
                    review_by: None,
                },
                consult: sker_schema::ConsultMetadata {
                    synonyms: vec![],
                    keywords: vec![],
                },
                tags: vec![],
            })
            .collect();

        RuleBundle {
            schema_version: "sker.bundle/v1".into(),
            bundle_id: "test".into(),
            bundle_sha256: "0".repeat(64),
            sources: vec![],
            rules,
            metadata: BundleMetadata {
                created_at: "2026-11-21T00:00:00Z".into(),
                compiler_version: "test".into(),
            },
        }
    }

    #[test]
    fn write_projections_cleans_only_obsolete_sker_skills_and_preserves_others() {
        let temp = std::env::temp_dir().join(format!("sker-test-proj-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let skills_dir = temp.join(".agents/skills");
        fs::create_dir_all(skills_dir.join("speckit-custom")).unwrap();
        fs::write(
            skills_dir.join("speckit-custom/SKILL.md"),
            "# User Custom Skill",
        )
        .unwrap();

        fs::create_dir_all(skills_dir.join("sker-obsolete-domain")).unwrap();
        fs::write(
            skills_dir.join("sker-obsolete-domain/SKILL.md"),
            "# Obsolete Sker Skill",
        )
        .unwrap();

        let bundle = make_test_bundle(&["active-domain"]);
        write_projections(&bundle, &temp).unwrap();

        assert!(
            skills_dir.join("speckit-custom/SKILL.md").exists(),
            "Non-sker user skills must be preserved"
        );
        assert!(
            !skills_dir.join("sker-obsolete-domain").exists(),
            "Obsolete sker-* skills must be deleted"
        );
        assert!(
            skills_dir.join("sker-active-domain/SKILL.md").exists(),
            "Active sker-* skills must be generated"
        );
        assert!(
            temp.join(".agent/rules/sker.md").exists(),
            "Antigravity rules must be projected"
        );

        let _ = fs::remove_dir_all(temp);
    }
}

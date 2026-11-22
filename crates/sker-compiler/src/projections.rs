use sker_schema::{AgentConfig, RuleBundle};
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
        "Before changing code: run `sker consult` for the relevant paths.".to_string(),
        "After changes: run `sker check` and resolve deterministic BLOCK findings.".to_string(),
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

/// Writes the generated guidance for each enabled integration below a repository root.
///
/// `AGENTS.md` is always projected; every additional enabled agent contributes its
/// declared `rules_file`. Managed blocks are replaced idempotently.
pub fn write_projections(
    bundle: &RuleBundle,
    root: impl AsRef<Path>,
    agents: &[AgentConfig],
) -> Result<Projection, std::io::Error> {
    let root = root.as_ref();
    let projection = project_guidance(bundle, root)?;
    let block = managed_block(bundle);

    let mut targets: Vec<String> = vec!["AGENTS.md".into()];
    for agent in agents {
        let rules_file = agent.rules_file();
        if !targets.iter().any(|t| t == &rules_file) {
            targets.push(rules_file);
        }
    }

    let mut agents_md = String::new();
    let mut claude_md = fs::read_to_string(root.join("CLAUDE.md")).unwrap_or_default();
    for rel in &targets {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let prior = fs::read_to_string(&path).unwrap_or_default();
        let updated = replace_managed(&prior, &block);
        fs::write(&path, &updated)?;
        if rel == "AGENTS.md" {
            agents_md = updated;
        } else if rel == "CLAUDE.md" {
            claude_md = updated;
        }
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
    Ok(Projection {
        agents_md,
        claude_md,
        skills: projection.skills,
    })
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
        let agents = [
            AgentConfig::Preset("universal".into()),
            AgentConfig::Preset("antigravity".into()),
        ];
        write_projections(&bundle, &temp, &agents).unwrap();

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

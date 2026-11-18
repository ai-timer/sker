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
    for (relative, content) in &projection.skills {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
    }
    Ok(projection)
}

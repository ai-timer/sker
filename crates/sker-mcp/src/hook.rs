use globset::Glob;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sker_schema::RuleSeverity;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct PreToolUseInput {
    #[serde(default)]
    pub hook_event_name: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_input: Option<Value>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HookOutput {
    #[serde(rename = "hookSpecificOutput")]
    pub hook_specific_output: HookSpecificOutput,
    #[serde(skip)]
    pub is_blocking_violation: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String,
    #[serde(rename = "additionalContext")]
    pub additional_context: String,
}

pub fn is_sker_tool(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("sker_")
        || lower.starts_with("mcp__sker__")
        || lower.contains("__sker_")
        || lower.contains("sker_consult")
        || lower.contains("sker_attest")
        || lower.contains("sker_search")
        || lower.contains("sker_propose")
}

pub fn is_readonly_tool(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "read_file"
            | "view"
            | "cat"
            | "grep"
            | "glob"
            | "search"
            | "list_dir"
            | "list_directory"
            | "get_file_contents"
            | "fetch"
    ) || lower.starts_with("read_")
        || lower.starts_with("get_")
        || lower.starts_with("list_")
}

fn glob_matches(pattern: &str, path: &str) -> bool {
    Glob::new(pattern)
        .ok()
        .is_some_and(|g| g.compile_matcher().is_match(path))
}

/// Evaluate a host PreToolUse event without network access or model inference.
pub fn handle(input: &PreToolUseInput) -> HookOutput {
    handle_with_repo(input, None)
}

pub fn handle_with_repo(input: &PreToolUseInput, repo: Option<&Path>) -> HookOutput {
    let name = input.tool_name.as_deref().unwrap_or("");
    let is_sker = is_sker_tool(name);
    let is_readonly = is_readonly_tool(name);

    if is_sker {
        return HookOutput {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: input
                    .hook_event_name
                    .clone()
                    .unwrap_or_else(|| "PreToolUse".into()),
                additional_context: "Sker local policy tools are available; consult rules before editing and keep proposals pending for human review.".into(),
            },
            is_blocking_violation: false,
        };
    }

    let bundle = repo.and_then(|r| {
        let bundle_path = r.join(".sker/bundle.rkyv");
        if bundle_path.exists() {
            let bytes = std::fs::read(&bundle_path).ok()?;
            sker_schema::validate_bundle_archive(&bytes).ok()
        } else {
            None
        }
    });

    let target_path = input
        .tool_input
        .as_ref()
        .and_then(|v| {
            v.get("file_path")
                .or_else(|| v.get("path"))
                .or_else(|| v.get("target_path"))
        })
        .and_then(Value::as_str);

    let mut context = String::new();
    let mut is_blocking_violation = false;

    if let (Some(bundle), Some(path)) = (&bundle, target_path) {
        let mut matched_rules = Vec::new();
        let mut has_block = false;

        for rule in &bundle.rules {
            let matches_include = rule
                .scope
                .include_paths
                .iter()
                .any(|p| glob_matches(p, path));
            let matches_exclude = rule
                .scope
                .exclude_paths
                .iter()
                .any(|p| glob_matches(p, path));
            let is_exempt = rule
                .exemptions
                .iter()
                .any(|e| e.paths.iter().any(|p| glob_matches(p, path)));

            if matches_include && !matches_exclude && !is_exempt {
                matched_rules.push(rule.rule_id.clone());
                if rule.severity == RuleSeverity::Block {
                    has_block = true;
                }
            }
        }

        if !matched_rules.is_empty() {
            context = format!(
                "High-risk path '{}' matches active Sker guardrail(s): {}. Consult applicable Sker rules before proceeding.",
                path,
                matched_rules.join(", ")
            );
            if has_block && !is_readonly {
                is_blocking_violation = true;
            }
        }
    } else if let Some(path) = target_path {
        let is_sensitive =
            path.contains("auth") || path.contains("security") || path.contains("credential");
        if is_sensitive {
            context =
                "High-risk security path: consult applicable Sker rules before proceeding.".into();
            if !is_readonly {
                is_blocking_violation = true;
            }
        }
    }

    HookOutput {
        hook_specific_output: HookSpecificOutput {
            hook_event_name: input
                .hook_event_name
                .clone()
                .unwrap_or_else(|| "PreToolUse".into()),
            additional_context: context,
        },
        is_blocking_violation,
    }
}

/// Adapter reports delivery separately so hosts can distinguish an empty advisory from I/O failure.
pub fn write_response(
    input: &PreToolUseInput,
    mut output: impl std::io::Write,
) -> Result<(), std::io::Error> {
    serde_json::to_writer(&mut output, &handle(input)).map_err(std::io::Error::other)?;
    output.write_all(b"\n")
}

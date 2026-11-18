use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct PreToolUseInput {
    #[serde(default)] pub hook_event_name: Option<String>,
    #[serde(default)] pub tool_name: Option<String>,
    #[serde(default)] pub tool_input: Option<Value>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HookOutput { #[serde(rename = "hookSpecificOutput")] pub hook_specific_output: HookSpecificOutput }
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HookSpecificOutput { #[serde(rename = "hookEventName")] pub hook_event_name: String, #[serde(rename = "additionalContext")] pub additional_context: String }

/// Evaluate a host PreToolUse event without network access or model inference.
pub fn handle(input: &PreToolUseInput) -> HookOutput {
    let name = input.tool_name.as_deref().unwrap_or("");
    let context = if name.starts_with("sker_") { "Sker local policy tools are available; consult rules before editing and keep proposals pending for human review." }
    else if let Some(paths) = input.tool_input.as_ref().and_then(|v| v.get("file_path")).and_then(Value::as_str) {
        if paths.contains("auth") || paths.contains("security") || paths.contains("credential") { "High-risk security path: consult applicable Sker rules before proceeding." } else { "" }
    } else { "" };
    HookOutput { hook_specific_output: HookSpecificOutput { hook_event_name: input.hook_event_name.clone().unwrap_or_else(|| "PreToolUse".into()), additional_context: context.into() } }
}

/// Adapter reports delivery separately so hosts can distinguish an empty advisory from I/O failure.
pub fn write_response(input: &PreToolUseInput, mut output: impl std::io::Write) -> Result<(), std::io::Error> {
    serde_json::to_writer(&mut output, &handle(input)).map_err(std::io::Error::other)?;
    output.write_all(b"\n")
}

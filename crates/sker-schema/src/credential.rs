use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{RetrievalError, RuleEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CredentialIssuer {
    LocalAgent,
    LocalCli,
    Mcp,
    Hook,
    SkerAttest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConsultResultStatus {
    Matched,
    NoMatch,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConsultTraceEntry {
    pub consult_id: String,
    pub occurred_at: String,
    pub query_digest: String,
    pub paths: Vec<String>,
    pub matched_rule_ids: Vec<String>,
    pub result_status: ConsultResultStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SemanticVerdictState {
    Comply,
    NotTriggeredInLogic,
    Exempt,
    Escalate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SemanticVerdict {
    pub rule_id: String,
    pub state: SemanticVerdictState,
    pub declaration: String,
    pub rationale_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentCredential {
    pub schema_version: String,
    pub bundle_sha256: String,
    pub diff_tree_hash: String,
    pub consult_trace: Vec<ConsultTraceEntry>,
    pub semantic_verdicts: Vec<SemanticVerdict>,
    pub issuer: CredentialIssuer,
    pub created_at: String,
    pub credential_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub proposal_id: String,
    pub state: ProposalState,
    pub entry: RuleEntry,
    pub rationale: String,
    pub based_on_bundle_sha256: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProposalState {
    Pending,
    InReview,
    Merged,
}

pub fn canonical_json_bytes(value: &Value) -> Result<Vec<u8>, serde_json::Error> {
    fn encode(value: &Value, output: &mut String) -> Result<(), serde_json::Error> {
        match value {
            Value::Null => output.push_str("null"),
            Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
            Value::Number(value) => {
                let number = value.to_string();
                output.push_str(if number == "-0" { "0" } else { &number });
            }
            Value::String(value) => output.push_str(&serde_json::to_string(value)?),
            Value::Array(values) => {
                output.push('[');
                for (index, item) in values.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    encode(item, output)?;
                }
                output.push(']');
            }
            Value::Object(object) => {
                output.push('{');
                let mut entries: Vec<_> = object.iter().collect();
                entries
                    .sort_by(|(left, _), (right, _)| left.encode_utf16().cmp(right.encode_utf16()));
                for (index, (key, item)) in entries.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    output.push_str(&serde_json::to_string(key)?);
                    output.push(':');
                    encode(item, output)?;
                }
                output.push('}');
            }
        }
        Ok(())
    }
    let mut output = String::new();
    encode(value, &mut output)?;
    Ok(output.into_bytes())
}

pub fn credential_blake3(credential: &AgentCredential) -> Result<String, serde_json::Error> {
    let mut payload = serde_json::to_value(credential)?;
    if let Value::Object(ref mut fields) = payload {
        fields.remove("credential_blake3");
    }
    let bytes = canonical_json_bytes(&payload)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConsultQuery {
    pub intent: String,
    pub paths: Vec<String>,
    pub limit: Option<usize>,
    pub minimum_score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleMatch {
    pub rule_id: String,
    pub title: String,
    pub domain: String,
    pub severity: crate::RuleSeverity,
    pub track: crate::RuleTrack,
    pub paths: Vec<String>,
    pub summary: String,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConsultResponse {
    pub status: ConsultResultStatus,
    pub matches: Vec<RuleMatch>,
    pub bundle_sha256: String,
    pub trace: Option<ConsultTraceEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchQuery {
    pub query: String,
    pub domains: Vec<String>,
    pub paths: Vec<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchStatus {
    Matched,
    NoMatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchResponse {
    pub status: SearchStatus,
    pub rules: Vec<RuleEntry>,
    pub bundle_sha256: String,
}

pub fn validate_query_paths(paths: &[String]) -> Result<(), RetrievalError> {
    for path in paths {
        if path.starts_with('/') || path.split('/').any(|part| part == "..") {
            return Err(RetrievalError::InvalidQuery {
                detail: format!("path must be repository-relative: {path}"),
            });
        }
    }
    Ok(())
}

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{RuleSeverity, RuleTrack};

/// BLAKE3 over a deterministic, path-sorted, length-prefixed diff encoding.
/// The encoding binds paths, status, old/new modes, and old/new blob bytes.
pub fn compute_diff_tree_hash(diff: &[DiffEntry]) -> String {
    fn field(hasher: &mut blake3::Hasher, bytes: &[u8]) {
        hasher.update(&(bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    let mut entries = diff.to_vec();
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"sker.diff-tree/v1\0");
    hasher.update(&(entries.len() as u64).to_be_bytes());
    for entry in entries {
        field(&mut hasher, entry.path.as_bytes());
        let status: &[u8] = match entry.status {
            DiffStatus::Added => b"added",
            DiffStatus::Modified => b"modified",
            DiffStatus::Deleted => b"deleted",
            DiffStatus::Renamed => b"renamed",
            DiffStatus::Copied => b"copied",
            DiffStatus::TypeChanged => b"type_changed",
            DiffStatus::Unmerged => b"unmerged",
        };
        field(&mut hasher, status);
        for mode in [entry.old_mode, entry.new_mode] {
            match mode {
                Some(mode) => field(&mut hasher, &mode.to_be_bytes()),
                None => field(&mut hasher, &[]),
            }
        }
        field(&mut hasher, &entry.old_blob);
        field(&mut hasher, &entry.new_blob);
    }
    hasher.finalize().to_hex().to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnalysisRequest {
    pub repository_root: String,
    pub base_revision: String,
    pub head_revision: String,
    pub diff: Vec<DiffEntry>,
    #[serde(default)]
    pub baseline: Vec<BaselineItem>,
    pub now: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct DiffEntry {
    pub path: String,
    pub status: DiffStatus,
    pub old_mode: Option<u32>,
    pub new_mode: Option<u32>,
    pub old_blob: Vec<u8>,
    pub new_blob: Vec<u8>,
    pub added_lines: Vec<u32>,
    pub modified_lines: Vec<u32>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum DiffStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Unmerged,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BaselineItem {
    pub repository_path: String,
    pub rule_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub syntax_anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub normalized_fragment: String,
    pub line_hint: Option<u32>,
    pub exception: Option<String>,
    pub audit: Vec<String>,
}

impl BaselineItem {
    /// Stable logical identity excludes `line_hint` by design.
    pub fn identity(&self) -> (&str, &str, Option<&str>, Option<&str>, &str) {
        (
            &self.repository_path,
            &self.rule_id,
            self.syntax_anchor.as_deref(),
            self.symbol.as_deref(),
            &self.normalized_fragment,
        )
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum FindingDisposition {
    Blocked,
    Warning,
    PendingSemantic,
    BaselineSuppressed,
    Exempt,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct CheckFinding {
    pub rule_id: String,
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub track: RuleTrack,
    pub severity: RuleSeverity,
    pub disposition: FindingDisposition,
    pub message: String,
    #[serde(default)]
    pub alternatives: Vec<String>,
    pub baseline_anchor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnalysisSummary {
    pub complete: bool,
    pub unsupported_paths: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Passed,
    Blocked,
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CheckReport {
    pub schema_version: String,
    pub status: CheckStatus,
    pub bundle_sha256: String,
    pub diff_tree_hash: String,
    pub changed_line_count: u64,
    pub findings: Vec<CheckFinding>,
    pub analysis: AnalysisSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReplayReport {
    pub from: String,
    pub to: String,
    pub bundle_sha256: String,
    pub complete: bool,
    pub missing_history_reason: Option<String>,
    pub rule_stats: Vec<ReplayRuleStats>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReplayRuleStats {
    pub rule_id: String,
    pub trigger_count: u64,
    pub commit_count: u64,
    pub trigger_rate: Option<f64>,
    pub potential_false_positive_samples: Vec<String>,
}

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompileError {
    #[error(
        "source {source_id} is unavailable (path not found; if referencing an external or central playbook, ensure the directory exists or run `git submodule update --init --recursive` / 路径未找到或未克隆，若引用外部规则源请检查路径或执行 git submodule update)"
    )]
    SourceUnavailable { source_id: String },
    #[error("source lock mismatch for {source_id}")]
    LockMismatch { source_id: String },
    #[error("invalid Entry {rule_id:?}: {detail}")]
    InvalidEntry {
        rule_id: Option<String>,
        detail: String,
    },
    #[error("override cycle: {rule_ids:?}")]
    OverrideCycle { rule_ids: Vec<String> },
    #[error("central BLOCK rule {rule_id} cannot be weakened by {override_id}")]
    CentralBlockDowngrade {
        rule_id: String,
        override_id: String,
    },
    #[error("conflicting rules: {rule_ids:?}")]
    RuleConflict { rule_ids: Vec<String> },
    #[error("fixture {fixture_id} failed for rule {rule_id}")]
    FixtureFailure { rule_id: String, fixture_id: String },
    #[error("bundle serialization failed: {detail}")]
    BundleSerialization { detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AnalysisError {
    #[error("repository unavailable: {detail}")]
    RepositoryUnavailable { detail: String },
    #[error("diff unavailable: {detail}")]
    DiffUnavailable { detail: String },
    #[error("unsupported language in {path}")]
    UnsupportedLanguage { path: String },
    #[error("parse failure in {path}: {detail}")]
    ParseFailure { path: String, detail: String },
    #[error("invalid baseline: {detail}")]
    InvalidBaseline { detail: String },
    #[error("rule {rule_id} evaluation failed: {detail}")]
    RuleEvaluation { rule_id: String, detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RetrievalError {
    #[error("invalid query: {detail}")]
    InvalidQuery { detail: String },
    #[error("bundle mismatch: expected {expected}, got {actual}")]
    BundleMismatch { expected: String, actual: String },
    #[error("retrieval index unavailable: {detail}")]
    IndexUnavailable { detail: String },
    #[error("retrieval index corrupt: {detail}")]
    IndexCorrupt { detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SchemaError {
    #[error("invalid path {path}: {reason}")]
    InvalidPath { path: String, reason: String },
    #[error("invalid digest: {0}")]
    InvalidDigest(String),
    #[error("invalid Entry: {0}")]
    InvalidEntry(String),
}

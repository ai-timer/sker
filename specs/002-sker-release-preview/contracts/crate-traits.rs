//! Normative public crate-boundary contract for the Preview v0.1 workspace.
//!
//! All request, result, and error types below are canonical declarations for
//! `crates/sker-schema` and MUST be re-exported from the `sker_schema` crate
//! root. The error enums shown below belong in that crate, not in each
//! implementation crate. Implementations live in their owning crates; no
//! cross-crate API may define parallel ad-hoc payload types.

use sker_schema::{
    AnalysisRequest, CheckReport, CompileRequest, ConsultQuery, ConsultResponse, RuleBundle,
    SearchQuery, SearchResponse,
};

pub trait BundleCompiler: Send + Sync {
    fn compile(&self, request: &CompileRequest) -> Result<RuleBundle, CompileError>;
}

pub trait DiffAnalyzer: Send + Sync {
    fn analyze(
        &self,
        bundle: &RuleBundle,
        request: &AnalysisRequest,
    ) -> Result<CheckReport, AnalysisError>;
}

pub trait IntentRetriever: Send + Sync {
    fn consult(
        &self,
        bundle: &RuleBundle,
        query: &ConsultQuery,
    ) -> Result<ConsultResponse, RetrievalError>;

    fn search(
        &self,
        bundle: &RuleBundle,
        query: &SearchQuery,
    ) -> Result<SearchResponse, RetrievalError>;
}

/// Shared errors returned across crate boundaries; define in `sker-schema`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    SourceUnavailable { source_id: String },
    LockMismatch { source_id: String },
    InvalidEntry { rule_id: Option<String>, detail: String },
    OverrideCycle { rule_ids: Vec<String> },
    CentralBlockDowngrade { rule_id: String, override_id: String },
    RuleConflict { rule_ids: Vec<String> },
    FixtureFailure { rule_id: String, fixture_id: String },
    BundleSerialization { detail: String },
}

/// Shared errors returned across crate boundaries; define in `sker-schema`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisError {
    RepositoryUnavailable { detail: String },
    DiffUnavailable { detail: String },
    UnsupportedLanguage { path: String },
    ParseFailure { path: String, detail: String },
    InvalidBaseline { detail: String },
    RuleEvaluation { rule_id: String, detail: String },
}

/// Shared errors returned across crate boundaries; define in `sker-schema`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetrievalError {
    InvalidQuery { detail: String },
    BundleMismatch { expected: String, actual: String },
    IndexUnavailable { detail: String },
    IndexCorrupt { detail: String },
}

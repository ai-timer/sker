//! Normative public crate-boundary contract for the Preview v0.1 workspace.
//!
//! All request, result, and error types referenced here are canonical
//! declarations in `crates/sker-schema` and MUST be re-exported from the
//! `sker_schema` crate root. Implementations live in their owning crates; no
//! cross-crate API may define parallel ad-hoc payload types.

use sker_schema::{
    AnalysisError, AnalysisRequest, CheckReport, CompileError, CompileRequest, ConsultQuery,
    ConsultResponse, RetrievalError, RuleBundle, SearchQuery, SearchResponse,
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

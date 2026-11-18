use sker_schema::{
    AnalysisError, AnalysisRequest, AnalysisSummary, BundleCompiler, CheckFinding, CheckReport,
    CheckStatus, CompileError, CompileRequest, ConsultQuery, ConsultResponse, DiffAnalyzer,
    FindingDisposition, IntentRetriever, RetrievalError, RuleBundle, SearchQuery, SearchResponse,
};

#[path = "../../../specs/002-sker-release-preview/contracts/crate-traits.rs"]
#[allow(dead_code)]
mod frozen_contract;

struct Compiler;
impl BundleCompiler for Compiler {
    fn compile(&self, _request: &CompileRequest) -> Result<RuleBundle, CompileError> {
        Err(CompileError::SourceUnavailable {
            source_id: "test".into(),
        })
    }
}

impl frozen_contract::BundleCompiler for Compiler {
    fn compile(&self, request: &CompileRequest) -> Result<RuleBundle, CompileError> {
        <Self as BundleCompiler>::compile(self, request)
    }
}

struct Analyzer;
impl DiffAnalyzer for Analyzer {
    fn analyze(
        &self,
        _bundle: &RuleBundle,
        _request: &AnalysisRequest,
    ) -> Result<CheckReport, AnalysisError> {
        Ok(CheckReport {
            schema_version: "sker.check-report/v1".into(),
            status: CheckStatus::Passed,
            bundle_sha256: "a".repeat(64),
            diff_tree_hash: "b".repeat(64),
            changed_line_count: 0,
            findings: vec![CheckFinding {
                rule_id: "test.rule".into(),
                path: "src/lib.rs".into(),
                start_line: 1,
                end_line: 1,
                track: sker_schema::RuleTrack::Deterministic,
                severity: sker_schema::RuleSeverity::Info,
                disposition: FindingDisposition::Warning,
                message: "test".into(),
                alternatives: vec![],
                baseline_anchor: None,
            }],
            analysis: AnalysisSummary {
                complete: true,
                unsupported_paths: vec![],
                warnings: vec![],
            },
        })
    }
}

impl frozen_contract::DiffAnalyzer for Analyzer {
    fn analyze(
        &self,
        bundle: &RuleBundle,
        request: &AnalysisRequest,
    ) -> Result<CheckReport, AnalysisError> {
        <Self as DiffAnalyzer>::analyze(self, bundle, request)
    }
}

struct Retriever;
impl IntentRetriever for Retriever {
    fn consult(
        &self,
        _bundle: &RuleBundle,
        _query: &ConsultQuery,
    ) -> Result<ConsultResponse, RetrievalError> {
        Err(RetrievalError::InvalidQuery {
            detail: "test".into(),
        })
    }

    fn search(
        &self,
        _bundle: &RuleBundle,
        _query: &SearchQuery,
    ) -> Result<SearchResponse, RetrievalError> {
        Err(RetrievalError::IndexUnavailable {
            detail: "test".into(),
        })
    }
}

impl frozen_contract::IntentRetriever for Retriever {
    fn consult(
        &self,
        bundle: &RuleBundle,
        query: &ConsultQuery,
    ) -> Result<ConsultResponse, RetrievalError> {
        <Self as IntentRetriever>::consult(self, bundle, query)
    }

    fn search(
        &self,
        bundle: &RuleBundle,
        query: &SearchQuery,
    ) -> Result<SearchResponse, RetrievalError> {
        <Self as IntentRetriever>::search(self, bundle, query)
    }
}

#[test]
fn crate_root_exports_match_shared_trait_contract() {
    let _: Box<dyn BundleCompiler> = Box::new(Compiler);
    let _: Box<dyn DiffAnalyzer> = Box::new(Analyzer);
    let _: Box<dyn IntentRetriever> = Box::new(Retriever);
    let _: Box<dyn frozen_contract::BundleCompiler> = Box::new(Compiler);
    let _: Box<dyn frozen_contract::DiffAnalyzer> = Box::new(Analyzer);
    let _: Box<dyn frozen_contract::IntentRetriever> = Box::new(Retriever);
    let _ = ConsultQuery {
        intent: "intent".into(),
        paths: vec!["src/lib.rs".into()],
        limit: Some(5),
        minimum_score: Some(0.1),
    };
}

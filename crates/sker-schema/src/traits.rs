use crate::{
    AnalysisError, AnalysisRequest, CheckReport, ConsultQuery, ConsultResponse, RetrievalError,
    RuleBundle, SearchQuery, SearchResponse,
};

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

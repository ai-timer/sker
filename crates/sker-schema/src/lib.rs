//! Shared, versioned domain types and cross-crate contracts for Sker.
//!
//! Every workspace boundary uses types re-exported from this crate. The
//! archived bundle format is validated with rkyv's safe `access` API before
//! consumers read the archive.

mod agent;
mod analysis;
mod bundle;
mod credential;
mod errors;
mod hash;
mod ledger;
mod rule;
mod traits;

#[cfg(test)]
mod tests;

pub use agent::{AGENT_PRESETS, AgentConfig, AgentPreset, CustomAgentConfig, find_agent_preset};
pub use analysis::{
    AnalysisRequest, AnalysisSummary, BaselineItem, CheckFinding, CheckReport, CheckStatus,
    DiffEntry, DiffStatus, FindingDisposition, ReplayReport, ReplayRuleStats,
    compute_diff_tree_hash,
};
pub use bundle::{
    ArchivedRuleBundle, BundleCompiler, BundleMetadata, CompileRequest, RuleBundle, RuleSource,
    RuleSourceKind, SourceLock, archive_bundle, compute_bundle_sha256, validate_bundle_archive,
};
pub use credential::{
    AgentCredential, ConsultQuery, ConsultResponse, ConsultResultStatus, ConsultTraceEntry,
    CredentialIssuer, Proposal, ProposalState, RuleMatch, SearchQuery, SearchResponse,
    SearchStatus, SemanticVerdict, SemanticVerdictState, canonical_json_bytes, credential_blake3,
    validate_query_paths,
};
pub use errors::{AnalysisError, CompileError, RetrievalError, SchemaError};
pub use hash::{HashChain, HashChainEntry, HashDigest, HashError};
pub use ledger::{
    AgeBucket, AgeDistributionBucket, EventCounters, EventLabels, EventLanguage, EventOutcome,
    GovernanceEvent, GovernanceEventType, HealthAction, HealthNotice, HealthStatus,
    MetricFreshness, MetricKey, MetricUnit, MetricValue, MetricWindow, RepositoryClass,
    RuleLifecycle, RuleLifecycleSnapshot,
};
pub use rule::{
    ConsultMetadata, EntryFixture, EntryScope, Exemption, FixturePolarity, FixtureTrigger,
    Language, Lifecycle, LifecycleKind, OverrideEdge, OverrideGraph, OverrideRelation, RuleEntry,
    RuleSeverity, RuleSignal, RuleTrack, ScopeSpecificity, pattern_specificity,
};
pub use traits::{DiffAnalyzer, IntentRetriever};

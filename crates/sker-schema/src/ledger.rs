use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{RuleTrack, SemanticVerdictState};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceEventType {
    Consult,
    Search,
    Check,
    Attest,
    Proposal,
    CiGate,
    Replay,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventCounters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consult_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_line_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventLabels {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_class: Option<RepositoryClass>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<EventLanguage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<EventOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryClass {
    Service,
    Library,
    Infra,
    Monorepo,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EventLanguage {
    Go,
    Python,
    Typescript,
    Yaml,
    Infra,
    Mixed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventOutcome {
    Pass,
    Block,
    Warn,
    NoData,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GovernanceEvent {
    pub event_id: String,
    pub occurred_at: String,
    pub event_type: GovernanceEventType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    pub bundle_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<RuleTrack>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<SemanticVerdictState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counters: Option<EventCounters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<EventLabels>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetricKey {
    ConsultCitationRate,
    WarningPrecision,
    LocalCoverage,
    MergeEscapeRate,
    ReworkCostRatio,
    DeclarationMismatchRate,
    RuleAgeDistribution,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MetricWindow {
    pub from: String,
    pub to: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MetricValue {
    pub key: MetricKey,
    pub value: Option<f64>,
    pub numerator: f64,
    pub denominator: f64,
    pub sample_count: u64,
    pub unit: MetricUnit,
    pub freshness: MetricFreshness,
    pub distribution: Vec<AgeDistributionBucket>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetricUnit {
    Ratio,
    Percent,
    Count,
    Days,
    Distribution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetricFreshness {
    Fresh,
    Delayed,
    Stale,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgeDistributionBucket {
    pub bucket: AgeBucket,
    pub rule_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgeBucket {
    #[serde(rename = "0_30_days")]
    Days0To30,
    #[serde(rename = "31_90_days")]
    Days31To90,
    #[serde(rename = "91_180_days")]
    Days91To180,
    #[serde(rename = "over_180_days")]
    Over180Days,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HealthNotice {
    pub rule_id: String,
    pub status: HealthStatus,
    pub sample_count: u64,
    pub noise_rate: Option<f64>,
    pub last_hit_at: Option<String>,
    pub expires_at: Option<String>,
    pub first_detected_at: String,
    pub message: String,
    pub action: HealthAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    SuspectedDead,
    Noisy,
    LifecycleExpiring,
    LifecycleExpired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum HealthAction {
    Review,
    Renew,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleLifecycle {
    Ordinary { cooldown_hours: u32 },
    IncidentHotfix { hotfix_expires_at: String },
}

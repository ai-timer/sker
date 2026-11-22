use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CompileError, RuleEntry};

pub trait BundleCompiler: Send + Sync {
    fn compile(&self, request: &CompileRequest) -> Result<RuleBundle, CompileError>;
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
pub enum RuleSourceKind {
    CentralPlaybook,
    BusinessLock,
    RepositoryLocal,
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
pub struct SourceLock {
    pub kind: RuleSourceKind,
    pub source_id: String,
    pub version: String,
    pub content_hash: String,
}

impl SourceLock {
    pub fn validate(&self) -> Result<(), crate::SchemaError> {
        if self.source_id.trim().is_empty()
            || self.source_id.len() > 256
            || self.version.trim().is_empty()
            || self.version.len() > 64
        {
            return Err(crate::SchemaError::InvalidEntry(
                "source lock identity/version is invalid".into(),
            ));
        }
        crate::HashDigest::new(self.content_hash.clone())
            .map_err(|error| crate::SchemaError::InvalidDigest(error.to_string()))?;
        Ok(())
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
#[serde(deny_unknown_fields)]
pub struct RuleSource {
    pub lock: SourceLock,
    pub rules: Vec<RuleEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompileRequest {
    pub sources: Vec<RuleSource>,
    pub compiler_version: String,
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
pub struct BundleMetadata {
    pub created_at: String,
    pub compiler_version: String,
}

/// Immutable, reproducible rule set. `bundle_sha256` covers only the
/// canonical logical source locks and normalized rule entries.
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
pub struct RuleBundle {
    pub schema_version: String,
    pub bundle_id: String,
    pub bundle_sha256: String,
    pub sources: Vec<SourceLock>,
    pub rules: Vec<RuleEntry>,
    pub metadata: BundleMetadata,
}

impl RuleBundle {
    pub fn validate(&self) -> Result<(), CompileError> {
        if self.schema_version != "sker.bundle/v1" {
            return Err(CompileError::BundleSerialization {
                detail: "schema_version must be sker.bundle/v1".into(),
            });
        }
        for source in &self.sources {
            source
                .validate()
                .map_err(|error| CompileError::BundleSerialization {
                    detail: error.to_string(),
                })?;
        }
        for rule in &self.rules {
            rule.validate()
                .map_err(|error| CompileError::InvalidEntry {
                    rule_id: Some(rule.rule_id.clone()),
                    detail: error.to_string(),
                })?;
        }
        let expected = compute_bundle_sha256(&self.sources, &self.rules)?;
        if expected != self.bundle_sha256 || self.bundle_id != self.bundle_sha256 {
            return Err(CompileError::BundleSerialization {
                detail: "bundle id or SHA-256 does not match logical contents".into(),
            });
        }
        Ok(())
    }
}

pub fn compute_bundle_sha256(
    sources: &[SourceLock],
    rules: &[RuleEntry],
) -> Result<String, CompileError> {
    let mut sources = sources.to_vec();
    sources.sort_by(|left, right| {
        left.source_id
            .cmp(&right.source_id)
            .then_with(|| left.version.cmp(&right.version))
    });
    let mut rules = rules.to_vec();
    rules.sort_by(|left, right| left.rule_id.cmp(&right.rule_id));
    let logical = serde_json::json!({"sources": sources, "rules": rules});
    let canonical = crate::canonical_json_bytes(&logical).map_err(|error| {
        CompileError::BundleSerialization {
            detail: error.to_string(),
        }
    })?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

pub fn archive_bundle(bundle: &RuleBundle) -> Result<rkyv::util::AlignedVec, CompileError> {
    bundle.validate()?;
    rkyv::to_bytes::<rkyv::rancor::Error>(bundle).map_err(|error| {
        CompileError::BundleSerialization {
            detail: error.to_string(),
        }
    })
}

pub fn validate_bundle_archive(bytes: &[u8]) -> Result<RuleBundle, CompileError> {
    let archived =
        rkyv::access::<ArchivedRuleBundle, rkyv::rancor::Error>(bytes).map_err(|error| {
            CompileError::BundleSerialization {
                detail: error.to_string(),
            }
        })?;
    let bundle: RuleBundle = rkyv::deserialize::<RuleBundle, rkyv::rancor::Error>(archived)
        .map_err(|error| CompileError::BundleSerialization {
            detail: error.to_string(),
        })?;
    bundle.validate()?;
    Ok(bundle)
}

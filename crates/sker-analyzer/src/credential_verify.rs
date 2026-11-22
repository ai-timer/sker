use sker_schema::{
    AgentCredential, DiffEntry, RuleBundle, SemanticVerdictState, credential_blake3,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialError {
    DigestMismatch,
    BundleMismatch,
    DiffMismatch,
    MissingDeclaration,
    StaleBinding,
    InvalidEvidence(String),
    InvalidExemption(String),
    RequiresEscalation(Vec<String>),
}

pub fn verify_credential(
    c: &AgentCredential,
    bundle_sha256: &str,
    diff_tree_hash: &str,
    required_semantic: &[String],
) -> Result<(), CredentialError> {
    verify_credential_with_context(
        c,
        bundle_sha256,
        diff_tree_hash,
        required_semantic,
        None,
        None,
    )
}

pub fn verify_credential_with_context(
    c: &AgentCredential,
    bundle_sha256: &str,
    diff_tree_hash: &str,
    required_semantic: &[String],
    diff: Option<&[DiffEntry]>,
    bundle: Option<&RuleBundle>,
) -> Result<(), CredentialError> {
    if credential_blake3(c).map_err(|_| CredentialError::DigestMismatch)? != c.credential_blake3 {
        return Err(CredentialError::DigestMismatch);
    }
    if c.bundle_sha256 != bundle_sha256 {
        return Err(CredentialError::BundleMismatch);
    }
    if c.diff_tree_hash != diff_tree_hash {
        return Err(CredentialError::DiffMismatch);
    }

    let mut escalations = Vec::new();

    for id in required_semantic {
        let Some(verdict) = c.semantic_verdicts.iter().find(|v| &v.rule_id == id) else {
            return Err(CredentialError::MissingDeclaration);
        };

        if verdict.declaration.trim().is_empty() {
            return Err(CredentialError::MissingDeclaration);
        }

        match verdict.state {
            SemanticVerdictState::Comply => {
                if let Some(diff) = diff {
                    let Some(ref path) = verdict.evidence_path else {
                        return Err(CredentialError::InvalidEvidence(format!(
                            "Comply verdict for '{id}' requires evidence_path"
                        )));
                    };
                    let Some(line) = verdict.evidence_line else {
                        return Err(CredentialError::InvalidEvidence(format!(
                            "Comply verdict for '{id}' requires evidence_line"
                        )));
                    };
                    let file_entry = diff.iter().find(|e| &e.path == path);
                    let Some(entry) = file_entry else {
                        return Err(CredentialError::InvalidEvidence(format!(
                            "evidence_path '{path}' not found in diff"
                        )));
                    };
                    let in_added = entry.added_lines.contains(&line);
                    let in_modified = entry.modified_lines.contains(&line);
                    if !in_added && !in_modified {
                        return Err(CredentialError::InvalidEvidence(format!(
                            "evidence_line {line} in '{path}' not within diff added/modified lines"
                        )));
                    }
                }
            }
            SemanticVerdictState::NotTriggeredInLogic => {
                if verdict.declaration.trim().is_empty() {
                    return Err(CredentialError::MissingDeclaration);
                }
            }
            SemanticVerdictState::Exempt => {
                if let Some(bundle) = bundle {
                    let rule = bundle.rules.iter().find(|r| &r.rule_id == id);
                    if let Some(rule) = rule {
                        let path_to_check = verdict
                            .evidence_path
                            .as_deref()
                            .or_else(|| diff.and_then(|d| d.first().map(|e| e.path.as_str())));
                        if let Some(p) = path_to_check {
                            let matches_exemption = rule.exemptions.iter().any(|ex| {
                                ex.paths
                                    .iter()
                                    .any(|pattern| crate::rules::glob(pattern, p))
                            });
                            if !matches_exemption {
                                return Err(CredentialError::InvalidExemption(format!(
                                    "path '{p}' does not match any exemption for rule '{id}'"
                                )));
                            }
                        }
                    }
                }
            }
            SemanticVerdictState::Escalate => {
                escalations.push(id.clone());
            }
        }
    }

    if !escalations.is_empty() {
        return Err(CredentialError::RequiresEscalation(escalations));
    }

    Ok(())
}

use sker_schema::{credential_blake3, AgentCredential};
#[derive(Debug, Clone, PartialEq, Eq)] pub enum CredentialError { DigestMismatch, BundleMismatch, DiffMismatch, MissingDeclaration, StaleBinding }
pub fn verify_credential(c:&AgentCredential,bundle_sha256:&str,diff_tree_hash:&str,required_semantic:&[String])->Result<(),CredentialError>{
 if credential_blake3(c).map_err(|_|CredentialError::DigestMismatch)?!=c.credential_blake3{return Err(CredentialError::DigestMismatch)}
 if c.bundle_sha256!=bundle_sha256{return Err(CredentialError::BundleMismatch)}
 if c.diff_tree_hash!=diff_tree_hash{return Err(CredentialError::DiffMismatch)}
 for id in required_semantic {if !c.semantic_verdicts.iter().any(|v|&v.rule_id==id&&!v.declaration.trim().is_empty()){return Err(CredentialError::MissingDeclaration)}} Ok(())
}

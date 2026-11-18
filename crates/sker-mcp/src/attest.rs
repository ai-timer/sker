use sker_schema::{AgentCredential, ConsultTraceEntry, CredentialIssuer, SemanticVerdict, canonical_json_bytes};

/// Issues a local credential bound to the finalized diff and immutable bundle.
pub fn issue(bundle_sha256: String, diff_tree_hash: String, consult_trace: Vec<ConsultTraceEntry>, semantic_verdicts: Vec<SemanticVerdict>, issuer: CredentialIssuer, created_at: String) -> Result<AgentCredential, String> {
    let mut credential = AgentCredential { schema_version: "sker.agent-credential/v1".into(), bundle_sha256, diff_tree_hash, consult_trace, semantic_verdicts, issuer, created_at, credential_blake3: String::new() };
    for digest in [&credential.bundle_sha256, &credential.diff_tree_hash].into_iter().chain(credential.consult_trace.iter().map(|trace| &trace.query_digest)).chain(credential.semantic_verdicts.iter().filter_map(|verdict| verdict.rationale_digest.as_ref())) {
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) { return Err("credential digests must be 64 lowercase hexadecimal characters".into()); }
    }
    if credential.consult_trace.iter().any(|trace| trace.consult_id.is_empty() || !is_datetime(&trace.occurred_at)) { return Err("consult trace identity or timestamp is invalid".into()); }
    if !is_datetime(&credential.created_at) { return Err("created_at must be an RFC3339 timestamp".into()); }
    if credential.semantic_verdicts.iter().any(|verdict| verdict.rule_id.is_empty() || verdict.declaration.trim().is_empty() || verdict.declaration.len() > 4000) { return Err("semantic verdict declaration is invalid".into()); }
    let mut payload = serde_json::to_value(&credential).map_err(|e| e.to_string())?;
    if let serde_json::Value::Object(fields) = &mut payload {
        fields.remove("credential_blake3");
        if let Some(serde_json::Value::Array(verdicts)) = fields.get_mut("semantic_verdicts") { for verdict in verdicts { if let serde_json::Value::Object(fields) = verdict && fields.get("rationale_digest").is_some_and(serde_json::Value::is_null) { fields.remove("rationale_digest"); } } }
    }
    credential.credential_blake3 = blake3::hash(&canonical_json_bytes(&payload).map_err(|e| e.to_string())?).to_hex().to_string();
    Ok(credential)
}

pub fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let days = (seconds / 86_400) as i64;
    let day_seconds = seconds % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", day_seconds / 3_600, (day_seconds / 60) % 60, day_seconds % 60)
}

fn is_datetime(value: &str) -> bool {
    let Some(t) = value.find('T') else { return false };
    value.ends_with('Z') || value.char_indices().any(|(index, character)| index > t && matches!(character, '+' | '-'))
}

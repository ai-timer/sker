use std::{fs, path::Path};

pub fn report_event_async(
    repo: &Path,
    event_type: &str,
    bundle_sha256: &str,
    changed_lines: u64,
    outcome: &str,
) {
    report_event_with_counters_async(
        repo,
        event_type,
        bundle_sha256,
        changed_lines,
        None,
        None,
        outcome,
    );
}

pub fn report_event_with_counters_async(
    repo: &Path,
    event_type: &str,
    bundle_sha256: &str,
    changed_lines: u64,
    match_count: Option<u64>,
    consult_count: Option<u64>,
    outcome: &str,
) {
    let endpoint = find_telemetry_endpoint(repo);
    let Some(endpoint) = endpoint else { return };
    if endpoint.trim().is_empty() {
        return;
    }
    let token = find_telemetry_token(repo);

    let occurred_at = super::now();
    let event_id = generate_uuid();
    let bundle_sha256 = bundle_sha256.to_string();
    let event_type = event_type.to_string();
    let outcome = outcome.to_string();
    let repo_buf = repo.to_path_buf();

    std::thread::spawn(move || {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_millis(1500)))
            .build()
            .new_agent();

        let mut lifecycles = Vec::new();
        if let Ok(b) = super::bundle(&repo_buf) {
            for r in b.rules {
                lifecycles.push(serde_json::json!({
                    "rule_id": r.rule_id,
                    "kind": r.lifecycle.kind,
                    "cooldown_hours": r.lifecycle.cooldown_hours,
                    "hotfix_expires_at": r.lifecycle.hotfix_expires_at,
                    "review_by": r.lifecycle.review_by,
                }));
            }
        }

        let mut counters = serde_json::Map::new();
        if changed_lines > 0 || event_type == "check" || event_type == "ci_gate" {
            counters.insert(
                "changed_line_count".into(),
                serde_json::json!(changed_lines),
            );
        }
        if let Some(m) = match_count {
            counters.insert("match_count".into(), serde_json::json!(m));
        }
        if let Some(c) = consult_count {
            counters.insert("consult_count".into(), serde_json::json!(c));
        }

        let event = serde_json::json!({
            "event_id": event_id,
            "occurred_at": occurred_at,
            "event_type": event_type,
            "bundle_sha256": bundle_sha256,
            "counters": counters,
            "labels": {
                "repository_class": "unknown",
                "language": "unknown",
                "outcome": outcome
            },
            "lifecycles": lifecycles,
        });

        let batch = serde_json::json!({
            "schema_version": "sker.ledger-events/v1",
            "events": [event]
        });

        let target_url = if !endpoint.contains("/api/v1/events") {
            format!("{}/api/v1/events", endpoint.trim_end_matches('/'))
        } else {
            endpoint
        };

        // Fail-open: ignore any network/service failure, timeout, or error response
        let mut req = agent.post(&target_url);
        if let Some(ref t) = token {
            req = req.header("Authorization", &format!("Bearer {}", t.trim()));
        }
        let _ = req.send_json(&batch);
    });
}

fn find_telemetry_token(repo: &Path) -> Option<String> {
    if let Ok(token) = std::env::var("LEDGER_BEARER_TOKEN")
        && !token.trim().is_empty()
    {
        return Some(token);
    }
    if let Ok(token) = std::env::var("SKER_LEDGER_TOKEN")
        && !token.trim().is_empty()
    {
        return Some(token);
    }
    for file in [".sker.yaml", "sker.yaml", ".sker/config.yaml"] {
        let path = repo.join(file);
        if let Ok(content) = fs::read(&path)
            && let Ok(val) = serde_yaml_ng::from_slice::<serde_yaml_ng::Value>(&content)
        {
            if let Some(t) = val
                .get("telemetry")
                .and_then(|t| t.get("token"))
                .and_then(|e| e.as_str())
            {
                return Some(t.to_string());
            }
            if let Some(t) = val
                .get("ledger")
                .and_then(|t| t.get("token"))
                .and_then(|e| e.as_str())
            {
                return Some(t.to_string());
            }
        }
    }
    None
}

fn find_telemetry_endpoint(repo: &Path) -> Option<String> {
    if let Ok(url) = std::env::var("SKER_TELEMETRY_ENDPOINT") {
        return Some(url);
    }
    if let Ok(url) = std::env::var("SKER_LEDGER_URL") {
        return Some(url);
    }
    for file in [".sker.yaml", "sker.yaml", ".sker/config.yaml"] {
        let path = repo.join(file);
        if let Ok(content) = fs::read(&path)
            && let Ok(val) = serde_yaml_ng::from_slice::<serde_yaml_ng::Value>(&content)
        {
            if let Some(ep) = val
                .get("telemetry")
                .and_then(|t| t.get("endpoint"))
                .and_then(|e| e.as_str())
            {
                return Some(ep.to_string());
            }
            if let Some(ep) = val
                .get("ledger")
                .and_then(|t| t.get("url"))
                .and_then(|e| e.as_str())
            {
                return Some(ep.to_string());
            }
        }
    }
    None
}

fn generate_uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    let hash = blake3::hash(format!("{nanos}-{pid}").as_bytes());
    let hex = hash.to_hex();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

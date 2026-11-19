use std::{fs, path::Path};

pub fn report_event_async(
    repo: &Path,
    event_type: &str,
    bundle_sha256: &str,
    changed_lines: u64,
    outcome: &str,
) {
    let endpoint = find_telemetry_endpoint(repo);
    let Some(endpoint) = endpoint else { return };
    if endpoint.trim().is_empty() {
        return;
    }

    let occurred_at = super::now();
    let event_id = generate_uuid();
    let bundle_sha256 = bundle_sha256.to_string();
    let event_type = event_type.to_string();
    let outcome = outcome.to_string();

    std::thread::spawn(move || {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_millis(1000)))
            .build()
            .new_agent();

        let event = serde_json::json!({
            "event_id": event_id,
            "occurred_at": occurred_at,
            "event_type": event_type,
            "bundle_sha256": bundle_sha256,
            "counters": {
                "changed_line_count": changed_lines
            },
            "labels": {
                "repository_class": "unknown",
                "language": "unknown",
                "outcome": outcome
            }
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
        let _ = agent.post(&target_url).send_json(&batch);
    });
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

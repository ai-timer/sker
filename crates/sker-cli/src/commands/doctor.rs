use super::{bundle, find_repo};
use anyhow::Result;
use std::{fs, path::Path};
pub fn run(path: &Path) -> Result<()> {
    let repo = find_repo(path)?;
    let mut failed = false;
    if repo.join(".sker.yaml").exists() {
        match sker_compiler::load_federated_sources(&repo, false) {
            Ok((_, has_fed)) => {
                if has_fed {
                    report("federated rules & lock", Ok(()), &mut failed);
                }
            }
            Err(e) => {
                report("federated rules & lock", Err(e.to_string()), &mut failed);
            }
        }
    }
    let b = bundle(&repo);
    if let Ok(ref bundle) = b {
        let today = super::now();
        for r in &bundle.rules {
            if let Some(ref rev) = r.lifecycle.review_by {
                let is_past = rev.as_str() < today.as_str();
                if is_past {
                    println!(
                        "WARN rule '{}' review date '{}' has passed; review required",
                        r.rule_id, rev
                    );
                }
            }
        }
    }
    report(
        "bundle hash",
        b.as_ref().map(|_| ()).map_err(|e| e.to_string()),
        &mut failed,
    );
    let max = 300usize;
    let agents = super::agent::get_configured_agents(&repo);
    for agent in &agents {
        let name = agent.name().to_lowercase();
        let rules_rel = agent.rules_file();
        let rules_res = rules_block_health(&repo, &rules_rel, max);
        report(
            &format!("{rules_rel} managed block <= {max} tokens"),
            rules_res.clone(),
            &mut failed,
        );

        let mcp_absent = agent.mcp_file().is_some_and(|rel| !repo.join(rel).exists());
        if name == "universal" && mcp_absent {
            // The default universal integration is AGENTS.md-only; an absent
            // .mcp.json is optional, never a failure.
            println!(
                "SKIP optional universal MCP (.mcp.json not present; run 'sker agent add universal' if using an MCP client)"
            );
        } else if name == "antigravity" {
            let mut mcp_res = mcp_health(&repo, agent);
            if mcp_res.is_err() && antigravity_home_config().is_some_and(|p| mcp_json_has_sker(&p))
            {
                mcp_res = Ok(());
            }
            report(
                "Antigravity configuration",
                rules_res.and(mcp_res),
                &mut failed,
            );
        } else {
            report(
                &format!("{name} MCP configuration"),
                mcp_health(&repo, agent),
                &mut failed,
            );
        }

        if name == "claude" {
            let hook_res = claude_hook_health(&repo);
            report("Hook configuration", hook_res, &mut failed);
        }
    }
    let endpoint_url = std::env::var("SKER_LEDGER_URL")
        .or_else(|_| std::env::var("SKER_TELEMETRY_ENDPOINT"))
        .ok();
    if let Some(url) = endpoint_url {
        let base = url.trim_end_matches('/');
        let token = std::env::var("LEDGER_BEARER_TOKEN")
            .or_else(|_| std::env::var("SKER_LEDGER_TOKEN"))
            .ok();
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_millis(1500)))
            .build()
            .new_agent();
        let probe_url = if base.ends_with("/api/v1/events") {
            base.replace("/api/v1/events", "/healthz")
        } else {
            format!("{base}/healthz")
        };
        let overview_url = format!("{base}/api/v1/metrics/overview");
        let mut overview_req = agent.get(&overview_url);
        if let Some(ref t) = token {
            overview_req = overview_req.header("Authorization", &format!("Bearer {}", t.trim()));
        }
        let check_res = match agent.get(&probe_url).call() {
            Ok(_) => match overview_req.call() {
                Ok(_) => Ok(()),
                Err(ureq::Error::StatusCode(401)) => Err(format!(
                    "Ledger authentication failed (HTTP 401): invalid or missing LEDGER_BEARER_TOKEN at {base}"
                )),
                Err(ureq::Error::StatusCode(code)) if code < 500 => Ok(()),
                Err(e) => Err(format!("Ledger metrics endpoint error: {e}")),
            },
            Err(ureq::Error::StatusCode(401)) => Err(format!(
                "Ledger authentication failed (HTTP 401): invalid or missing LEDGER_BEARER_TOKEN at {base}"
            )),
            Err(e) => Err(format!("Ledger unavailable at {base}: {e}")),
        };
        report("optional Ledger", check_res, &mut failed);
    } else {
        println!("SKIP optional Ledger: SKER_LEDGER_URL not configured")
    };
    if failed {
        anyhow::bail!("doctor found failed checks; see repair guidance above")
    };
    Ok(())
}
fn report(label: &str, result: Result<(), String>, failed: &mut bool) {
    match result {
        Ok(()) => println!("PASS {label}"),
        Err(e) => {
            *failed = true;
            println!("FAIL {label}: {e}")
        }
    }
}
fn rules_block_health(repo: &Path, rel: &str, max: usize) -> Result<(), String> {
    let p = repo.join(rel);
    let text = fs::read_to_string(&p)
        .map_err(|_| format!("{rel} missing; run `sker init` or `sker compile`"))?;
    match managed_block(&text) {
        Some(s) if s.split_whitespace().count() <= max => Ok(()),
        Some(_) => Err("managed block exceeds token limit".into()),
        None => Err("managed block missing or malformed; run `sker compile`".into()),
    }
}
fn mcp_health(repo: &Path, agent: &sker_schema::AgentConfig) -> Result<(), String> {
    let Some(rel) = agent.mcp_file() else {
        return Ok(());
    };
    let p = repo.join(&rel);
    let repair = format!("run `sker agent add {}`", agent.name());
    if rel.ends_with(".toml") {
        let text = fs::read_to_string(&p).map_err(|_| format!("{rel} missing; {repair}"))?;
        if text.contains("[mcp_servers.sker]") {
            Ok(())
        } else {
            Err(format!("{rel} has no [mcp_servers.sker] entry; {repair}"))
        }
    } else if mcp_json_has_sker(&p) {
        Ok(())
    } else {
        Err(format!(
            "{rel} missing or has no sker mcpServers entry; {repair}"
        ))
    }
}
fn mcp_json_has_sker(p: &Path) -> bool {
    fs::read(p)
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .is_some_and(|v| v["mcpServers"]["sker"]["command"] == "sker")
}
fn antigravity_home_config() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".gemini/antigravity/mcp_config.json"))
        .filter(|p| p.exists())
}
fn claude_hook_health(repo: &Path) -> Result<(), String> {
    let p = repo.join(".claude/settings.json");
    let bytes = fs::read(&p).map_err(|_| "no Claude Hook config; run `sker agent add claude`")?;
    let json: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| ".claude/settings.json is not valid JSON".to_string())?;
    let has_sker_hook = json["hooks"]["PreToolUse"].as_array().is_some_and(|arr| {
        arr.iter().any(|entry| {
            entry["hooks"].as_array().is_some_and(|hs| {
                hs.iter().any(|h| {
                    h["command"]
                        .as_str()
                        .is_some_and(|c| c.contains("sker hook"))
                })
            })
        })
    });
    if has_sker_hook {
        Ok(())
    } else {
        Err("no PreToolUse `sker hook` entry; run `sker agent add claude`".into())
    }
}
fn managed_block(s: &str) -> Option<&str> {
    let a = s.find("<!-- sker:managed:start -->")?;
    let b = s.find("<!-- sker:managed:end -->")?;
    if b < a {
        return None;
    }
    Some(&s[a..b + "<!-- sker:managed:end -->".len()])
}

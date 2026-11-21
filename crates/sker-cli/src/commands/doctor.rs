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
    for file in [
        "AGENTS.md",
        "CLAUDE.md",
        ".agent/rules/sker.md",
        ".agents/rules/sker.md",
    ] {
        let p = repo.join(file);
        if p.exists() {
            let text = fs::read_to_string(&p)?;
            let block = managed_block(&text);
            let ok = block.is_some_and(|s| s.split_whitespace().count() <= max);
            report(
                &format!("{file} managed block <= {max} tokens"),
                if ok {
                    Ok(())
                } else {
                    Err("managed block exceeds token limit or is malformed".into())
                },
                &mut failed,
            );
        }
    }
    let settings = repo.join(".mcp.json");
    let codex = repo.join(".codex/config.toml");
    let antigravity_user_cfg = std::env::var_os("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".gemini/antigravity/mcp_config.json"));
    let hook = repo.join(".claude/settings.json");
    report(
        "MCP configuration",
        if settings.exists()
            || codex.exists()
            || antigravity_user_cfg.as_ref().is_some_and(|p| p.exists())
        {
            Ok(())
        } else {
            Err("no MCP host config; run `sker init` and register `sker mcp`".into())
        },
        &mut failed,
    );
    let antigravity_host = repo.join(".agent").exists()
        || repo.join(".agents").exists()
        || antigravity_user_cfg.as_ref().is_some_and(|p| p.exists());
    if antigravity_host {
        let rule_file_exists = repo.join(".agent/rules/sker.md").exists()
            || repo.join(".agents/rules/sker.md").exists();
        report(
            "Antigravity configuration",
            if settings.exists() || antigravity_user_cfg.as_ref().is_some_and(|p| p.exists()) {
                if rule_file_exists {
                    Ok(())
                } else {
                    Err(
                        "Antigravity detected but .agent/rules/sker.md missing; run `sker init`"
                            .into(),
                    )
                }
            } else {
                Err("Antigravity MCP configuration missing; run `sker init`".into())
            },
            &mut failed,
        );
    } else {
        println!("SKIP Antigravity configuration: no Antigravity host detected");
    }
    if repo.join(".claude").exists()
        || std::env::var_os("HOME")
            .is_some_and(|h| std::path::PathBuf::from(h).join(".claude").exists())
    {
        report(
            "Hook configuration",
            if hook.exists() {
                Ok(())
            } else {
                Err("no Claude Hook config; run `sker init` and configure PreToolUse".into())
            },
            &mut failed,
        );
    } else {
        println!("SKIP Hook configuration: no Claude host detected")
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
fn managed_block(s: &str) -> Option<&str> {
    let a = s.find("<!-- sker:managed:start -->")?;
    let b = s.find("<!-- sker:managed:end -->")?;
    if b < a {
        return None;
    }
    Some(&s[a..b + "<!-- sker:managed:end -->".len()])
}

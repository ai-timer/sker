use super::{find_repo, load_baseline, now};
use anyhow::{Context, Result, bail};
use sker_schema::{AnalysisRequest, CheckStatus, DiffAnalyzer, FindingDisposition};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
pub fn run(
    path: &Path,
    bundle_override: Option<&Path>,
    ci: bool,
    sarif: Option<&Path>,
    json_path: Option<&Path>,
    base: Option<&str>,
    credential: Option<&Path>,
) -> Result<()> {
    let repo = find_repo(path)?;
    if repo.join(".sker.yaml").exists() {
        let _ = sker_compiler::load_federated_sources(&repo, false)
            .map_err(|e| anyhow::anyhow!("Federated rules lock verification failed: {e}"))?;
    }
    let b = super::support::bundle_with_override(&repo, bundle_override)?;
    let explicit_base = base
        .map(str::to_owned)
        .or_else(|| std::env::var("GITHUB_BASE_SHA").ok());
    let diff_result = sker_analyzer::compute_diff(
        repo.to_str().context("non-UTF8 repository path")?,
        explicit_base.as_deref(),
        ci,
    )
    .context("unable to compute Git diff")?;
    let request = AnalysisRequest {
        repository_root: repo.to_string_lossy().into_owned(),
        base_revision: diff_result.base_revision,
        head_revision: diff_result.head_revision,
        diff: diff_result.entries,
        baseline: load_baseline(&repo)?,
        now: now(),
    };
    let report = sker_analyzer::Analyzer.analyze(&b, &request)?;
    let agent_evidence = credential.is_some() || std::env::var_os("SKER_AGENT_RUN").is_some();
    if agent_evidence {
        let p =
            credential.context("Agent semantic credential missing; pass --credential <path>")?;
        let c: sker_schema::AgentCredential =
            serde_json::from_slice(&fs::read(p).context("reading Agent credential")?)?;
        let required: Vec<String> = report
            .findings
            .iter()
            .filter(|f| {
                f.track == sker_schema::RuleTrack::Semantic
                    && f.disposition == FindingDisposition::PendingSemantic
            })
            .map(|f| f.rule_id.clone())
            .collect();
        let verify_res = sker_analyzer::verify_credential_with_context(
            &c,
            &report.bundle_sha256,
            &report.diff_tree_hash,
            &required,
            Some(&request.diff),
            Some(&b),
        );
        match verify_res {
            Ok(()) => {}
            Err(sker_analyzer::CredentialError::RequiresEscalation(rule_ids)) => {
                if ci {
                    eprintln!(
                        "sker check: CI gate blocked due to pending human escalation on rule(s): {}",
                        rule_ids.join(", ")
                    );
                    std::process::exit(1);
                }
            }
            Err(e) => {
                bail!("Agent credential or semantic declaration invalid: {e:?}");
            }
        }
    }
    if ci {
        let default_sarif = repo.join(".sker/report.sarif");
        let default_json = repo.join(".sker/report.json");
        let sp = sarif.unwrap_or(&default_sarif);
        let jp = json_path.unwrap_or(&default_json);
        write_pair(sp, &sker_analyzer::sarif_document(&report), jp, &report)?;
    } else {
        println!(
            "{}: {} finding(s), {} changed line(s)",
            report.bundle_sha256,
            report.findings.len(),
            report.changed_line_count
        );
        println!("{}", serde_json::to_string_pretty(&report)?);
    }
    let outcome = if report.status == CheckStatus::Blocked {
        "block"
    } else if report.status == CheckStatus::Passed {
        "pass"
    } else {
        "no_data"
    };
    super::telemetry::report_event_async(
        &repo,
        if ci { "ci_gate" } else { "check" },
        &report.bundle_sha256,
        report.changed_line_count,
        outcome,
    );
    if ci && !report.analysis.complete {
        std::process::exit(2)
    }
    if report.status == CheckStatus::Blocked {
        std::process::exit(1)
    }
    Ok(())
}
fn write_pair(
    sarif_path: &Path,
    sarif: &serde_json::Value,
    json_path: &Path,
    report: &sker_schema::CheckReport,
) -> Result<()> {
    if sarif_path == json_path {
        bail!("SARIF and JSON output paths must differ")
    };
    let sa = serde_json::to_vec_pretty(sarif)?;
    let ja = serde_json::to_vec_pretty(report)?;
    let salt = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let st = stage(sarif_path, &sa, salt)?;
    let jt = match stage(json_path, &ja, salt) {
        Ok(p) => p,
        Err(e) => {
            let _ = fs::remove_file(st);
            return Err(e);
        }
    };
    if let Err(e) = fs::rename(&st, sarif_path) {
        let _ = fs::remove_file(&st);
        let _ = fs::remove_file(&jt);
        return Err(e.into());
    }
    if let Err(e) = fs::rename(&jt, json_path) {
        let _ = fs::remove_file(&jt);
        return Err(e.into());
    }
    Ok(())
}
fn stage(path: &Path, data: &[u8], salt: u128) -> Result<PathBuf> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("report");
    let tmp = parent.join(format!(".{name}.{salt}.tmp"));
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    f.write_all(data)?;
    f.sync_all()?;
    Ok(tmp)
}

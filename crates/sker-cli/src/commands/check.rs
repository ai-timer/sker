use super::{bundle, find_repo, load_baseline, now};
use anyhow::{Context, Result, bail};
use sker_schema::{AnalysisRequest, CheckStatus, DiffAnalyzer, FindingDisposition};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
pub fn run(
    path: &Path,
    ci: bool,
    sarif: Option<&Path>,
    json_path: Option<&Path>,
    base: Option<&str>,
    credential: Option<&Path>,
) -> Result<()> {
    let repo = find_repo(path)?;
    let b = bundle(&repo)?;
    let base = base
        .map(str::to_owned)
        .or_else(|| std::env::var("GITHUB_BASE_SHA").ok())
        .unwrap_or_else(|| "HEAD^".into());
    let diff = sker_analyzer::extract_diff(
        repo.to_str().context("non-UTF8 repository path")?,
        &base,
        "HEAD",
    )
    .context("unable to compute Git diff")?;
    let request = AnalysisRequest {
        repository_root: repo.to_string_lossy().into_owned(),
        base_revision: base,
        head_revision: "HEAD".into(),
        diff,
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
        sker_analyzer::verify_credential(
            &c,
            &report.bundle_sha256,
            &report.diff_tree_hash,
            &required,
        )
        .map_err(|e| anyhow::anyhow!("Agent credential or semantic declaration invalid: {e:?}"))?;
    }
    if ci {
        let sp = sarif.context("--ci requires --sarif <path>")?;
        let jp = json_path.context("--ci requires --json <path>")?;
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
    if ci && !report.analysis.complete {
        std::process::exit(2)
    }
    if ci && report.status == CheckStatus::Blocked {
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

use super::{atomic_json, bundle, find_repo, now};
use anyhow::{Context, Result, bail};
use serde_json::json;
use sker_retriever::IntentRetriever;
use sker_schema::*;
use std::{
    fs,
    io::{self, Read},
    path::Path,
};

pub fn consult(path: &Path, intent: &str, paths: &[String]) -> Result<()> {
    let repo = find_repo(path)?;
    let b = bundle(&repo)?;
    validate_query_paths(paths)?;
    let retriever = sker_retriever::TantivyRetriever::default();
    let query = ConsultQuery {
        intent: intent.to_string(),
        paths: paths.to_vec(),
        limit: Some(10),
        minimum_score: None,
    };
    let res = retriever.consult(&b, &query)?;
    super::telemetry::report_event_async(
        &repo,
        "consult",
        &b.bundle_sha256,
        0,
        if res.matches.is_empty() {
            "no_data"
        } else {
            "pass"
        },
    );
    println!("{}", serde_json::to_string(&res)?);
    Ok(())
}

pub fn search(path: &Path, query: &str) -> Result<()> {
    let repo = find_repo(path)?;
    let b = bundle(&repo)?;
    let retriever = sker_retriever::TantivyRetriever::default();
    let q = SearchQuery {
        query: query.to_string(),
        paths: vec![],
        domains: vec![],
        limit: Some(20),
    };
    let res = retriever.search(&b, &q)?;
    super::telemetry::report_event_async(
        &repo,
        "search",
        &b.bundle_sha256,
        0,
        if res.rules.is_empty() {
            "no_data"
        } else {
            "pass"
        },
    );
    println!("{}", serde_json::to_string(&res)?);
    Ok(())
}

pub fn attest(path: &Path, semantic: &str, output: Option<&Path>) -> Result<()> {
    let repo = find_repo(path)?;
    let b = bundle(&repo)?;
    let base = std::env::var("GITHUB_BASE_SHA").unwrap_or_else(|_| "HEAD^".into());
    let diff = sker_analyzer::extract_diff(repo.to_str().unwrap_or("."), &base, "HEAD")?;
    let hash = compute_diff_tree_hash(&diff);
    let semantic_verdicts: Vec<SemanticVerdict> = serde_json::from_str(semantic)
        .context("--semantic must be a JSON array of SemanticVerdict")?;
    let relevant: Vec<_> = b
        .rules
        .iter()
        .filter(|r| r.track == RuleTrack::Semantic)
        .map(|r| r.rule_id.as_str())
        .collect();
    for id in relevant {
        if !semantic_verdicts.iter().any(|v| v.rule_id == id) {
            bail!("missing semantic declaration for {id}")
        }
    }
    let credential = sker_mcp::attest::issue(
        b.bundle_sha256.clone(),
        hash,
        vec![],
        semantic_verdicts,
        CredentialIssuer::LocalCli,
        now(),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    super::telemetry::report_event_async(&repo, "attest", &b.bundle_sha256, 0, "pass");
    if let Some(out) = output {
        atomic_json(out, &credential)?;
    } else {
        println!("{}", serde_json::to_string_pretty(&credential)?);
    }
    Ok(())
}

pub fn propose(path: &Path, entry: &Path, rationale: &str) -> Result<()> {
    let repo = find_repo(path)?;
    let b = bundle(&repo)?;
    let raw = fs::read(entry)?;
    let rule: RuleEntry = serde_yaml_ng::from_slice(&raw)?;
    rule.validate()?;
    let id = blake3::hash(&raw).to_hex()[..12].to_string();
    let proposal = Proposal {
        proposal_id: format!("prop-{}", id),
        state: ProposalState::Pending,
        entry: rule,
        rationale: rationale.to_string(),
        based_on_bundle_sha256: Some(b.bundle_sha256.clone()),
        created_at: now(),
    };
    let pdir = repo.join(".sker/proposals");
    fs::create_dir_all(&pdir)?;
    atomic_json(&pdir.join(format!("{}.json", proposal.proposal_id)), &proposal)?;
    super::telemetry::report_event_async(&repo, "proposal", &b.bundle_sha256, 0, "pass");
    println!("saved proposal {}", proposal.proposal_id);
    Ok(())
}

pub fn replay(path: &Path, from: &str, to: &str) -> Result<()> {
    let repo = find_repo(path)?;
    let b = bundle(&repo)?;
    let output = std::process::Command::new("git")
        .current_dir(&repo)
        .args(["rev-list", &format!("{from}..{to}")])
        .output()?;
    let mut commits = Vec::new();
    if output.status.success() {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let rev = line.trim();
            if rev.is_empty() {
                continue;
            }
            commits.push(sker_analyzer::ReplayCommit {
                revision: rev.to_string(),
                bundle_sha256: b.bundle_sha256.clone(),
                rule_ids: b.rules.iter().map(|r| r.rule_id.clone()).collect(),
                complete: true,
            });
        }
    }
    let report = sker_analyzer::replay(&commits, from, to, &b.bundle_sha256);
    super::telemetry::report_event_async(&repo, "replay", &b.bundle_sha256, 0, "pass");
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn mcp(repo: &Path) -> Result<()> {
    let root = find_repo(repo)?;
    let b = bundle(&root)?;
    let proposals_dir = root.join(".sker/proposals");
    let server = sker_mcp::SkerServer::new(b, proposals_dir);
    sker_mcp::serve_stdio(server).await
}

pub fn hook(repo: &Path, hard_block: bool) -> Result<()> {
    let _ = find_repo(repo)?;
    let mut input_str = String::new();
    let _ = io::stdin().read_to_string(&mut input_str);
    let input: sker_mcp::PreToolUseInput = if input_str.trim().is_empty() {
        sker_mcp::PreToolUseInput {
            hook_event_name: Some("PreToolUse".into()),
            tool_name: None,
            tool_input: None,
        }
    } else {
        serde_json::from_str(&input_str).unwrap_or(sker_mcp::PreToolUseInput {
            hook_event_name: Some("PreToolUse".into()),
            tool_name: None,
            tool_input: None,
        })
    };
    let output = sker_mcp::hook::handle(&input);
    let delivery_status = "delivered";
    let is_blocked = hard_block && !output.hook_specific_output.additional_context.is_empty();
    println!(
        "{}",
        json!({
            "hookSpecificOutput": output.hook_specific_output,
            "delivery_status": delivery_status,
            "blocked": is_blocked,
        })
    );
    if is_blocked {
        std::process::exit(1);
    }
    Ok(())
}

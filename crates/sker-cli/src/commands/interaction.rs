use super::{atomic_json, bundle, find_repo, now};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use sker_schema::*;
use std::{
    fs,
    io::{self, BufRead},
    path::Path,
};
pub fn consult(path: &Path, intent: &str, paths: &[String]) -> Result<()> {
    let repo = find_repo(path)?;
    let b = bundle(&repo)?;
    validate_query_paths(paths)?;
    let words = terms(intent);
    let mut rows: Vec<_> = b
        .rules
        .iter()
        .filter(|r| paths.iter().any(|p| scope_match(&r.scope.include_paths, p)))
        .map(|r| {
            let text = format!(
                "{} {} {} {} {}",
                r.title,
                r.description,
                r.domain,
                r.consult.keywords.join(" "),
                r.consult.synonyms.join(" ")
            )
            .to_lowercase();
            let score = words.iter().filter(|w| text.contains(w.as_str())).count() as f64
                / words.len().max(1) as f64;
            (score, r)
        })
        .filter(|(score, _)| *score >= 0.15)
        .collect();
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    let out:Vec<_>=rows.into_iter().take(10).map(|(score,r)|json!({"rule_id":r.rule_id,"title":r.title,"domain":r.domain,"severity":r.severity,"track":r.track,"paths":r.scope.include_paths,"summary":r.description,"score":score})).collect();
    println!(
        "{}",
        json!({"status":if out.is_empty(){"no_match"}else{"matched"},"bundle_sha256":b.bundle_sha256,"matches":out})
    );
    Ok(())
}
pub fn search(path: &Path, query: &str) -> Result<()> {
    let repo = find_repo(path)?;
    let b = bundle(&repo)?;
    let terms = terms(query);
    let mut found: Vec<_> = b
        .rules
        .iter()
        .map(|r| {
            let text = format!(
                "{} {} {} {}",
                r.title,
                r.description,
                r.domain,
                r.tags.join(" ")
            )
            .to_lowercase();
            (
                terms.iter().filter(|w| text.contains(w.as_str())).count(),
                r,
            )
        })
        .filter(|(n, _)| *n > 0)
        .collect();
    found.sort_by_key(|a| std::cmp::Reverse(a.0));
    let results:Vec<_>=found.into_iter().take(20).map(|(_,r)|json!({"rule_id":r.rule_id,"title":r.title,"domain":r.domain,"severity":r.severity,"track":r.track,"paths":r.scope.include_paths,"summary":r.description,"score":1})).collect();
    println!(
        "{}",
        json!({"status":if results.is_empty(){"no_match"}else{"matched"},"bundle_sha256":b.bundle_sha256,"results":results})
    );
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
    let mut credential = AgentCredential {
        schema_version: "sker.agent-credential/v1".into(),
        bundle_sha256: b.bundle_sha256,
        diff_tree_hash: hash,
        consult_trace: vec![],
        semantic_verdicts,
        issuer: CredentialIssuer::LocalCli,
        created_at: now(),
        credential_blake3: String::new(),
    };
    credential.credential_blake3 = credential_blake3(&credential)?;
    if let Some(p) = output {
        atomic_json(p, &credential)?
    } else {
        println!("{}", serde_json::to_string_pretty(&credential)?)
    }
    Ok(())
}
pub fn propose(repo: &Path, entry: &Path, rationale: &str) -> Result<()> {
    let root = find_repo(repo)?;
    let rule: RuleEntry = serde_yaml_ng::from_slice(&fs::read(entry)?)?;
    rule.validate()?;
    let proposal = json!({"schema_version":"sker.proposal/v1","proposal_id":format!("{}-{}",rule.rule_id,now().replace(':',"-")),"state":"pending","entry":rule,"rationale":rationale,"created_at":now()});
    let dir = root.join(".sker/proposals");
    fs::create_dir_all(&dir)?;
    atomic_json(
        &dir.join(format!(
            "{}.json",
            proposal["proposal_id"].as_str().unwrap()
        )),
        &proposal,
    )?;
    println!("{}", proposal);
    Ok(())
}
pub fn replay(repo: &Path, from: &str, to: &str) -> Result<()> {
    let root = find_repo(repo)?;
    let b = bundle(&root)?;
    let output = std::process::Command::new("git")
        .args([
            "log",
            "--format=%H",
            &format!("--since={from}",),
            "--until",
            to,
        ])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        bail!("git history query failed")
    };
    println!(
        "{}",
        json!({"from":from,"to":to,"bundle_sha256":b.bundle_sha256,"complete":false,"missing_history_reason":"history replay requires stored per-commit analysis records","rule_stats":[]})
    );
    Ok(())
}
pub fn mcp(repo: &Path) -> Result<()> {
    let root = find_repo(repo)?;
    bundle(&root)?;
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("invalid MCP request: {e}");
                continue;
            }
        };
        let id = request.get("id").cloned();
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        if id.is_none() {
            continue;
        }
        let response = match method {
            "initialize" => {
                json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"sker","version":env!("CARGO_PKG_VERSION")}})
            }
            "ping" => json!({}),
            "tools/list" => json!({"tools":[
                {"name":"sker_consult","description":"Find relevant local rules","inputSchema":{"type":"object","required":["intent","paths"],"properties":{"intent":{"type":"string"},"paths":{"type":"array","items":{"type":"string"}}}}},
                {"name":"sker_search","description":"Search local rule metadata","inputSchema":{"type":"object","required":["query"],"properties":{"query":{"type":"string"}}}},
                {"name":"sker_attest","description":"Issue a local diff-bound Agent credential","inputSchema":{"type":"object","properties":{"semantic_verdicts":{"type":"array"}}}},
                {"name":"sker_propose","description":"Store a pending rule proposal","inputSchema":{"type":"object","required":["entry","rationale"],"properties":{"entry":{"type":"object"},"rationale":{"type":"string"}}}}
            ]}),
            "tools/call" => {
                let params = request.get("params").cloned().unwrap_or(Value::Null);
                match call_tool(&root, &params) {
                    Ok(value) => {
                        json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false})
                    }
                    Err(error) => {
                        json!({"content":[{"type":"text","text":format!("{error:#}")}],"isError":true})
                    }
                }
            }
            _ => json!({"code":-32601,"message":format!("unknown method: {method}")}),
        };
        let envelope = if response.get("code").is_some() {
            json!({"jsonrpc":"2.0","id":id,"error":response})
        } else {
            json!({"jsonrpc":"2.0","id":id,"result":response})
        };
        println!("{envelope}");
    }
    Ok(())
}
fn call_tool(repo: &Path, params: &Value) -> Result<Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .context("tool name missing")?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    match name {
        "sker_consult" => {
            let intent = args
                .get("intent")
                .and_then(Value::as_str)
                .context("intent missing")?;
            let paths: Vec<String> =
                serde_json::from_value(args.get("paths").cloned().unwrap_or(Value::Null))?;
            validate_query_paths(&paths)?;
            let b = bundle(repo)?;
            let terms = terms(intent);
            let mut rows: Vec<_> = b
                .rules
                .iter()
                .filter(|r| paths.iter().any(|p| scope_match(&r.scope.include_paths, p)))
                .map(|r| {
                    let text = format!(
                        "{} {} {} {} {}",
                        r.title,
                        r.description,
                        r.domain,
                        r.consult.keywords.join(" "),
                        r.consult.synonyms.join(" ")
                    )
                    .to_lowercase();
                    (
                        terms.iter().filter(|w| text.contains(w.as_str())).count() as f64
                            / terms.len().max(1) as f64,
                        r,
                    )
                })
                .filter(|(score, _)| *score >= 0.15)
                .collect();
            rows.sort_by(|a, b| b.0.total_cmp(&a.0));
            let matches:Vec<_>=rows.into_iter().take(10).map(|(score,r)|json!({"rule_id":r.rule_id,"title":r.title,"domain":r.domain,"severity":r.severity,"track":r.track,"paths":r.scope.include_paths,"summary":r.description,"score":score})).collect();
            Ok(
                json!({"status":if matches.is_empty(){"no_match"}else{"matched"},"bundle_sha256":b.bundle_sha256,"matches":matches}),
            )
        }
        "sker_search" => {
            let query = args
                .get("query")
                .and_then(Value::as_str)
                .context("query missing")?;
            let b = bundle(repo)?;
            let terms = terms(query);
            let mut rows: Vec<_> = b
                .rules
                .iter()
                .map(|r| {
                    let text = format!(
                        "{} {} {} {}",
                        r.title,
                        r.description,
                        r.domain,
                        r.tags.join(" ")
                    )
                    .to_lowercase();
                    (
                        terms.iter().filter(|w| text.contains(w.as_str())).count(),
                        r,
                    )
                })
                .filter(|(n, _)| *n > 0)
                .collect();
            rows.sort_by_key(|x| std::cmp::Reverse(x.0));
            let results:Vec<_>=rows.into_iter().take(20).map(|(_,r)|json!({"rule_id":r.rule_id,"title":r.title,"domain":r.domain,"severity":r.severity,"track":r.track,"paths":r.scope.include_paths,"summary":r.description,"score":1})).collect();
            Ok(
                json!({"status":if results.is_empty(){"no_match"}else{"matched"},"bundle_sha256":b.bundle_sha256,"results":results}),
            )
        }
        "sker_attest" => {
            let b = bundle(repo)?;
            let base = std::env::var("GITHUB_BASE_SHA").unwrap_or_else(|_| "HEAD^".into());
            let diff = sker_analyzer::extract_diff(repo.to_str().unwrap_or("."), &base, "HEAD")?;
            let semantic: Vec<SemanticVerdict> = serde_json::from_value(
                args.get("semantic_verdicts")
                    .cloned()
                    .unwrap_or_else(|| json!([])),
            )?;
            let mut c = AgentCredential {
                schema_version: "sker.agent-credential/v1".into(),
                bundle_sha256: b.bundle_sha256,
                diff_tree_hash: compute_diff_tree_hash(&diff),
                consult_trace: vec![],
                semantic_verdicts: semantic,
                issuer: CredentialIssuer::Mcp,
                created_at: now(),
                credential_blake3: String::new(),
            };
            c.credential_blake3 = credential_blake3(&c)?;
            Ok(serde_json::to_value(c)?)
        }
        "sker_propose" => {
            let entry: RuleEntry =
                serde_json::from_value(args.get("entry").cloned().context("entry missing")?)?;
            entry.validate()?;
            let rationale = args
                .get("rationale")
                .and_then(Value::as_str)
                .context("rationale missing")?;
            let id = format!("{}-{}", entry.rule_id, now().replace(':', "-"));
            let p = json!({"schema_version":"sker.proposal/v1","proposal_id":id,"state":"pending","entry":entry,"rationale":rationale,"created_at":now()});
            let dir = repo.join(".sker/proposals");
            fs::create_dir_all(&dir)?;
            atomic_json(&dir.join(format!("{id}.json")), &p)?;
            Ok(p)
        }
        _ => bail!("unknown tool {name}"),
    }
}
pub fn hook(repo: &Path, hard_block: bool) -> Result<()> {
    let root = find_repo(repo)?;
    let mut input = String::new();
    for line in io::stdin().lock().lines() {
        input.push_str(&line?);
        if input.len() > 1_000_000 {
            bail!("Hook input exceeds 1 MiB")
        }
    }
    let parsed: Value =
        serde_json::from_str(&input).context("Hook input must be a JSON PreToolUse event")?;
    let tool = parsed
        .get("tool_name")
        .or_else(|| parsed.get("tool"))
        .and_then(Value::as_str)
        .context("Hook event is missing tool_name")?;
    let body = parsed
        .get("tool_input")
        .or_else(|| parsed.get("input"))
        .unwrap_or(&Value::Null);
    let targets = collect_paths(body);
    let b = bundle(&root)?;
    let matches: Vec<_> = b
        .rules
        .iter()
        .filter(|r| {
            targets
                .iter()
                .any(|p| scope_match(&r.scope.include_paths, p))
                && risk_domain(&r.domain)
        })
        .map(|r| json!({"rule_id":r.rule_id,"domain":r.domain,"message":r.title}))
        .collect();
    let warn = if matches.is_empty() {
        String::new()
    } else {
        format!(
            "High-risk domain reminder before {tool}: review applicable Sker guidance. {}",
            matches
                .iter()
                .map(|m| m["message"].as_str().unwrap_or(""))
                .collect::<Vec<_>>()
                .join("; ")
        )
    };
    println!(
        "{}",
        json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":warn},"delivery_status":"delivered","blocked":hard_block&&!matches.is_empty()})
    );
    if hard_block && !matches.is_empty() {
        std::process::exit(1)
    }
    Ok(())
}
fn terms(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 1)
        .map(str::to_owned)
        .collect()
}
fn scope_match(globs: &[String], path: &str) -> bool {
    globs.is_empty()
        || globs.iter().any(|g| {
            let simple = g.trim_start_matches("**/").replace("**", "");
            path == g || path.ends_with(&simple) || g.trim_end_matches("*").is_empty()
        })
}
fn risk_domain(domain: &str) -> bool {
    let d = domain.to_lowercase();
    [
        "security",
        "auth",
        "payment",
        "privacy",
        "credential",
        "secret",
        "crypto",
    ]
    .iter()
    .any(|k| d.contains(k))
}
fn collect_paths(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    if ["path", "file_path", "filename"].contains(&k.as_str())
                        && let Some(s) = x.as_str()
                    {
                        out.push(s.to_owned())
                    }
                    walk(x, out)
                }
            }
            Value::Array(a) => {
                for x in a {
                    walk(x, out)
                }
            }
            _ => {}
        }
    }
    walk(v, &mut out);
    out
}

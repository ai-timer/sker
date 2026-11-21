use ast_grep_core::tree_sitter::LanguageExt;
use ast_grep_language::{Go, Python, Rust, SupportLang, Tsx, Yaml};
use globset::Glob;
use sker_schema::{
    AnalysisError, AnalysisRequest, AnalysisSummary, BaselineItem, CheckFinding, CheckReport,
    CheckStatus, FindingDisposition, RuleBundle, RuleSeverity, RuleTrack, compute_diff_tree_hash,
};

use crate::baseline::MatchAnchor;

pub(crate) fn analyze(
    bundle: &RuleBundle,
    request: &AnalysisRequest,
) -> Result<CheckReport, AnalysisError> {
    let mut findings_with_anchor = Vec::new();
    let mut unsupported = Vec::new();
    let mut changed = 0;
    let mut warnings = Vec::new();

    // Check review_by for all rules in bundle
    for rule in &bundle.rules {
        if let Some(ref rev) = rule.lifecycle.review_by {
            let is_past = rev.as_str() < request.now.as_str();
            if is_past {
                warnings.push(format!(
                    "Rule '{}' review date {} has passed",
                    rule.rule_id, rev
                ));
            }
        }
    }

    for file in &request.diff {
        let lines: Vec<_> = file
            .added_lines
            .iter()
            .chain(&file.modified_lines)
            .copied()
            .collect();
        changed += lines.len() as u64;
        if lines.is_empty() {
            continue;
        }
        let src = String::from_utf8_lossy(&file.new_blob);
        let ctx = CheckFileContext {
            file,
            lines: &lines,
            bundle,
            now: &request.now,
        };
        let ext = file.path.rsplit('.').next().unwrap_or("");
        let found = match ext {
            "go" => check_lang(Go, &src, &ctx, &mut findings_with_anchor, &mut warnings),
            "py" => check_lang(Python, &src, &ctx, &mut findings_with_anchor, &mut warnings),
            "ts" | "tsx" | "js" | "jsx" => {
                check_lang(Tsx, &src, &ctx, &mut findings_with_anchor, &mut warnings)
            }
            "yaml" | "yml" => {
                check_lang(Yaml, &src, &ctx, &mut findings_with_anchor, &mut warnings)
            }
            "rs" => check_lang(Rust, &src, &ctx, &mut findings_with_anchor, &mut warnings),
            "md" | "json" | "toml" | "txt" | "lock" | "gitignore" => {
                let scoped = bundle.rules.iter().any(|r| {
                    r.track == RuleTrack::Deterministic
                        && path_matches(&r.scope.include_paths, &r.scope.exclude_paths, &file.path)
                });
                !scoped
            }
            _ => false,
        };
        if !found {
            unsupported.push(file.path.clone());
        }
    }

    for sig_finding in crate::signals::extract(bundle, request) {
        let msg = sig_finding.message.clone();
        findings_with_anchor.push((
            sig_finding,
            MatchAnchor {
                syntax_anchor: None,
                symbol: None,
                norm: crate::baseline::normalize(&msg),
                hash: blake3::hash(msg.as_bytes()).to_hex().to_string(),
            },
        ));
    }

    crate::baseline::apply(&mut findings_with_anchor, &request.baseline);

    let mut findings: Vec<CheckFinding> =
        findings_with_anchor.into_iter().map(|(f, _)| f).collect();

    findings.sort_by(|a, b| {
        (&a.path, a.start_line, &a.rule_id).cmp(&(&b.path, b.start_line, &b.rule_id))
    });

    let complete = unsupported.is_empty();
    let has_deterministic_block = findings.iter().any(|f| {
        f.track == RuleTrack::Deterministic
            && f.severity == RuleSeverity::Block
            && f.disposition == FindingDisposition::Blocked
    });

    let status = if has_deterministic_block {
        CheckStatus::Blocked
    } else if complete {
        CheckStatus::Passed
    } else {
        CheckStatus::Incomplete
    };

    Ok(CheckReport {
        schema_version: "sker.check-report/v1".into(),
        status,
        bundle_sha256: if bundle.bundle_sha256.is_empty() {
            String::new()
        } else {
            bundle.bundle_sha256.clone()
        },
        diff_tree_hash: compute_diff_tree_hash(&request.diff),
        changed_line_count: changed,
        findings,
        analysis: AnalysisSummary {
            complete,
            unsupported_paths: unsupported,
            warnings,
        },
    })
}

pub fn generate_baseline(
    bundle: &RuleBundle,
    request: &AnalysisRequest,
) -> Result<Vec<BaselineItem>, AnalysisError> {
    let mut items = Vec::new();
    for file in &request.diff {
        let src = String::from_utf8_lossy(&file.new_blob);
        let ext = file.path.rsplit('.').next().unwrap_or("");
        match ext {
            "go" => collect_baseline_lang(Go, &src, file, bundle, &request.now, &mut items),
            "py" => collect_baseline_lang(Python, &src, file, bundle, &request.now, &mut items),
            "ts" | "tsx" | "js" | "jsx" => {
                collect_baseline_lang(Tsx, &src, file, bundle, &request.now, &mut items)
            }
            "yaml" | "yml" => {
                collect_baseline_lang(Yaml, &src, file, bundle, &request.now, &mut items)
            }
            "rs" => collect_baseline_lang(Rust, &src, file, bundle, &request.now, &mut items),
            _ => {}
        }
    }
    items.sort_by(|a, b| a.identity().cmp(&b.identity()));
    items.dedup_by(|a, b| a.identity() == b.identity());
    Ok(items)
}

struct CheckFileContext<'a> {
    file: &'a sker_schema::DiffEntry,
    lines: &'a [u32],
    bundle: &'a RuleBundle,
    now: &'a str,
}

fn check_lang<L>(
    lang: L,
    src: &str,
    ctx: &CheckFileContext<'_>,
    out: &mut Vec<(CheckFinding, MatchAnchor)>,
    warnings: &mut Vec<String>,
) -> bool
where
    L: ast_grep_core::Language + LanguageExt + Clone,
{
    let file = ctx.file;
    let lines = ctx.lines;
    let bundle = ctx.bundle;
    let now = ctx.now;
    let root = lang.ast_grep(src);
    for rule in bundle.rules.iter().filter(|r| {
        r.track == RuleTrack::Deterministic
            && path_matches(&r.scope.include_paths, &r.scope.exclude_paths, &file.path)
            && !r
                .exemptions
                .iter()
                .any(|e| e.paths.iter().any(|p| glob(p, &file.path)))
    }) {
        let is_expired_hotfix = rule.lifecycle.kind == sker_schema::LifecycleKind::IncidentHotfix
            && rule
                .lifecycle
                .hotfix_expires_at
                .as_deref()
                .is_some_and(|exp| exp < now);
        if is_expired_hotfix && let Some(ref exp) = rule.lifecycle.hotfix_expires_at {
            let msg = format!(
                "Rule '{}' incident hotfix expired at {}; BLOCK disposition suppressed",
                rule.rule_id, exp
            );
            if !warnings.contains(&msg) {
                warnings.push(msg);
            }
        }

        let mut matched_spans = std::collections::HashSet::new();

        // 1. Try detector YAML via ast-grep-config
        let det_yaml = rule.detector_content.as_deref().or_else(|| {
            let det = rule.detector.as_deref().or(rule.ast_rule.as_deref())?;
            if det.contains('\n')
                || (det.contains(':') && !det.ends_with(".yaml") && !det.ends_with(".yml"))
            {
                Some(det)
            } else {
                None
            }
        });
        if let Some(yaml) = det_yaml
            && let Ok(configs) =
                ast_grep_config::from_yaml_string::<SupportLang>(yaml, &Default::default())
        {
            for config in configs {
                let clang = config.language;
                let cgrep = clang.ast_grep(src);
                if let Ok(matcher) = config.get_matcher(&Default::default()) {
                    for n in cgrep.root().find_all(matcher) {
                        let start = n.start_pos().line() as u32 + 1;
                        let end = n.end_pos().line() as u32 + 1;
                        if !lines.iter().any(|l| *l >= start && *l <= end) {
                            continue;
                        }
                        matched_spans.insert((start, end));
                        let syntax_anchor = Some(n.kind().to_string());
                        let symbol = enclosing_symbol(&n);
                        let (norm, hash) = normalized_fragment(&n);

                        let disposition =
                            if rule.severity == RuleSeverity::Block && !is_expired_hotfix {
                                FindingDisposition::Blocked
                            } else {
                                FindingDisposition::Warning
                            };

                        out.push((
                            CheckFinding {
                                rule_id: rule.rule_id.clone(),
                                path: file.path.clone(),
                                start_line: start,
                                end_line: end,
                                track: rule.track,
                                severity: rule.severity,
                                disposition,
                                message: rule.title.clone(),
                                alternatives: rule
                                    .compliant_alternatives
                                    .clone()
                                    .unwrap_or_default(),
                                baseline_anchor: None,
                            },
                            MatchAnchor {
                                syntax_anchor,
                                symbol,
                                norm,
                                hash,
                            },
                        ));
                    }
                }
            }
        }

        // 2. Fallback / supplementary signals[].pattern
        for sig in &rule.signals {
            if sig.pattern.is_empty() {
                continue;
            }
            if !sig.languages.iter().any(|l| {
                matches!(l, sker_schema::Language::Any)
                    || matches!(
                        (l, file.path.rsplit('.').next().unwrap_or("")),
                        (sker_schema::Language::Go, "go")
                            | (sker_schema::Language::Python, "py")
                            | (
                                sker_schema::Language::Typescript,
                                "ts" | "tsx" | "js" | "jsx"
                            )
                            | (
                                sker_schema::Language::Yaml | sker_schema::Language::Infra,
                                "yaml" | "yml"
                            )
                            | (sker_schema::Language::Rust, "rs")
                    )
            }) {
                continue;
            }
            let pattern = ast_grep_core::Pattern::new(&sig.pattern, lang.clone());
            for n in root.root().find_all(pattern) {
                let start = n.start_pos().line() as u32 + 1;
                let end = n.end_pos().line() as u32 + 1;
                if matched_spans.contains(&(start, end)) {
                    continue;
                }
                if !lines.iter().any(|l| *l >= start && *l <= end) {
                    continue;
                }
                matched_spans.insert((start, end));
                let syntax_anchor = Some(n.kind().to_string());
                let symbol = enclosing_symbol(&n);
                let (norm, hash) = normalized_fragment(&n);

                let disposition = if rule.severity == RuleSeverity::Block && !is_expired_hotfix {
                    FindingDisposition::Blocked
                } else {
                    FindingDisposition::Warning
                };

                out.push((
                    CheckFinding {
                        rule_id: rule.rule_id.clone(),
                        path: file.path.clone(),
                        start_line: start,
                        end_line: end,
                        track: rule.track,
                        severity: rule.severity,
                        disposition,
                        message: rule.title.clone(),
                        alternatives: rule.compliant_alternatives.clone().unwrap_or_default(),
                        baseline_anchor: None,
                    },
                    MatchAnchor {
                        syntax_anchor,
                        symbol,
                        norm,
                        hash,
                    },
                ));
            }
        }
    }
    true
}

fn collect_baseline_lang<L>(
    lang: L,
    src: &str,
    file: &sker_schema::DiffEntry,
    bundle: &RuleBundle,
    now: &str,
    out: &mut Vec<BaselineItem>,
) where
    L: ast_grep_core::Language + LanguageExt + Clone,
{
    let root = lang.ast_grep(src);
    for rule in bundle.rules.iter().filter(|r| {
        r.track == RuleTrack::Deterministic
            && path_matches(&r.scope.include_paths, &r.scope.exclude_paths, &file.path)
            && !r
                .exemptions
                .iter()
                .any(|e| e.paths.iter().any(|p| glob(p, &file.path)))
    }) {
        let mut matched_spans = std::collections::HashSet::new();

        // 1. Try detector_content
        let det_yaml = rule.detector_content.as_deref().or_else(|| {
            let det = rule.detector.as_deref().or(rule.ast_rule.as_deref())?;
            if det.contains('\n')
                || (det.contains(':') && !det.ends_with(".yaml") && !det.ends_with(".yml"))
            {
                Some(det)
            } else {
                None
            }
        });
        if let Some(yaml) = det_yaml
            && let Ok(configs) =
                ast_grep_config::from_yaml_string::<SupportLang>(yaml, &Default::default())
        {
            for config in configs {
                let clang = config.language;
                let cgrep = clang.ast_grep(src);
                if let Ok(matcher) = config.get_matcher(&Default::default()) {
                    for n in cgrep.root().find_all(matcher) {
                        let start = n.start_pos().line() as u32 + 1;
                        let end = n.end_pos().line() as u32 + 1;
                        matched_spans.insert((start, end));
                        let syntax_anchor = Some(n.kind().to_string());
                        let symbol = enclosing_symbol(&n);
                        let (_norm, hash) = normalized_fragment(&n);
                        out.push(BaselineItem {
                            repository_path: file.path.clone(),
                            rule_id: rule.rule_id.clone(),
                            syntax_anchor,
                            symbol,
                            normalized_fragment: hash,
                            line_hint: Some(start),
                            exception: None,
                            audit: vec![format!("frozen at {now}")],
                        });
                    }
                }
            }
        }

        // 2. Signals
        for sig in &rule.signals {
            if !sig.languages.iter().any(|l| {
                matches!(l, sker_schema::Language::Any)
                    || matches!(
                        (l, file.path.rsplit('.').next().unwrap_or("")),
                        (sker_schema::Language::Go, "go")
                            | (sker_schema::Language::Python, "py")
                            | (
                                sker_schema::Language::Typescript,
                                "ts" | "tsx" | "js" | "jsx"
                            )
                            | (
                                sker_schema::Language::Yaml | sker_schema::Language::Infra,
                                "yaml" | "yml"
                            )
                            | (sker_schema::Language::Rust, "rs")
                    )
            }) {
                continue;
            }
            let pattern = ast_grep_core::Pattern::new(&sig.pattern, lang.clone());
            for n in root.root().find_all(pattern) {
                let start = n.start_pos().line() as u32 + 1;
                let end = n.end_pos().line() as u32 + 1;
                if matched_spans.contains(&(start, end)) {
                    continue;
                }
                matched_spans.insert((start, end));
                let syntax_anchor = Some(n.kind().to_string());
                let symbol = enclosing_symbol(&n);
                let (_norm, hash) = normalized_fragment(&n);
                out.push(BaselineItem {
                    repository_path: file.path.clone(),
                    rule_id: rule.rule_id.clone(),
                    syntax_anchor,
                    symbol,
                    normalized_fragment: hash,
                    line_hint: Some(start),
                    exception: None,
                    audit: vec![format!("frozen at {now}")],
                });
            }
        }
    }
}

pub(crate) fn enclosing_symbol<D: ast_grep_core::Doc>(
    n: &ast_grep_core::Node<D>,
) -> Option<String> {
    for a in n.ancestors() {
        let kind = a.kind();
        if kind.contains("function")
            || kind.contains("method")
            || kind.contains("class")
            || kind.contains("type")
            || kind.contains("fn")
            || kind.contains("def")
        {
            if let Some(name_node) = a.field("name") {
                let text = name_node.text().to_string();
                if !text.is_empty() {
                    return Some(text);
                }
            }
            for child in a.children() {
                if child.kind().contains("identifier") || child.kind() == "name" {
                    let text = child.text().to_string();
                    if !text.is_empty() {
                        return Some(text);
                    }
                }
            }
        }
    }
    None
}

pub(crate) fn normalized_fragment<D: ast_grep_core::Doc>(
    n: &ast_grep_core::Node<D>,
) -> (String, String) {
    let norm = n.text().split_whitespace().collect::<Vec<_>>().join(" ");
    let hash = blake3::hash(norm.as_bytes()).to_hex().to_string();
    (norm, hash)
}

pub(crate) fn glob(p: &str, path: &str) -> bool {
    Glob::new(p)
        .ok()
        .is_some_and(|g| g.compile_matcher().is_match(path))
}
pub(crate) fn path_matches(include: &[String], exclude: &[String], path: &str) -> bool {
    include.iter().any(|p| glob(p, path)) && !exclude.iter().any(|p| glob(p, path))
}

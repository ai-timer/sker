use ast_grep_core::{Language, tree_sitter::LanguageExt};
use ast_grep_language::{Go, Python, Tsx, Yaml};
use sker_schema::{
    AnalysisRequest, CheckFinding, FindingDisposition, RuleBundle, RuleSeverity, RuleTrack,
};
pub(crate) fn extract(bundle: &RuleBundle, request: &AnalysisRequest) -> Vec<CheckFinding> {
    let mut out = Vec::new();
    for file in &request.diff {
        let ext = file.path.rsplit('.').next().unwrap_or("");
        let src = String::from_utf8_lossy(&file.new_blob);
        match ext {
            "go" => scan(Go, &src, file, bundle, &mut out),
            "py" => scan(Python, &src, file, bundle, &mut out),
            "ts" | "tsx" | "js" | "jsx" => scan(Tsx, &src, file, bundle, &mut out),
            "yaml" | "yml" => scan(Yaml, &src, file, bundle, &mut out),
            _ => (),
        }
    }
    out
}
fn scan<L>(
    lang: L,
    src: &str,
    file: &sker_schema::DiffEntry,
    bundle: &RuleBundle,
    out: &mut Vec<CheckFinding>,
) where
    L: Language + LanguageExt + Clone,
{
    let root = lang.ast_grep(src);
    for rule in bundle.rules.iter().filter(|r| {
        r.track == RuleTrack::Semantic
            && crate::rules::path_matches(
                &r.scope.include_paths,
                &r.scope.exclude_paths,
                &file.path,
            )
            && !r
                .exemptions
                .iter()
                .any(|e| e.paths.iter().any(|p| crate::rules::glob(p, &file.path)))
    }) {
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
            && let Ok(configs) = ast_grep_config::from_yaml_string::<ast_grep_language::SupportLang>(
                yaml,
                &Default::default(),
            )
        {
            for config in configs {
                if let Ok(matcher) = config.get_matcher(&Default::default()) {
                    for node in root.root().find_all(matcher) {
                        let line = node.start_pos().line() as u32 + 1;
                        if file.added_lines.contains(&line) || file.modified_lines.contains(&line) {
                            out.push(CheckFinding {
                                rule_id: rule.rule_id.clone(),
                                path: file.path.clone(),
                                start_line: line,
                                end_line: node.end_pos().line() as u32 + 1,
                                track: RuleTrack::Semantic,
                                severity: rule.severity,
                                disposition: FindingDisposition::PendingSemantic,
                                message: "pending semantic judgment".into(),
                                alternatives: if rule.severity == RuleSeverity::Block {
                                    rule.compliant_alternatives.clone().unwrap_or_default()
                                } else {
                                    Vec::new()
                                },
                                baseline_anchor: None,
                            });
                        }
                    }
                }
            }
        }

        for sig in &rule.signals {
            if sig.pattern.is_empty() {
                continue;
            }
            let ext = file.path.rsplit('.').next().unwrap_or("");
            if !sig.languages.iter().any(|l| {
                matches!(l, sker_schema::Language::Any)
                    || matches!(
                        (l, ext),
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
                    )
            }) {
                continue;
            }
            let pattern = ast_grep_core::Pattern::new(&sig.pattern, lang.clone());
            for node in root.root().find_all(pattern) {
                let line = node.start_pos().line() as u32 + 1;
                if file.added_lines.contains(&line) || file.modified_lines.contains(&line) {
                    out.push(CheckFinding {
                        rule_id: rule.rule_id.clone(),
                        path: file.path.clone(),
                        start_line: line,
                        end_line: node.end_pos().line() as u32 + 1,
                        track: RuleTrack::Semantic,
                        severity: rule.severity,
                        disposition: FindingDisposition::PendingSemantic,
                        message: "pending semantic judgment".into(),
                        alternatives: if rule.severity == RuleSeverity::Block {
                            rule.compliant_alternatives.clone().unwrap_or_default()
                        } else {
                            Vec::new()
                        },
                        baseline_anchor: None,
                    })
                }
            }
        }
    }
}

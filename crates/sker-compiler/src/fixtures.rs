use ast_grep_core::matcher::Pattern;
use ast_grep_language::{LanguageExt, SupportLang};
use sker_schema::{
    CompileError, EntryFixture, FixturePolarity, FixtureTrigger, Language, RuleEntry, RuleTrack,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn fixture_path(path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .join(path)
    }
}

fn resolve_detector_yaml(rule: &RuleEntry, _path: &str) -> Option<String> {
    if let Some(ref c) = rule.detector_content {
        return Some(c.clone());
    }
    let det_ref = rule
        .detector
        .as_deref()
        .or(rule.ast_rule.as_deref())
        .or_else(|| {
            rule.signals
                .iter()
                .find_map(|s| s.detector.as_deref().or(s.ast_rule.as_deref()))
        })?;
    if det_ref.contains('\n')
        || (det_ref.contains(':') && !det_ref.ends_with(".yaml") && !det_ref.ends_with(".yml"))
    {
        return Some(det_ref.to_string());
    }
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../");
    let candidates = [
        repo_root.join(det_ref),
        repo_root.join("examples/sample-playbook").join(det_ref),
    ];
    for c in candidates {
        if c.exists()
            && let Ok(content) = fs::read_to_string(&c)
        {
            return Some(content);
        }
    }
    None
}

fn triggers(rule: &RuleEntry, source: &str, path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let inferred = match ext {
        "go" => Language::Go,
        "py" => Language::Python,
        "ts" | "tsx" => Language::Typescript,
        "yaml" | "yml" => Language::Yaml,
        "rs" => Language::Rust,
        _ => Language::Any,
    };

    if let Some(yaml) = resolve_detector_yaml(rule, path)
        && let Ok(configs) =
            ast_grep_config::from_yaml_string::<SupportLang>(&yaml, &Default::default())
    {
        for config in configs {
            let lang = config.language;
            let grep = lang.ast_grep(source);
            if let Ok(matcher) = config.get_matcher(&Default::default())
                && grep.root().find(matcher).is_some()
            {
                return true;
            }
        }
    }

    rule.signals
        .iter()
        .filter(|s| s.languages.contains(&inferred) || s.languages.contains(&Language::Any))
        .any(|signal| {
            if signal.pattern.is_empty() {
                return false;
            }
            signal
                .languages
                .iter()
                .find_map(|language| {
                    let alias = match language {
                        Language::Go => "go",
                        Language::Python => "python",
                        Language::Typescript => "typescript",
                        Language::Yaml | Language::Infra => "yaml",
                        Language::Rust => "rust",
                        Language::Any => return None,
                    };
                    let lang: SupportLang = alias.parse().ok()?;
                    let matcher = Pattern::new(&signal.pattern, lang);
                    Some(lang.ast_grep(source).root().find(matcher).is_some())
                })
                .unwrap_or_else(|| source.contains(&signal.pattern))
        })
}
fn validate_one(rule: &RuleEntry, fixture: &EntryFixture) -> Result<(), CompileError> {
    let path = fixture_path(&fixture.path);
    let source = fs::read_to_string(&path).map_err(|_| CompileError::FixtureFailure {
        rule_id: rule.rule_id.clone(),
        fixture_id: fixture.fixture_id.clone(),
    })?;
    let found = triggers(rule, &source, &fixture.path);
    let expected = fixture.expected == FixtureTrigger::Trigger;
    if found != expected {
        return Err(CompileError::FixtureFailure {
            rule_id: rule.rule_id.clone(),
            fixture_id: fixture.fixture_id.clone(),
        });
    }
    Ok(())
}
pub fn validate_fixtures(rules: &[RuleEntry]) -> Result<(), CompileError> {
    for rule in rules.iter().filter(|r| r.track == RuleTrack::Deterministic) {
        let mut positive = false;
        let mut negative = false;
        for fixture in &rule.fixtures {
            validate_one(rule, fixture)?;
            positive |= fixture.polarity == FixturePolarity::Positive;
            negative |= fixture.polarity == FixturePolarity::Negative;
        }
        if !positive || !negative {
            return Err(CompileError::FixtureFailure {
                rule_id: rule.rule_id.clone(),
                fixture_id: "missing polarity".into(),
            });
        }
    }
    Ok(())
}

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
        _ => Language::Any,
    };
    rule.signals
        .iter()
        .filter(|s| s.languages.contains(&inferred) || s.languages.contains(&Language::Any))
        .any(|signal| {
            signal
                .languages
                .iter()
                .find_map(|language| {
                    let alias = match language {
                        Language::Go => "go",
                        Language::Python => "python",
                        Language::Typescript => "typescript",
                        Language::Yaml | Language::Infra => "yaml",
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

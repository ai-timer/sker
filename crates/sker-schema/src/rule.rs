use std::collections::{BTreeMap, BTreeSet, VecDeque};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::errors::{CompileError, SchemaError};

/// Descending sort key: literal path segments, literal characters, path depth,
/// then inverse glob complexity. This makes exact and narrower scopes win.
pub type ScopeSpecificity = (u16, u16, u16, u16);

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct RuleEntry {
    pub schema_version: String,
    pub rule_id: String,
    pub version: String,
    pub title: String,
    pub domain: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    pub severity: RuleSeverity,
    pub track: RuleTrack,
    pub scope: EntryScope,
    pub signals: Vec<RuleSignal>,
    pub exemptions: Vec<Exemption>,
    pub overrides: Vec<OverrideEdge>,
    pub fixtures: Vec<EntryFixture>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compliant_alternatives: Option<Vec<String>>,
    pub lifecycle: Lifecycle,
    pub consult: ConsultMetadata,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl RuleEntry {
    pub fn validate(&self) -> Result<(), SchemaError> {
        if self.schema_version != "sker.entry/v1" {
            return Err(SchemaError::InvalidEntry(
                "schema_version must be sker.entry/v1".into(),
            ));
        }
        if !identifier_is_valid(&self.rule_id, 128)
            || self.version.is_empty()
            || self.version.len() > 64
            || self.title.trim().len() < 3
            || self.title.len() > 200
            || self.domain.is_empty()
            || self.domain.len() > 128
            || !self.domain.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'/' | b'-')
            })
            || self.description.is_empty()
            || self.description.len() > 10_000
            || self
                .rationale
                .as_ref()
                .is_some_and(|value| value.len() > 10_000)
        {
            return Err(SchemaError::InvalidEntry(
                "rule identity or text fields violate Entry v1 bounds".into(),
            ));
        }
        if self.scope.include_paths.is_empty()
            || has_duplicates(&self.scope.include_paths)
            || has_duplicates(&self.scope.exclude_paths)
        {
            return Err(SchemaError::InvalidEntry(
                "scope.include_paths must not be empty".into(),
            ));
        }
        if self.signals.is_empty() {
            return Err(SchemaError::InvalidEntry(
                "signals must not be empty".into(),
            ));
        }
        for signal in &self.signals {
            if !identifier_is_valid(&signal.signal_id, 128)
                || signal.languages.is_empty()
                || signal.description.is_empty()
                || signal.description.len() > 2_000
                || signal.pattern.is_empty()
                || signal.pattern.len() > 4_096
                || has_duplicates(&signal.languages)
            {
                return Err(SchemaError::InvalidEntry(format!(
                    "signal {} violates Entry v1 bounds",
                    signal.signal_id
                )));
            }
        }
        if has_duplicates(
            &self
                .signals
                .iter()
                .map(|signal| signal.signal_id.as_str())
                .collect::<Vec<_>>(),
        ) {
            return Err(SchemaError::InvalidEntry(
                "signal_id values must be unique".into(),
            ));
        }
        if self.track == RuleTrack::Deterministic {
            let alternatives = self
                .compliant_alternatives
                .as_ref()
                .is_some_and(|items| !items.is_empty());
            let positive = self.fixtures.iter().any(|fixture| {
                fixture.polarity == FixturePolarity::Positive
                    && fixture.expected == FixtureTrigger::Trigger
            });
            let negative = self.fixtures.iter().any(|fixture| {
                fixture.polarity == FixturePolarity::Negative
                    && fixture.expected == FixtureTrigger::NoTrigger
            });
            if !alternatives || !positive || !negative {
                return Err(SchemaError::InvalidEntry(
                    "deterministic rules require compliant_alternatives and positive/negative fixtures".into(),
                ));
            }
        }
        for exemption in &self.exemptions {
            if exemption.exemption_id.trim().is_empty()
                || exemption.exemption_id.len() > 128
                || exemption.paths.is_empty()
                || exemption.reason.is_empty()
                || exemption.reason.len() > 4_000
                || exemption.approved_by.is_empty()
                || exemption.approved_by.len() > 256
                || !looks_like_datetime(&exemption.expires_at)
                || has_duplicates(&exemption.paths)
            {
                return Err(SchemaError::InvalidEntry(format!(
                    "exemption {} violates Entry v1 bounds",
                    exemption.exemption_id
                )));
            }
            for pattern in &exemption.paths {
                validate_repo_pattern(pattern)?;
            }
        }
        for edge in &self.overrides {
            if edge.target_rule_id.is_empty()
                || edge.target_rule_id.len() > 128
                || edge.reason.is_empty()
                || edge.reason.len() > 4_000
            {
                return Err(SchemaError::InvalidEntry(
                    "override edge violates Entry v1 bounds".into(),
                ));
            }
        }
        for fixture in &self.fixtures {
            if fixture.fixture_id.is_empty()
                || fixture.fixture_id.len() > 128
                || !fixture_path_is_valid(&fixture.path)
            {
                return Err(SchemaError::InvalidEntry(format!(
                    "fixture {} has an invalid path or ID",
                    fixture.fixture_id
                )));
            }
            let expected = match fixture.polarity {
                FixturePolarity::Positive => FixtureTrigger::Trigger,
                FixturePolarity::Negative => FixtureTrigger::NoTrigger,
            };
            if fixture.expected != expected {
                return Err(SchemaError::InvalidEntry(format!(
                    "fixture {} polarity and expected result disagree",
                    fixture.fixture_id
                )));
            }
        }
        if self.compliant_alternatives.as_ref().is_some_and(|items| {
            items
                .iter()
                .any(|item| item.is_empty() || item.len() > 2_000)
                || has_duplicates(items)
        }) {
            return Err(SchemaError::InvalidEntry("compliant_alternatives must contain unique non-empty strings up to 2000 characters".into()));
        }
        if has_duplicates(&self.consult.synonyms)
            || has_duplicates(&self.consult.keywords)
            || self
                .consult
                .synonyms
                .iter()
                .chain(&self.consult.keywords)
                .any(|word| word.is_empty() || word.len() > 128)
            || has_duplicates(&self.tags)
            || self.tags.iter().any(|tag| tag.is_empty() || tag.len() > 64)
        {
            return Err(SchemaError::InvalidEntry(
                "consult metadata or tags violate Entry v1 bounds".into(),
            ));
        }
        match &self.lifecycle.kind {
            LifecycleKind::Ordinary
                if self.lifecycle.cooldown_hours != Some(24)
                    || self.lifecycle.hotfix_expires_at.is_some() =>
            {
                return Err(SchemaError::InvalidEntry(
                    "ordinary lifecycle requires cooldown_hours=24 and no hotfix expiry".into(),
                ));
            }
            LifecycleKind::IncidentHotfix
                if self
                    .lifecycle
                    .hotfix_expires_at
                    .as_deref()
                    .is_none_or(str::is_empty)
                    || self.lifecycle.cooldown_hours.is_some() =>
            {
                return Err(SchemaError::InvalidEntry(
                    "incident hotfix lifecycle requires hotfix_expires_at and no cooldown".into(),
                ));
            }
            _ => {}
        }
        for pattern in self
            .scope
            .include_paths
            .iter()
            .chain(&self.scope.exclude_paths)
        {
            validate_repo_pattern(pattern)?;
        }
        Ok(())
    }
}

fn identifier_is_valid(value: &str, max_length: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_length
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn has_duplicates<T: Ord>(values: &[T]) -> bool {
    let mut sorted: Vec<_> = values.iter().collect();
    sorted.sort();
    sorted.windows(2).any(|window| window[0] == window[1])
}

fn looks_like_datetime(value: &str) -> bool {
    let separator = value.find('T').unwrap_or(value.len());
    value.contains('T')
        && (value.ends_with('Z')
            || value
                .char_indices()
                .rev()
                .find(|(index, character)| *index > separator && matches!(character, '+' | '-'))
                .is_some())
}

fn fixture_path_is_valid(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && !path.starts_with('/')
        && !path.split('/').any(|part| part == "..")
        && !path
            .chars()
            .any(|ch| matches!(ch, '*' | '?' | '[' | ']' | '{' | '}'))
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct EntryScope {
    pub include_paths: Vec<String>,
    #[serde(default)]
    pub exclude_paths: Vec<String>,
}

impl EntryScope {
    pub fn specificity_tuple(&self, repository_path: &str) -> Option<ScopeSpecificity> {
        if self
            .exclude_paths
            .iter()
            .any(|pattern| glob_matches(pattern, repository_path))
        {
            return None;
        }
        self.include_paths
            .iter()
            .filter(|pattern| glob_matches(pattern, repository_path))
            .map(|pattern| pattern_specificity(pattern))
            .max()
    }
}

pub fn pattern_specificity(pattern: &str) -> ScopeSpecificity {
    let literal_segments = pattern
        .split('/')
        .filter(|segment| !segment.is_empty() && !segment.contains(['*', '?', '[', ']']))
        .count()
        .min(u16::MAX as usize) as u16;
    let literal_characters = pattern
        .chars()
        .filter(|ch| !matches!(ch, '*' | '?' | '[' | ']' | '{' | '}'))
        .count()
        .min(u16::MAX as usize) as u16;
    let depth = pattern.matches('/').count().min(u16::MAX as usize) as u16;
    let wildcards = pattern
        .chars()
        .filter(|ch| matches!(ch, '*' | '?' | '[' | ']' | '{' | '}'))
        .count();
    let inverse_complexity = u16::MAX.saturating_sub(wildcards.min(u16::MAX as usize) as u16);
    (
        literal_segments,
        literal_characters,
        depth,
        inverse_complexity,
    )
}

fn validate_repo_pattern(pattern: &str) -> Result<(), SchemaError> {
    if pattern.is_empty()
        || pattern.len() > 512
        || pattern.starts_with('/')
        || pattern.split('/').any(|part| part == "..")
    {
        return Err(SchemaError::InvalidPath {
            path: pattern.to_owned(),
            reason:
                "path patterns must be repository-relative and cannot traverse parent directories"
                    .into(),
        });
    }
    Ok(())
}

fn glob_matches(pattern: &str, path: &str) -> bool {
    let pattern_parts: Vec<_> = pattern.split('/').collect();
    let path_parts: Vec<_> = path.split('/').collect();
    glob_parts_match(&pattern_parts, &path_parts)
}

fn glob_parts_match(pattern: &[&str], path: &[&str]) -> bool {
    match (pattern.first(), path.first()) {
        (None, None) => true,
        (Some(&"**"), _) => {
            glob_parts_match(&pattern[1..], path)
                || (!path.is_empty() && glob_parts_match(pattern, &path[1..]))
        }
        (Some(p), Some(s)) => {
            segment_glob_matches(p, s) && glob_parts_match(&pattern[1..], &path[1..])
        }
        _ => false,
    }
}

fn segment_glob_matches(pattern: &str, value: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let v: Vec<char> = value.chars().collect();
    let (mut pi, mut vi, mut star, mut retry) = (0, 0, None, 0);
    while vi < v.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == v[vi]) {
            pi += 1;
            vi += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            pi += 1;
            retry = vi;
        } else if let Some(star_at) = star {
            retry += 1;
            vi = retry;
            pi = star_at + 1;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "UPPERCASE")]
pub enum RuleSeverity {
    Block,
    Warn,
    Info,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum RuleTrack {
    Deterministic,
    Semantic,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct RuleSignal {
    pub signal_id: String,
    pub languages: Vec<Language>,
    pub description: String,
    pub pattern: String,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Go,
    Python,
    Typescript,
    Yaml,
    Infra,
    Any,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct Exemption {
    pub exemption_id: String,
    pub paths: Vec<String>,
    pub reason: String,
    pub expires_at: String,
    pub approved_by: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct OverrideEdge {
    pub target_rule_id: String,
    pub relation: OverrideRelation,
    pub reason: String,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum OverrideRelation {
    Supersedes,
    Narrows,
    Requires,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OverrideGraph {
    nodes: BTreeSet<String>,
    /// Each pair is `(overriding_rule, target_rule)`.
    edges: BTreeSet<(String, String)>,
}

impl OverrideGraph {
    pub fn add_rule(&mut self, rule: &RuleEntry) {
        self.nodes.insert(rule.rule_id.clone());
        for edge in &rule.overrides {
            self.nodes.insert(edge.target_rule_id.clone());
            self.edges
                .insert((rule.rule_id.clone(), edge.target_rule_id.clone()));
        }
    }

    pub fn add_edge(&mut self, overriding_rule: impl Into<String>, target_rule: impl Into<String>) {
        let overriding_rule = overriding_rule.into();
        let target_rule = target_rule.into();
        self.nodes.insert(overriding_rule.clone());
        self.nodes.insert(target_rule.clone());
        self.edges.insert((overriding_rule, target_rule));
    }

    /// Returns dependencies before dependants; an override target is emitted
    /// before the rule that overrides it. A cycle returns its involved IDs.
    pub fn topological_order(&self) -> Result<Vec<String>, CompileError> {
        let mut outgoing: BTreeMap<String, BTreeSet<String>> = self
            .nodes
            .iter()
            .cloned()
            .map(|id| (id, BTreeSet::new()))
            .collect();
        let mut indegree: BTreeMap<String, usize> =
            self.nodes.iter().cloned().map(|id| (id, 0)).collect();
        for (overriding, target) in &self.edges {
            if outgoing
                .get_mut(target)
                .expect("edge endpoints are nodes")
                .insert(overriding.clone())
            {
                *indegree
                    .get_mut(overriding)
                    .expect("edge endpoints are nodes") += 1;
            }
        }
        let mut ready: VecDeque<String> = indegree
            .iter()
            .filter(|(_, degree)| **degree == 0)
            .map(|(id, _)| id.clone())
            .collect();
        let mut ordered = Vec::with_capacity(self.nodes.len());
        while let Some(node) = ready.pop_front() {
            ordered.push(node.clone());
            if let Some(children) = outgoing.get(&node) {
                for child in children {
                    let degree = indegree.get_mut(child).expect("child is a node");
                    *degree -= 1;
                    if *degree == 0 {
                        ready.push_back(child.clone());
                    }
                }
            }
        }
        if ordered.len() != self.nodes.len() {
            let cycle_ids = indegree
                .into_iter()
                .filter_map(|(id, degree)| (degree > 0).then_some(id))
                .collect();
            return Err(CompileError::OverrideCycle {
                rule_ids: cycle_ids,
            });
        }
        Ok(ordered)
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum FixturePolarity {
    Positive,
    Negative,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum FixtureTrigger {
    Trigger,
    NoTrigger,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct EntryFixture {
    pub fixture_id: String,
    pub polarity: FixturePolarity,
    pub path: String,
    pub expected: FixtureTrigger,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct Lifecycle {
    pub kind: LifecycleKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_hours: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotfix_expires_at: Option<String>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleKind {
    Ordinary,
    IncidentHotfix,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct ConsultMetadata {
    pub synonyms: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

use crate::fixtures::validate_fixtures;
use sker_schema::{
    CompileError, OverrideGraph, RuleEntry, RuleSeverity, RuleSource, RuleSourceKind,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn merge(
    sources: &[RuleSource],
    fixtures: bool,
) -> Result<(Vec<sker_schema::SourceLock>, Vec<RuleEntry>), CompileError> {
    let mut locks = Vec::new();
    let mut rules = BTreeMap::<String, RuleEntry>::new();
    for source in sources {
        source
            .lock
            .validate()
            .map_err(|e| CompileError::BundleSerialization {
                detail: e.to_string(),
            })?;
        locks.push(source.lock.clone());
        for rule in &source.rules {
            rule.validate().map_err(|e| CompileError::InvalidEntry {
                rule_id: Some(rule.rule_id.clone()),
                detail: e.to_string(),
            })?;
            if rules.insert(rule.rule_id.clone(), rule.clone()).is_some() {
                return Err(CompileError::RuleConflict {
                    rule_ids: vec![rule.rule_id.clone()],
                });
            }
        }
    }
    let mut graph = OverrideGraph::default();
    for rule in rules.values() {
        graph.add_rule(rule);
    }
    if let Err(CompileError::OverrideCycle { rule_ids }) = graph.topological_order() {
        return Err(CompileError::OverrideCycle { rule_ids });
    }
    for (id, rule) in &rules {
        if rule.severity != RuleSeverity::Block {
            continue;
        }
        for source in sources
            .iter()
            .filter(|s| s.lock.kind != RuleSourceKind::CentralPlaybook)
        {
            for candidate in &source.rules {
                let weakens = candidate
                    .overrides
                    .iter()
                    .any(|edge| edge.target_rule_id == *id)
                    && (candidate.severity != RuleSeverity::Block
                        || !candidate.exemptions.is_empty());
                if weakens {
                    return Err(CompileError::CentralBlockDowngrade {
                        rule_id: id.clone(),
                        override_id: candidate.rule_id.clone(),
                    });
                }
            }
        }
        if !rule.exemptions.is_empty()
            && sources.iter().any(|s| {
                s.lock.kind == RuleSourceKind::CentralPlaybook
                    && s.rules.iter().any(|r| r.rule_id == *id)
            })
        { /* central's own exemptions are schema controlled */ }
    }
    let mut seen = BTreeSet::new();
    for lock in &locks {
        if !seen.insert((&lock.source_id, &lock.version)) {
            return Err(CompileError::RuleConflict {
                rule_ids: vec![lock.source_id.clone()],
            });
        }
    }
    let rules: Vec<_> = rules.into_values().collect();
    if fixtures {
        validate_fixtures(&rules)?;
    }
    Ok((locks, rules))
}

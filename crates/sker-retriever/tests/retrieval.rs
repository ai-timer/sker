use sker_retriever::TantivyRetriever;
use sker_schema::{
    BundleMetadata, ConsultQuery, IntentRetriever, RuleBundle, RuleSourceKind, SourceLock,
    compute_bundle_sha256,
};

fn bundle(rule_count: usize) -> RuleBundle {
    let rule: sker_schema::RuleEntry = serde_yaml_ng::from_str(include_str!(
        "../../../examples/sample-playbook/rules/go-no-logging.yaml"
    ))
    .unwrap();
    let mut rules = Vec::with_capacity(rule_count);
    for index in 0..rule_count {
        let mut item = rule.clone();
        item.rule_id = format!("go.no-fmt-printf-{index:03}");
        rules.push(item);
    }
    let lock = SourceLock {
        kind: RuleSourceKind::RepositoryLocal,
        source_id: "tests".into(),
        version: "1".into(),
        content_hash: "a".repeat(64),
    };
    let hash = compute_bundle_sha256(std::slice::from_ref(&lock), &rules).unwrap();
    RuleBundle {
        schema_version: "sker.bundle/v1".into(),
        bundle_id: hash.clone(),
        bundle_sha256: hash,
        sources: vec![lock],
        rules,
        metadata: BundleMetadata {
            created_at: "2026-11-18T00:00:00Z".into(),
            compiler_version: "test".into(),
        },
    }
}

#[test]
fn consult_ranks_synonyms_scopes_paths_and_returns_honest_no_match() {
    let retriever = TantivyRetriever::default();
    let bundle = bundle(1);
    let result = retriever
        .consult(
            &bundle,
            &ConsultQuery {
                intent: "slog".into(),
                paths: vec!["services/api/main.go".into()],
                limit: Some(4),
                minimum_score: None,
            },
        )
        .unwrap();
    assert_eq!(
        result.matches.first().unwrap().rule_id,
        "go.no-fmt-printf-000"
    );
    let wrong_path = retriever
        .consult(
            &bundle,
            &ConsultQuery {
                intent: "logging".into(),
                paths: vec!["docs/readme.md".into()],
                limit: None,
                minimum_score: None,
            },
        )
        .unwrap();
    assert!(wrong_path.matches.is_empty());
    let below = retriever
        .consult(
            &bundle,
            &ConsultQuery {
                intent: "unrelated quantum aerospace".into(),
                paths: vec!["services/api/main.go".into()],
                limit: None,
                minimum_score: Some(0.99),
            },
        )
        .unwrap();
    assert!(below.matches.is_empty());
    let over_threshold = retriever
        .consult(
            &bundle,
            &ConsultQuery {
                intent: "slog".into(),
                paths: vec!["services/api/main.go".into()],
                limit: None,
                minimum_score: Some(0.96),
            },
        )
        .unwrap();
    assert!(over_threshold.matches.is_empty());
}

#[test]
fn offline_500_rule_results_remain_bounded() {
    let retriever = TantivyRetriever::default();
    let bundle = bundle(500);
    let result = retriever
        .consult(
            &bundle,
            &ConsultQuery {
                intent: "slog".into(),
                paths: vec!["services/api/main.go".into()],
                limit: Some(7),
                minimum_score: None,
            },
        )
        .unwrap();
    assert_eq!(result.matches.len(), 7);
}

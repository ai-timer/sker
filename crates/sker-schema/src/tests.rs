use crate::{
    AgentCredential, BundleMetadata, ConsultResultStatus, ConsultTraceEntry, CredentialIssuer,
    DiffEntry, DiffStatus, EntryScope, HashChain, HashDigest, OverrideGraph, RuleBundle, RuleEntry,
    RuleSourceKind, RuleTrack, SemanticVerdict, SemanticVerdictState, SourceLock, archive_bundle,
    compute_bundle_sha256, validate_bundle_archive,
};

fn sample_go_rule() -> RuleEntry {
    serde_yaml_ng::from_str(include_str!(
        "../../../examples/sample-playbook/rules/go-no-logging.yaml"
    ))
    .expect("sample Go rule parses")
}

#[test]
fn scope_specificity_is_stable_and_exclusions_win() {
    let scope = EntryScope {
        include_paths: vec!["src/**/*.go".into(), "src/services/billing/*.go".into()],
        exclude_paths: vec!["**/generated/**".into()],
    };
    let specific = scope
        .specificity_tuple("src/services/billing/handler.go")
        .unwrap();
    assert_eq!(
        specific,
        crate::pattern_specificity("src/services/billing/*.go")
    );
    assert!(
        scope
            .specificity_tuple("src/services/billing/generated/handler.go")
            .is_none()
    );
    assert!(scope.specificity_tuple("other/service.go").is_none());
}

#[test]
fn override_graph_orders_dependencies_and_reports_cycles() {
    let mut graph = OverrideGraph::default();
    graph.add_edge("local", "business");
    graph.add_edge("business", "central");
    assert_eq!(
        graph.topological_order().unwrap(),
        vec!["central", "business", "local"]
    );

    graph.add_edge("central", "local");
    let error = graph.topological_order().unwrap_err();
    assert!(matches!(error, crate::CompileError::OverrideCycle { .. }));
}

#[test]
fn blake3_hash_chain_detects_payload_changes() {
    let mut chain = HashChain::default();
    chain.append(b"first");
    chain.append(b"second");
    assert!(chain.verify(&[b"first".to_vec(), b"second".to_vec()]));
    assert!(!chain.verify(&[b"first".to_vec(), b"tampered".to_vec()]));
    assert!(HashDigest::new("a".repeat(64)).is_ok());
    assert!(HashDigest::new("A".repeat(64)).is_err());
}

#[test]
fn bundle_hash_is_order_independent_and_rkyv_archive_is_validated() {
    let rule = sample_go_rule();
    let lock = SourceLock {
        kind: RuleSourceKind::RepositoryLocal,
        source_id: "sample".into(),
        version: "1".into(),
        content_hash: "b".repeat(64),
    };
    let sha =
        compute_bundle_sha256(std::slice::from_ref(&lock), std::slice::from_ref(&rule)).unwrap();
    let bundle = RuleBundle {
        schema_version: "sker.bundle/v1".into(),
        bundle_id: sha.clone(),
        bundle_sha256: sha,
        sources: vec![lock],
        rules: vec![rule],
        metadata: BundleMetadata {
            created_at: "2026-11-18T00:00:00Z".into(),
            compiler_version: "0.1.0-preview".into(),
        },
    };
    let bytes = archive_bundle(&bundle).unwrap();
    assert_eq!(validate_bundle_archive(&bytes).unwrap(), bundle);
    let mut corrupted = bytes.to_vec();
    corrupted[0] ^= 0xff;
    assert!(validate_bundle_archive(&corrupted).is_err());
}

#[test]
fn deterministic_entry_requires_both_fixture_polarities_and_alternatives() {
    let mut rule = sample_go_rule();
    assert_eq!(rule.track, RuleTrack::Deterministic);
    rule.fixtures.clear();
    assert!(rule.validate().is_err());
}

#[test]
fn credential_digest_uses_stable_canonical_json_and_excludes_its_own_field() {
    let credential = AgentCredential {
        schema_version: "sker.agent-credential/v1".into(),
        bundle_sha256: "a".repeat(64),
        diff_tree_hash: "b".repeat(64),
        consult_trace: vec![ConsultTraceEntry {
            consult_id: "c1".into(),
            occurred_at: "2026-11-18T00:00:00Z".into(),
            query_digest: "c".repeat(64),
            paths: vec!["src/lib.rs".into()],
            matched_rule_ids: vec!["rule.one".into()],
            result_status: ConsultResultStatus::Matched,
        }],
        semantic_verdicts: vec![SemanticVerdict {
            rule_id: "semantic.one".into(),
            state: SemanticVerdictState::Comply,
            declaration: "notification path is idempotent".into(),
            rationale_digest: None,
            evidence_path: None,
            evidence_line: None,
        }],
        issuer: CredentialIssuer::LocalCli,
        created_at: "2026-11-18T00:00:00Z".into(),
        credential_blake3: String::new(),
    };
    let digest = crate::credential_blake3(&credential).unwrap();
    assert_eq!(digest.len(), 64);
    let mut finalized = credential;
    finalized.credential_blake3 = digest.clone();
    assert_eq!(crate::credential_blake3(&finalized).unwrap(), digest);
    let value = serde_json::json!({"z": "last", "a": 1});
    assert_eq!(
        crate::canonical_json_bytes(&value).unwrap(),
        br#"{"a":1,"z":"last"}"#
    );
}

#[test]
fn diff_tree_hash_is_path_sorted_and_binds_modes_and_blobs() {
    let first = DiffEntry {
        path: "src/a.rs".into(),
        status: DiffStatus::Modified,
        old_mode: Some(0o100644),
        new_mode: Some(0o100644),
        old_blob: b"old".to_vec(),
        new_blob: b"new".to_vec(),
        added_lines: vec![1],
        modified_lines: vec![1],
    };
    let second = DiffEntry {
        path: "src/b.rs".into(),
        status: DiffStatus::Added,
        old_mode: None,
        new_mode: Some(0o100644),
        old_blob: vec![],
        new_blob: b"created".to_vec(),
        added_lines: vec![1],
        modified_lines: vec![],
    };
    let expected = crate::compute_diff_tree_hash(&[first.clone(), second.clone()]);
    assert_eq!(
        expected,
        crate::compute_diff_tree_hash(&[second.clone(), first.clone()])
    );
    let mut changed = second;
    changed.new_blob.push(b'!');
    assert_ne!(expected, crate::compute_diff_tree_hash(&[first, changed]));
}

use sker_compiler::{load_playbook, validate_fixtures};
use sker_schema::{FixtureTrigger, RuleTrack};

#[test]
fn sample_deterministic_rule_positive_and_negative_fixtures_pass() {
    let source = load_playbook(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/sample-playbook"
    ))
    .unwrap();
    let deterministic: Vec<_> = source
        .rules
        .into_iter()
        .filter(|r| r.track == RuleTrack::Deterministic)
        .collect();
    assert_eq!(deterministic.len(), 3);
    validate_fixtures(&deterministic).unwrap();
}

#[test]
fn broken_fixture_polarity_rejects_publication() {
    let mut source = load_playbook(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/sample-playbook"
    ))
    .unwrap();
    let rule = source
        .rules
        .iter_mut()
        .find(|r| r.track == RuleTrack::Deterministic)
        .unwrap();
    rule.fixtures
        .iter_mut()
        .find(|f| f.expected == FixtureTrigger::NoTrigger)
        .unwrap()
        .expected = FixtureTrigger::Trigger;
    assert!(validate_fixtures(&source.rules).is_err());
}

#[test]
fn locked_source_content_hash_mismatch_is_rejected() {
    let root = std::env::temp_dir().join(format!("sker-source-lock-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("rule.yaml"), "schema_version: sker.entry/v1\n").unwrap();
    std::fs::write(root.join("lock.yaml"), "schema_version: sker.sources/v1\nsources:\n  - kind: business_lock\n    source_id: business\n    version: pinned-rev\n    path: rule.yaml\n    content_hash: ").unwrap();
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(root.join("lock.yaml"))
        .unwrap()
        .write_all(format!("{}\n", "0".repeat(64)).as_bytes())
        .unwrap();
    let result = sker_compiler::load_sources(&root, root.join("lock.yaml"));
    assert!(matches!(
        result,
        Err(sker_schema::CompileError::LockMismatch { .. })
    ));
    let _ = std::fs::remove_dir_all(root);
}

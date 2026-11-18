use serde_yaml_ng::Value;
use sker_schema::{RuleEntry, RuleTrack};

#[test]
fn playbook_has_deterministic_go_detector_and_polarity_fixtures() {
    let manifest: Value =
        serde_yaml_ng::from_str(include_str!("../../../examples/sample-playbook/sker.yaml"))
            .unwrap();
    assert_eq!(
        manifest["schema_version"].as_str(),
        Some("sker.playbook/v1")
    );
    let entry: RuleEntry = serde_yaml_ng::from_str(include_str!(
        "../../../examples/sample-playbook/rules/go-no-logging.yaml"
    ))
    .unwrap();
    entry.validate().unwrap();
    assert_eq!(entry.track, RuleTrack::Deterministic);
    let detector: Value = serde_yaml_ng::from_str(include_str!(
        "../../../examples/sample-playbook/rules/go-no-logging.yml"
    ))
    .unwrap();
    assert_eq!(detector["language"].as_str(), Some("Go"));
    assert_eq!(
        detector["rule"]["pattern"].as_str(),
        Some("fmt.Printf($FMT, $$$ARGS)")
    );
    assert!(
        include_str!(
            "../../../examples/sample-playbook/fixtures/go-no-logging/positive/no_logging.go"
        )
        .contains("fmt.Printf")
    );
    assert!(
        include_str!(
            "../../../examples/sample-playbook/fixtures/go-no-logging/negative/allowed_logging.go"
        )
        .contains("logger.Info")
    );
}

#[test]
fn semantic_cross_service_rule_has_go_python_typescript_and_yaml_signals() {
    let entry: RuleEntry = serde_yaml_ng::from_str(include_str!(
        "../../../examples/sample-playbook/rules/cross-service-notification.yaml"
    ))
    .unwrap();
    entry.validate().unwrap();
    assert_eq!(entry.track, RuleTrack::Semantic);
    let languages = &entry.signals[0].languages;
    assert!(languages.contains(&sker_schema::Language::Go));
    assert!(languages.contains(&sker_schema::Language::Python));
    assert!(languages.contains(&sker_schema::Language::Typescript));
    assert!(languages.contains(&sker_schema::Language::Yaml));
    let present: Value = serde_yaml_ng::from_str(include_str!(
        "../../../examples/sample-playbook/fixtures/cross-service-notification/signal_present.yaml"
    ))
    .unwrap();
    let absent: Value = serde_yaml_ng::from_str(include_str!(
        "../../../examples/sample-playbook/fixtures/cross-service-notification/signal_absent.yaml"
    ))
    .unwrap();
    assert_eq!(present["signal_expected"].as_bool(), Some(true));
    assert_eq!(absent["signal_expected"].as_bool(), Some(false));
}

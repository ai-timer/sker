use serde_json::Value;
use sker_schema::{RuleEntry, RuleSeverity, RuleTrack};

const ENTRY_SCHEMA: &str =
    include_str!("../../../specs/002-sker-release-preview/contracts/entry-schema.json");

#[test]
fn entry_schema_contract_declares_all_required_fields_and_closed_objects() {
    let schema: Value = serde_json::from_str(ENTRY_SCHEMA).unwrap();
    let required = schema["required"].as_array().unwrap();
    for field in [
        "schema_version",
        "rule_id",
        "version",
        "title",
        "domain",
        "description",
        "severity",
        "track",
        "scope",
        "signals",
        "exemptions",
        "overrides",
        "fixtures",
        "lifecycle",
        "consult",
    ] {
        assert!(
            required.iter().any(|item| item == field),
            "missing required Entry field {field}"
        );
        assert!(
            schema["properties"].get(field).is_some(),
            "Entry field {field} has no schema property"
        );
    }
    assert_eq!(schema["additionalProperties"], false);
    let severity = schema["properties"]["severity"]["enum"].as_array().unwrap();
    for value in ["BLOCK", "WARN", "INFO"] {
        assert!(severity.iter().any(|item| item == value));
    }
}

#[test]
fn schemars_derives_for_entry_v1_types() {
    let generated = schemars::schema_for!(RuleEntry).to_value();
    assert_eq!(generated["title"], "RuleEntry");
}

#[test]
fn sample_entries_deserialize_and_obey_shared_validation() {
    let go: RuleEntry = serde_yaml_ng::from_str(include_str!(
        "../../../examples/sample-playbook/rules/go-no-logging.yaml"
    ))
    .unwrap();
    go.validate().unwrap();
    assert_eq!(go.severity, RuleSeverity::Block);
    assert_eq!(go.track, RuleTrack::Deterministic);
    let semantic: RuleEntry = serde_yaml_ng::from_str(include_str!(
        "../../../examples/sample-playbook/rules/cross-service-notification.yaml"
    ))
    .unwrap();
    semantic.validate().unwrap();
    assert_eq!(semantic.track, RuleTrack::Semantic);
}

#[test]
fn json_and_yaml_enum_encodings_match_entry_contract() {
    let rule: RuleEntry = serde_yaml_ng::from_str(include_str!(
        "../../../examples/sample-playbook/rules/go-no-logging.yaml"
    ))
    .unwrap();
    let value = serde_json::to_value(rule).unwrap();
    assert_eq!(value["severity"], "BLOCK");
    assert_eq!(value["track"], "deterministic");
    assert_eq!(value["fixtures"][0]["polarity"], "positive");
}

#[test]
fn unsupported_enum_values_are_rejected() {
    assert!(serde_yaml_ng::from_str::<RuleTrack>("probabilistic").is_err());
    assert!(serde_yaml_ng::from_str::<RuleSeverity>("CRITICAL").is_err());
}

#[test]
fn ledger_types_encode_the_openapi_metric_and_counter_values() {
    let openapi: Value = serde_yaml_ng::from_str(include_str!(
        "../../../specs/002-sker-release-preview/contracts/ledger-openapi.yaml"
    ))
    .unwrap();
    let metric_keys = openapi["components"]["schemas"]["MetricValue"]["properties"]["key"]["enum"]
        .as_array()
        .unwrap();
    assert_eq!(metric_keys.len(), 7);
    assert_eq!(
        serde_json::to_value(sker_schema::MetricKey::ConsultCitationRate).unwrap(),
        "consult_citation_rate"
    );
    assert_eq!(
        serde_json::to_value(sker_schema::AgeBucket::Days0To30).unwrap(),
        "0_30_days"
    );
    let counters = sker_schema::EventCounters {
        consult_count: Some(2),
        match_count: None,
        changed_line_count: Some(3),
        duration_ms: Some(1.5),
    };
    let encoded = serde_json::to_value(counters).unwrap();
    assert_eq!(encoded["consult_count"], 2);
    assert!(encoded.get("match_count").is_none());
    assert_eq!(encoded["duration_ms"], 1.5);
}

#[test]
fn cli_ci_contract_matches_root_check_report_encoding_and_exit_codes() {
    let contract: Value = serde_json::from_str(include_str!(
        "../../../specs/002-sker-release-preview/contracts/cli-ci.json"
    ))
    .unwrap();
    assert_eq!(
        contract["command"]["argv"],
        serde_json::json!([
            "sker", "check", "--ci", "--sarif", "<path>", "--json", "<path>"
        ])
    );
    for code in ["0", "1", "2"] {
        assert!(contract["command"]["exit_codes"][code].is_string());
    }
    let report_schema = &contract["check_report_schema"];
    let report_required = report_schema["required"].as_array().unwrap();
    for field in [
        "schema_version",
        "status",
        "bundle_sha256",
        "diff_tree_hash",
        "changed_line_count",
        "findings",
        "analysis",
    ] {
        assert!(report_required.iter().any(|item| item == field));
        assert!(report_schema["properties"].get(field).is_some());
    }
    let finding_required = report_schema["properties"]["findings"]["items"]["required"]
        .as_array()
        .unwrap();
    assert!(finding_required.iter().any(|item| item == "alternatives"));
}

use sker_mcp::{
    attest,
    hook::{PreToolUseInput, handle},
};
use sker_schema::{
    ConsultResultStatus, ConsultTraceEntry, CredentialIssuer, SemanticVerdict,
    SemanticVerdictState, canonical_json_bytes,
};

#[test]
fn attest_issues_contract_shaped_blake3_credential() {
    let credential = attest::issue(
        "a".repeat(64),
        "b".repeat(64),
        vec![ConsultTraceEntry {
            consult_id: "c1".into(),
            occurred_at: "2026-11-18T00:00:00Z".into(),
            query_digest: "c".repeat(64),
            paths: vec!["src/lib.rs".into()],
            matched_rule_ids: vec![],
            result_status: ConsultResultStatus::NoMatch,
        }],
        vec![SemanticVerdict {
            rule_id: "semantic.notice".into(),
            state: SemanticVerdictState::Comply,
            declaration: "Reviewed and complied".into(),
            rationale_digest: None,
            evidence_path: None,
            evidence_line: None,
        }],
        CredentialIssuer::Mcp,
        "2026-11-18T00:00:00Z".into(),
    )
    .unwrap();
    assert_eq!(credential.schema_version, "sker.agent-credential/v1");
    assert_eq!(credential.credential_blake3.len(), 64);
    assert!(
        credential
            .credential_blake3
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    let mut payload = serde_json::to_value(&credential).unwrap();
    let fields = payload.as_object_mut().unwrap();
    fields.remove("credential_blake3");
    fields.get_mut("semantic_verdicts").unwrap()[0]
        .as_object_mut()
        .unwrap()
        .remove("rationale_digest");
    assert_eq!(
        credential.credential_blake3,
        blake3::hash(&canonical_json_bytes(&payload).unwrap())
            .to_hex()
            .to_string()
    );
}

#[test]
fn pre_tool_use_adapter_preserves_event_and_emits_fast_context_shape() {
    let input = PreToolUseInput {
        hook_event_name: Some("PreToolUse".into()),
        tool_name: Some("Edit".into()),
        tool_input: Some(serde_json::json!({"file_path":"src/security/token.rs"})),
    };
    let output = handle(&input);
    let value = serde_json::to_value(output).unwrap();
    assert_eq!(value["hookSpecificOutput"]["hookEventName"], "PreToolUse");
    assert!(
        value["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains("High-risk")
    );
}

#[test]
fn rmcp_router_exposes_the_four_contract_tool_names() {
    let contract: serde_json::Value = serde_json::from_str(include_str!(
        "../../../specs/002-sker-release-preview/contracts/mcp-tools.json"
    ))
    .unwrap();
    let expected: Vec<_> = contract["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    let registered = sker_mcp::tools::SkerServer::tool_definitions();
    let mut actual: Vec<_> = registered.iter().map(|tool| tool.name.as_ref()).collect();
    let mut expected = expected;
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
    for tool in &registered {
        let declared = contract["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["name"] == tool.name.as_ref())
            .unwrap();
        assert_eq!(
            serde_json::to_value(tool.annotations.as_ref().unwrap()).unwrap(),
            declared["annotations"]
        );
        let actual_schema = serde_json::to_value(&tool.input_schema).unwrap();
        assert_eq!(actual_schema, declared["inputSchema"]);
        let output_schema = serde_json::to_value(tool.output_schema.as_ref().unwrap()).unwrap();
        assert_eq!(output_schema, declared["outputSchema"]);
        let mut actual_required: Vec<_> = actual_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let mut declared_required: Vec<_> = declared["inputSchema"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        actual_required.sort();
        declared_required.sort();
        assert_eq!(
            actual_required, declared_required,
            "{} required fields",
            tool.name
        );
    }
}

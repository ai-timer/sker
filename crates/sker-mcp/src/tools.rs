use rmcp::model::JsonObject;
use rmcp::{
    ErrorData, ServerHandler,
    handler::server::{
        tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    model::ServerConfig,
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use sker_retriever::TantivyRetriever;
use sker_schema::{
    ConsultQuery, ConsultResultStatus, ConsultTraceEntry, CredentialIssuer, RuleBundle, RuleEntry,
    SemanticVerdict,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConsultArgs {
    #[schemars(length(min = 1, max = 4000))]
    pub intent: String,
    #[schemars(length(min = 1))]
    pub paths: Vec<String>,
    #[schemars(range(min = 1, max = 50))]
    pub limit: Option<usize>,
    #[schemars(regex(pattern = "^[0-9a-f]{64}$"))]
    pub bundle_sha256: Option<String>,
}
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchArgs {
    #[schemars(length(min = 1, max = 4000))]
    pub query: String,
    pub paths: Option<Vec<String>>,
    #[schemars(length(min = 1, max = 128))]
    pub domain: Option<String>,
    #[schemars(range(min = 1, max = 100))]
    pub limit: Option<usize>,
    #[schemars(regex(pattern = "^[0-9a-f]{64}$"))]
    pub bundle_sha256: Option<String>,
}
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AttestArgs {
    pub bundle_sha256: String,
    pub diff_tree_hash: String,
    pub consult_trace: Vec<ConsultTraceEntry>,
    pub semantic_verdicts: Vec<SemanticVerdict>,
    pub issuer: CredentialIssuer,
    pub created_at: String,
}
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProposeArgs {
    pub entry: RuleEntry,
    pub rationale: String,
    pub based_on_bundle_sha256: Option<String>,
}

/// Immutable bundle plus a request-local retriever; no session state or vector model is retained.
pub struct SkerServer {
    bundle: Arc<RuleBundle>,
    proposals: PathBuf,
    proposal_lock: Mutex<()>,
    tool_router: ToolRouter<Self>,
}
impl SkerServer {
    pub fn new(bundle: RuleBundle, proposals: impl Into<PathBuf>) -> Self {
        Self {
            bundle: Arc::new(bundle),
            proposals: proposals.into(),
            proposal_lock: Mutex::new(()),
            tool_router: Self::tool_router(),
        }
    }
    pub fn bundle_sha256(&self) -> &str {
        &self.bundle.bundle_sha256
    }
    fn check_bundle(&self, requested: Option<String>) -> Result<(), ErrorData> {
        if let Some(actual) = requested
            && actual != self.bundle.bundle_sha256
        {
            return Err(ErrorData::invalid_params(
                "bundle_sha256 does not match active bundle",
                None,
            ));
        }
        Ok(())
    }
    fn output(value: Value) -> Json<Value> {
        Json(value)
    }
}

fn contract_schema(tool_name: &str, schema_kind: &str) -> Arc<JsonObject> {
    let contract: Value = serde_json::from_str(include_str!(
        "../../../specs/002-sker-release-preview/contracts/mcp-tools.json"
    ))
    .expect("checked-in MCP contract is valid JSON");
    let schema = contract["tools"]
        .as_array()
        .and_then(|tools| tools.iter().find(|tool| tool["name"] == tool_name))
        .and_then(|tool| tool.get(schema_kind))
        .and_then(Value::as_object)
        .expect("contract defines tool schema")
        .clone();
    Arc::new(schema)
}

impl SkerServer {
    /// Return the registered MCP definitions for host-side contract inspection.
    pub fn tool_definitions() -> Vec<rmcp::model::Tool> {
        Self::tool_router().list_all()
    }
}

#[tool_router]
impl SkerServer {
    #[tool(name = "sker_consult", description = "Return only locally relevant rules for intent and target paths; low relevance is an explicit no_match.", input_schema = crate::tools::contract_schema("sker_consult", "inputSchema"), output_schema = crate::tools::contract_schema("sker_consult", "outputSchema"), annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false))]
    fn consult(&self, Parameters(args): Parameters<ConsultArgs>) -> Result<Json<Value>, ErrorData> {
        self.check_bundle(args.bundle_sha256)?;
        let result = sker_schema::IntentRetriever::consult(
            &TantivyRetriever::default(),
            &self.bundle,
            &ConsultQuery {
                intent: args.intent,
                paths: args.paths,
                limit: args.limit,
                minimum_score: None,
            },
        )
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Self::output(
            json!({"status": if result.status == ConsultResultStatus::Matched {"matched"} else {"no_match"}, "bundle_sha256": result.bundle_sha256, "matches": result.matches}),
        ))
    }
    #[tool(name = "sker_search", description = "Search rule metadata locally and return locatable results; an empty result is distinct from a tool error.", input_schema = crate::tools::contract_schema("sker_search", "inputSchema"), output_schema = crate::tools::contract_schema("sker_search", "outputSchema"), annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false))]
    fn search(&self, Parameters(args): Parameters<SearchArgs>) -> Result<Json<Value>, ErrorData> {
        self.check_bundle(args.bundle_sha256)?;
        let results = TantivyRetriever::default()
            .ranked_search(
                &self.bundle,
                &args.query,
                &args.paths.unwrap_or_default(),
                &args.domain.into_iter().collect::<Vec<_>>(),
                args.limit.unwrap_or(20),
            )
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Self::output(
            json!({"status": if results.is_empty() {"no_match"} else {"matched"}, "bundle_sha256": self.bundle.bundle_sha256, "results": results}),
        ))
    }
    #[tool(name = "sker_attest", description = "Locally finalize a credential before commit/PR submission.", input_schema = crate::tools::contract_schema("sker_attest", "inputSchema"), output_schema = crate::tools::contract_schema("sker_attest", "outputSchema"), annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false))]
    fn attest(&self, Parameters(args): Parameters<AttestArgs>) -> Result<Json<Value>, ErrorData> {
        if args.bundle_sha256 != self.bundle.bundle_sha256 {
            return Err(ErrorData::invalid_params(
                "bundle_sha256 does not match active bundle",
                None,
            ));
        }
        let credential = crate::attest::issue(
            args.bundle_sha256,
            args.diff_tree_hash,
            args.consult_trace,
            args.semantic_verdicts,
            args.issuer,
            args.created_at,
        )
        .map_err(|e| ErrorData::internal_error(e, None))?;
        let mut credential = serde_json::to_value(credential)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        if let Some(verdicts) = credential
            .get_mut("semantic_verdicts")
            .and_then(Value::as_array_mut)
        {
            for verdict in verdicts {
                if let Some(fields) = verdict.as_object_mut()
                    && fields.get("rationale_digest").is_some_and(Value::is_null)
                {
                    fields.remove("rationale_digest");
                }
            }
        }
        Ok(Self::output(
            json!({"status":"created", "credential":credential}),
        ))
    }
    #[tool(name = "sker_propose", description = "Create a pending rule proposal for human review; it does not update the active bundle.", input_schema = crate::tools::contract_schema("sker_propose", "inputSchema"), output_schema = crate::tools::contract_schema("sker_propose", "outputSchema"), annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false))]
    fn propose(&self, Parameters(args): Parameters<ProposeArgs>) -> Result<Json<Value>, ErrorData> {
        args.entry
            .validate()
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
        if args.rationale.trim().is_empty() || args.rationale.len() > 8000 {
            return Err(ErrorData::invalid_params(
                "rationale must contain 1..=8000 bytes",
                None,
            ));
        }
        if args
            .based_on_bundle_sha256
            .as_deref()
            .is_some_and(|v| v != self.bundle.bundle_sha256)
        {
            return Err(ErrorData::invalid_params(
                "based_on_bundle_sha256 does not match active bundle",
                None,
            ));
        }
        let proposal_id = blake3::hash(
            format!(
                "{}:{}:{}",
                args.entry.rule_id, args.rationale, self.bundle.bundle_sha256
            )
            .as_bytes(),
        )
        .to_hex()
        .to_string();
        let proposal = sker_schema::Proposal {
            proposal_id: proposal_id.clone(),
            state: sker_schema::ProposalState::Pending,
            entry: args.entry,
            rationale: args.rationale,
            based_on_bundle_sha256: Some(self.bundle.bundle_sha256.clone()),
            created_at: crate::attest::now_rfc3339(),
        };
        let _guard = self
            .proposal_lock
            .lock()
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        std::fs::create_dir_all(&self.proposals)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let path = self.proposals.join(format!("{proposal_id}.json"));
        let bytes = serde_json::to_vec_pretty(&proposal)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        std::fs::write(path, bytes).map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(Self::output(
            json!({"status":"accepted", "proposal_id":proposal_id, "state":"pending"}),
        ))
    }
}

#[tool_handler]
impl ServerHandler for SkerServer {
    fn get_info(&self) -> ServerConfig {
        let _registered_tools = &self.tool_router;
        ServerConfig::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sker_schema::{BundleMetadata, RuleSourceKind, SourceLock, compute_bundle_sha256};

    fn sample_bundle() -> RuleBundle {
        let rule: RuleEntry = serde_yaml_ng::from_str(include_str!(
            "../../../examples/sample-playbook/rules/go-no-logging.yaml"
        ))
        .unwrap();
        let lock = SourceLock {
            kind: RuleSourceKind::RepositoryLocal,
            source_id: "test".into(),
            version: "1".into(),
            content_hash: "a".repeat(64),
        };
        let hash = compute_bundle_sha256(std::slice::from_ref(&lock), std::slice::from_ref(&rule))
            .unwrap();
        RuleBundle {
            schema_version: "sker.bundle/v1".into(),
            bundle_id: hash.clone(),
            bundle_sha256: hash,
            sources: vec![lock],
            rules: vec![rule],
            metadata: BundleMetadata {
                created_at: "2026-11-18T00:00:00Z".into(),
                compiler_version: "test".into(),
            },
        }
    }

    #[test]
    fn propose_only_persists_pending_candidate_without_changing_bundle() {
        let bundle = sample_bundle();
        let original_hash = bundle.bundle_sha256.clone();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("sker-mcp-proposals-{}-{nonce}", std::process::id()));
        let server = SkerServer::new(bundle, &directory);
        let entry = server.bundle.rules[0].clone();
        let output = server
            .propose(Parameters(ProposeArgs {
                entry,
                rationale: "Human review required".into(),
                based_on_bundle_sha256: Some(original_hash.clone()),
            }))
            .unwrap()
            .0;
        assert_eq!(output["status"], "accepted");
        assert_eq!(output["state"], "pending");
        let proposal_path =
            directory.join(format!("{}.json", output["proposal_id"].as_str().unwrap()));
        let proposal: Value =
            serde_json::from_slice(&std::fs::read(proposal_path).unwrap()).unwrap();
        assert_eq!(proposal["state"], "pending");
        assert_eq!(server.bundle.bundle_sha256, original_hash);
        let _ = std::fs::remove_dir_all(directory);
    }
}

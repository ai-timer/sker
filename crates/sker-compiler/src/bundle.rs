use crate::merge::merge;
use sker_schema::{
    BundleMetadata, CompileError, CompileRequest, RuleBundle, compute_bundle_sha256,
};

/// Stateless compiler implementing the schema crate's canonical boundary trait.
#[derive(Debug, Default, Clone, Copy)]
pub struct Compiler;
impl sker_schema::BundleCompiler for Compiler {
    fn compile(&self, request: &CompileRequest) -> Result<RuleBundle, CompileError> {
        compile(request)
    }
}
pub fn compile(request: &CompileRequest) -> Result<RuleBundle, CompileError> {
    let (sources, rules) = merge(&request.sources, true)?;
    let hash = compute_bundle_sha256(&sources, &rules)?;
    let bundle = RuleBundle {
        schema_version: "sker.bundle/v1".into(),
        bundle_id: hash.clone(),
        bundle_sha256: hash,
        sources,
        rules,
        metadata: BundleMetadata {
            created_at: "1970-01-01T00:00:00Z".into(),
            compiler_version: request.compiler_version.clone(),
        },
    };
    bundle.validate()?;
    Ok(bundle)
}

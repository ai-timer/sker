//! Stateless local MCP server and host hook adapter.
pub mod attest;
pub mod hook;
pub mod tools;

pub use tools::SkerServer;
pub use attest::issue as issue_attestation;
pub use hook::{handle as handle_hook, write_response as write_hook_response, PreToolUseInput, HookOutput};

/// Run the local, process-scoped stateless stdio MCP endpoint.
pub async fn serve_stdio(server: SkerServer) -> anyhow::Result<()> {
    use rmcp::ServiceExt;
    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}

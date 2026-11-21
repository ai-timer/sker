//! Stateless local MCP server and host hook adapter.
pub mod attest;
pub mod hook;
pub mod tools;

pub use attest::issue as issue_attestation;
pub use hook::{
    HookOutput, PreToolUseInput, handle as handle_hook, write_response as write_hook_response,
};
pub use tools::SkerServer;

/// Run the local, process-scoped stateless stdio MCP endpoint.
pub async fn serve_stdio(server: SkerServer) -> anyhow::Result<()> {
    use rmcp::ServiceExt;
    use std::io::IsTerminal;

    eprintln!(
        "sker-mcp ready on stdio (bundle: {})",
        server.bundle_sha256()
    );
    if std::io::stdin().is_terminal() {
        eprintln!("Note: Sker MCP runs over stdio and is normally spawned by an AI coding host.");
        eprintln!(
            "Interactive terminal detected. Waiting for JSON-RPC messages... Press Ctrl+C to exit."
        );
    }

    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}

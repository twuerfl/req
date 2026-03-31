// REQ: LLR-0024
// REQ: LLR-0025
//! req_mcp — MCP server for AI model integration.
//!
//! Exposes `req_engine` operations as Model Context Protocol (MCP) tools and
//! resources over stdio transport, enabling AI models (Claude, Copilot, etc.)
//! to call req operations natively.
//!
//! ## MCP Tools
//!
//! - `req_scan`     — scan source directories for requirement tags
//! - `req_coverage` — get coverage statistics
//! - `req_gaps`     — find traceability gaps
//! - `req_check`    — validate all links
//! - `req_trace`    — get trace tree for a requirement
//! - `req_impact`   — impact analysis for a requirement
//! - `req_list`     — list requirements (filterable by type/status)
//! - `req_export`   — export requirements as JSON or ai-context
//!
//! ## MCP Resources
//!
//! - `req://requirements/{type}/{id}` — single requirement
//! - `req://requirements/{type}`      — all requirements of a type
//! - `req://coverage`                 — live coverage report
//! - `req://gaps`                     — live gap report

use req_engine::ReqEngine;
use req_mcp::server::ReqServer;
use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // REQ_BASE lets the MCP client specify the project root explicitly,
    // which is necessary when the client (e.g. VS Code extension) does not
    // support the `cwd` field in .mcp.json.
    let base = match std::env::var("REQ_BASE") {
        Ok(p) => std::path::PathBuf::from(p),
        Err(_) => std::env::current_dir()?,
    };
    let engine = ReqEngine::open(&base).map_err(|e| {
        anyhow::anyhow!(
            "Failed to open req project in {}: {}. Run `req init` first.",
            base.display(),
            e
        )
    })?;

    let server = ReqServer::new(engine);
    let service = server
        .serve((tokio::io::stdin(), tokio::io::stdout()))
        .await?;
    service.waiting().await?;
    Ok(())
}

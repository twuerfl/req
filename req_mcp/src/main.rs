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
//! ### Requirements lifecycle
//! - `req_init`                — initialise a new project (no-op if already done)
//! - `req_create_requirement`  — create an HLR/LLR/TST on disk and in cache
//! - `req_remove`              — remove a requirement from disk and/or cache
//! - `req_import`              — import requirements from Markdown/JSON
//! - `req_import_ai`           — import AI-generated suggestions (forced draft)
//! - `req_migrate`             — upgrade requirement files to current schema
//!
//! ### Query & analysis
//! - `req_scan`                — scan source directories for requirement tags
//! - `req_coverage`            — get coverage statistics
//! - `req_gaps`                — find traceability gaps (incl. import staleness)
//! - `req_check`               — validate all links
//! - `req_trace`               — get trace tree for a requirement
//! - `req_impact`              — impact analysis for a requirement
//! - `req_list`                — list requirements (filterable by type/status)
//! - `req_export`              — export requirements as JSON/ai-context/markdown
//!
//! ### AI output integrity audits
//! - `req_audit_triviality`     — static hollow-body detection
//! - `req_audit_criteria`       — acceptance-criterion linkage report
//! - `req_audit_mutation`       — correlate a cargo-mutants JSON report
//! - `req_audit_coverage`       — correlate a cargo llvm-cov JSON report
//! - `req_audit_export_context` — full LLM-reviewable audit bundle
//! - `req_audit_independence`   — impl/test author independence check
//! - `req_check_provenance`     — verify all requirements have valid provenance
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

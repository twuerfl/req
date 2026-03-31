// REQ: LLR-0024
//! Error conversion helpers for the MCP server.

use rmcp::model::ErrorData;

/// Convert a `req_engine` error into an MCP `ErrorData` (internal error).
pub fn engine_err(e: req_engine::Error) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}

//! MCP server handler for `req` engine operations.
//!
//! Exposes all `ReqEngine` operations as Model Context Protocol tools and
//! resources using the `rmcp` v1.3 SDK.

use crate::error::engine_err;
use req_engine::{RequirementType, ReqEngine};
use rmcp::{
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{
        Annotated, Implementation, ListResourceTemplatesResult, ListResourcesResult,
        PaginatedRequestParams, RawResource, RawResourceTemplate, ReadResourceRequestParams,
        ReadResourceResult, ResourceContents, ServerCapabilities, ServerInfo,
    },
    schemars::{self, JsonSchema},
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use rmcp::model::{CallToolResult, Content, ErrorData};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

// ── Input parameter structs ────────────────────────────────────────────────

/// Input for `req_scan`
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ScanInput {
    /// Clear the cache before scanning (default: false)
    #[serde(default)]
    pub clear: bool,
}

/// Input for `req_trace` and `req_impact`
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct IdInput {
    /// Requirement ID to operate on (e.g. `"LLR-0001"`)
    pub id: String,
}

/// Input for `req_list`
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListInput {
    /// Filter by requirement type: `"hlr"`, `"llr"`, or `"tst"`
    pub r#type: Option<String>,
    /// Filter by status: `"draft"`, `"approved"`, `"deprecated"`, `"rejected"`
    pub status: Option<String>,
}

/// Input for `req_export`
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ExportInput {
    /// Export format: `"json"` (default) or `"ai-context"`
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "json".to_string()
}

/// Input for `req_audit_coverage`
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AuditCoverageInput {
    /// Path to the `cargo llvm-cov --json` report file
    pub report: String,
}

// ── Server struct ──────────────────────────────────────────────────────────

/// MCP server exposing `req` engine operations as tools and resources.
#[derive(Clone)]
pub struct ReqServer {
    engine: Arc<Mutex<ReqEngine>>,
    tool_router: ToolRouter<Self>,
}

impl ReqServer {
    /// Construct a new server wrapping the given engine.
    pub fn new(engine: ReqEngine) -> Self {
        Self {
            engine: Arc::new(Mutex::new(engine)),
            tool_router: Self::tool_router(),
        }
    }

    /// Lock the engine and run a closure, converting errors to `ErrorData`.
    fn with_engine<T, F>(&self, f: F) -> Result<T, ErrorData>
    where
        F: FnOnce(&ReqEngine) -> req_engine::Result<T>,
    {
        let guard = self
            .engine
            .lock()
            .map_err(|_| ErrorData::internal_error("engine mutex poisoned", None))?;
        f(&*guard).map_err(engine_err)
    }

    /// Serialize a value to pretty JSON, mapping errors to ErrorData.
    fn to_json<T: Serialize>(value: &T) -> Result<String, ErrorData> {
        serde_json::to_string_pretty(value)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))
    }
}

// ── Tool implementations ───────────────────────────────────────────────────

// REQ: LLR-0025
#[tool_router]
impl ReqServer {
    /// Scan source directories for REQ tags and update the traceability cache.
    #[tool(description = "Scan source directories for REQ tags and update the traceability cache")]
    pub fn req_scan(&self, Parameters(input): Parameters<ScanInput>) -> Result<CallToolResult, ErrorData> {
        let result = self.with_engine(|e| e.scan(None, input.clear))?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&result)?)]))
    }

    /// Get requirement coverage statistics.
    #[tool(description = "Get coverage statistics: HLR→LLR coverage, LLR implementation and test coverage percentages")]
    pub fn req_coverage(&self) -> Result<CallToolResult, ErrorData> {
        let result = self.with_engine(|e| e.coverage())?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&result)?)]))
    }

    /// Find all traceability gaps.
    #[tool(description = "Find all traceability gaps: HLRs without LLRs, LLRs without code refs, undefined IDs, etc.")]
    pub fn req_gaps(&self) -> Result<CallToolResult, ErrorData> {
        let result = self.with_engine(|e| e.gaps())?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&result)?)]))
    }

    /// Validate all traceability links and return any issues.
    #[tool(description = "Validate all requirement links and return a list of errors and warnings")]
    pub fn req_check(&self) -> Result<CallToolResult, ErrorData> {
        let result = self.with_engine(|e| e.validate())?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&result)?)]))
    }

    /// Get the full trace tree for a single requirement.
    #[tool(description = "Get the full trace tree (parents, children, code refs, tests) for a requirement ID")]
    pub fn req_trace(&self, Parameters(input): Parameters<IdInput>) -> Result<CallToolResult, ErrorData> {
        let result = self.with_engine(|e| e.trace_tree(&input.id))?;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Run impact analysis for a requirement.
    #[tool(description = "Impact analysis: given a requirement ID, return all affected requirements, files, and tests")]
    pub fn req_impact(&self, Parameters(input): Parameters<IdInput>) -> Result<CallToolResult, ErrorData> {
        let result = self.with_engine(|e| e.impact(&input.id))?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&result)?)]))
    }

    /// List requirements, optionally filtered by type and/or status.
    #[tool(description = "List requirements filtered by type (hlr/llr/tst) and/or status (draft/approved/deprecated/rejected)")]
    pub fn req_list(&self, Parameters(input): Parameters<ListInput>) -> Result<CallToolResult, ErrorData> {
        let type_filter = input.r#type.as_deref().and_then(RequirementType::from_str);
        let mut reqs = self.with_engine(|e| e.list_requirements(type_filter))?;
        if let Some(status_str) = &input.status {
            let s = status_str.to_lowercase();
            reqs.retain(|r| r.status.as_str() == s);
        }
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&reqs)?)]))
    }

    /// Export requirements in the requested format.
    #[tool(description = "Export all requirements as JSON or ai-context format for LLM consumption")]
    pub fn req_export(&self, Parameters(input): Parameters<ExportInput>) -> Result<CallToolResult, ErrorData> {
        let result = self.with_engine(|e| e.export(&input.format, None))?;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    // REQ: LLR-0025
    /// Parse a `cargo llvm-cov --json` report and return per-LLR line-hit scores.
    #[tool(description = "Parse a cargo llvm-cov --json report and return per-LLR/TST line-hit percentages. Pass the path to the JSON file produced by `cargo llvm-cov --json --output-path <file>`.")]
    pub fn req_audit_coverage(&self, Parameters(input): Parameters<AuditCoverageInput>) -> Result<CallToolResult, ErrorData> {
        let report_path = std::path::Path::new(&input.report);
        let scores = self.with_engine(|e| e.audit_coverage(report_path))?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&scores)?)]))
    }
}

// ── ServerHandler + resource dispatch ─────────────────────────────────────

#[tool_handler]
impl ServerHandler for ReqServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new(
            "req-mcp-server",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(
            "req MCP server: query requirements, coverage, gaps, and traceability. \
             The project must be initialized with `req init` in the working directory.",
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let resources = vec![
            Annotated::new(RawResource::new("req://coverage", "coverage"), None),
            Annotated::new(RawResource::new("req://gaps", "gaps"), None),
            Annotated::new(RawResource::new("req://requirements/hlr", "requirements/hlr"), None),
            Annotated::new(RawResource::new("req://requirements/llr", "requirements/llr"), None),
            Annotated::new(RawResource::new("req://requirements/tst", "requirements/tst"), None),
        ];
        Ok(ListResourcesResult {
            resources,
            meta: None,
            next_cursor: None,
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        let templates = vec![Annotated::new(
            RawResourceTemplate::new(
                "req://requirements/{type}/{id}",
                "requirements/{type}/{id}",
            ),
            None,
        )];
        Ok(ListResourceTemplatesResult {
            resource_templates: templates,
            meta: None,
            next_cursor: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        let uri = request.uri.as_str();
        let json = self.dispatch_resource_pub(uri)?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(json, uri).with_mime_type("application/json"),
        ]))
    }
}

impl ReqServer {
    /// Dispatch a resource URI to the appropriate engine call, returning JSON.
    pub fn dispatch_resource_pub(&self, uri: &str) -> Result<String, ErrorData> {
        match uri {
            "req://coverage" => {
                let cov = self.with_engine(|e| e.coverage())?;
                Self::to_json(&cov)
            }
            "req://gaps" => {
                let gaps = self.with_engine(|e| e.gaps())?;
                Self::to_json(&gaps)
            }
            _ if uri.starts_with("req://requirements/") => {
                let rest = &uri["req://requirements/".len()..];
                let parts: Vec<&str> = rest.splitn(2, '/').collect();
                let type_filter =
                    RequirementType::from_str(parts[0]).ok_or_else(|| {
                        ErrorData::invalid_params(
                            format!("unknown requirement type: {}", parts[0]),
                            None,
                        )
                    })?;
                let reqs = self.with_engine(|e| e.list_requirements(Some(type_filter)))?;
                if parts.len() == 2 {
                    let id = parts[1];
                    let req = reqs.into_iter().find(|r| r.id == id).ok_or_else(|| {
                        ErrorData::invalid_params(
                            format!("requirement not found: {id}"),
                            None,
                        )
                    })?;
                    Self::to_json(&req)
                } else {
                    Self::to_json(&reqs)
                }
            }
            _ => Err(ErrorData::invalid_params(
                format!("unknown resource URI: {uri}"),
                None,
            )),
        }
    }
}

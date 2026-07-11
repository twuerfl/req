//! MCP server handler for `req` engine operations.
//!
//! Exposes all `ReqEngine` operations as Model Context Protocol tools and
//! resources using the `rmcp` v1.3 SDK.

use crate::error::engine_err;
use req_engine::ai_import::ImportOptions;
use req_engine::provenance;
use req_engine::{RequirementStatus, RequirementType, ReqEngine};
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
use std::path::PathBuf;
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
    /// Export format: `"json"` (default), `"ai-context"`, or `"markdown"`
    #[serde(default = "default_format")]
    pub format: String,
    /// Export a single requirement by ID (optional). If omitted, all are exported.
    #[serde(default)]
    pub id: Option<String>,
}

fn default_format() -> String {
    "json".to_string()
}

/// Input for `req_audit_coverage` and `req_audit_mutation`
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AuditCoverageInput {
    /// Path to the `cargo llvm-cov --json` report file
    pub report: String,
}

/// Input for `req_create_requirement`
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CreateRequirementInput {
    /// Requirement type: `"hlr"`, `"llr"`, or `"tst"`
    pub r#type: String,
    /// Requirement title
    pub title: String,
    /// Parent requirement ID (required for LLR; optional otherwise)
    #[serde(default)]
    pub parent: Option<String>,
    /// Status: `"draft"` (default), `"approved"`, `"deprecated"`, `"rejected"`
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String {
    "draft".to_string()
}

/// Input for `req_remove`
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RemoveInput {
    /// Requirement ID to remove (e.g. `"LLR-0042"`)
    pub id: String,
    /// Purge cache entries only — do not delete the `.md` file
    #[serde(default)]
    pub cache_only: bool,
    /// Skip dependency check (remove even if children or code refs exist)
    #[serde(default)]
    pub force: bool,
}

/// Input for `req_import`
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ImportInput {
    /// Path to the file to import
    pub input: String,
    /// Import format: `"markdown"` (default) or `"json"`
    #[serde(default = "default_import_format")]
    pub format: String,
    /// Origin identifier for audit trail (e.g. `"DOORS-project-X"`)
    #[serde(default)]
    pub provenance: Option<String>,
}

fn default_import_format() -> String {
    "markdown".to_string()
}

/// Input for `req_import_ai`
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ImportAiInput {
    /// Path to the AI suggestions JSON file
    pub input: String,
    /// Preview without writing to disk/cache (dry run)
    #[serde(default)]
    pub dry_run: bool,
    /// Origin identifier stored as provenance attribute
    #[serde(default)]
    pub provenance: Option<String>,
}

/// Input for `req_migrate`
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct MigrateInput {
    /// Preview changes without writing files
    #[serde(default)]
    pub dry_run: bool,
}

/// Input for `req_audit_triviality` and `req_audit_independence`
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct OptionalIdInput {
    /// Restrict to a single LLR ID (optional; default: all)
    #[serde(default)]
    pub id: Option<String>,
}

/// Input for `req_audit_mutation`
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AuditMutationInput {
    /// Path to the `cargo mutants --json` report file
    pub report: String,
}

/// Input for `req_audit_export_context`
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AuditExportContextInput {
    /// LLR ID to bundle
    pub id: String,
    /// Path to a `cargo mutants --json` report (optional)
    #[serde(default)]
    pub mutation: Option<String>,
    /// Path to a `cargo llvm-cov --json` report (optional)
    #[serde(default)]
    pub coverage: Option<String>,
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

    /// Find all traceability gaps (including import staleness).
    #[tool(description = "Find all traceability gaps: HLRs without LLRs, LLRs without code refs, undefined IDs, import staleness, etc.")]
    pub fn req_gaps(&self) -> Result<CallToolResult, ErrorData> {
        let result = self.with_engine(|e| e.gaps_full())?;
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
    #[tool(description = "Export requirements as JSON, ai-context, or markdown. Optional `id` exports a single requirement.")]
    pub fn req_export(&self, Parameters(input): Parameters<ExportInput>) -> Result<CallToolResult, ErrorData> {
        let result = self.with_engine(|e| e.export(&input.format, input.id.as_deref()))?;
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

    /// Create a new requirement (HLR/LLR/TST) on disk and in the cache.
    #[tool(description = "Create a new requirement. Requires type (hlr/llr/tst), title, optional parent ID, and status (default draft). Returns the created Requirement as JSON.")]
    pub fn req_create_requirement(&self, Parameters(input): Parameters<CreateRequirementInput>) -> Result<CallToolResult, ErrorData> {
        let req_type = RequirementType::from_str(&input.r#type).ok_or_else(|| {
            ErrorData::invalid_params(
                format!("unknown requirement type: {}", input.r#type),
                None,
            )
        })?;
        let status = RequirementStatus::from_str(&input.status).ok_or_else(|| {
            ErrorData::invalid_params(
                format!("unknown status: {}", input.status),
                None,
            )
        })?;
        let req = self.with_engine(|e| {
            e.create_requirement(req_type, &input.title, input.parent.as_deref(), status)
        })?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&req)?)]))
    }

    /// Remove a requirement from disk and/or the cache.
    #[tool(description = "Remove a requirement by ID. `cache_only` purges cache entries without deleting the .md file. `force` skips the dependent-check. Returns the RemoveResult as JSON.")]
    pub fn req_remove(&self, Parameters(input): Parameters<RemoveInput>) -> Result<CallToolResult, ErrorData> {
        let result = self.with_engine(|e| {
            e.remove_requirement(&input.id, input.cache_only, input.force)
        })?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&result)?)]))
    }

    /// Import requirements from a Markdown or JSON file.
    #[tool(description = "Import requirements from a file. `format` is markdown (default) or json. Optional `provenance` records the origin for audit trails. Returns the imported requirements as JSON.")]
    pub fn req_import(&self, Parameters(input): Parameters<ImportInput>) -> Result<CallToolResult, ErrorData> {
        let path = PathBuf::from(&input.input);
        let reqs = self.with_engine(|e| {
            e.import(&path, &input.format, input.provenance.as_deref())
        })?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&reqs)?)]))
    }

    /// Import AI-generated requirement suggestions (forced to draft status).
    #[tool(description = "Import AI-generated requirement suggestions from a JSON file. All imported requirements are forced to draft status. `dry_run` validates without writing. Returns the ImportResult as JSON.")]
    pub fn req_import_ai(&self, Parameters(input): Parameters<ImportAiInput>) -> Result<CallToolResult, ErrorData> {
        let path = PathBuf::from(&input.input);
        let suggestions = req_engine::ai_import::load_suggestions(&path).map_err(engine_err)?;
        let options = ImportOptions {
            dry_run: input.dry_run,
            provenance: input.provenance.clone(),
            source_path: Some(path),
        };
        let result = self.with_engine(|e| e.import_ai(suggestions, options))?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&result)?)]))
    }

    /// Upgrade requirement files to the current schema format.
    #[tool(description = "Re-write all requirement files to the current schema format. `dry_run` previews without writing. Returns the MigrateResult as JSON.")]
    pub fn req_migrate(&self, Parameters(input): Parameters<MigrateInput>) -> Result<CallToolResult, ErrorData> {
        let result = self.with_engine(|e| e.migrate(input.dry_run))?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&result)?)]))
    }

    /// Run static triviality detection across all code refs for one LLR, or all LLRs.
    #[tool(description = "Detect hollow/trivial implementations via static analysis. Optional `id` restricts to a single LLR. Returns an array of TrivialityReport as JSON.")]
    pub fn req_audit_triviality(&self, Parameters(input): Parameters<OptionalIdInput>) -> Result<CallToolResult, ErrorData> {
        let reports = self.with_engine(|e| e.audit_triviality(input.id.as_deref()))?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&reports)?)]))
    }

    /// Build an acceptance-criterion linkage report for a single LLR.
    #[tool(description = "Check acceptance criterion linkage for an LLR. Returns a CriteriaReport with each criterion's linked status and test location as JSON.")]
    pub fn req_audit_criteria(&self, Parameters(input): Parameters<IdInput>) -> Result<CallToolResult, ErrorData> {
        let report = self.with_engine(|e| e.audit_criteria(&input.id))?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&report)?)]))
    }

    /// Parse a `cargo mutants --json` report and return per-LLR mutation scores.
    #[tool(description = "Parse a cargo mutants --json report and return per-LLR mutation scores (caught/missed/score_percent). Pass the path to the JSON file.")]
    pub fn req_audit_mutation(&self, Parameters(input): Parameters<AuditMutationInput>) -> Result<CallToolResult, ErrorData> {
        let report_path = std::path::Path::new(&input.report);
        let report = self.with_engine(|e| e.audit_mutation(report_path))?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&report)?)]))
    }

    /// Export a full audit bundle for one LLR for LLM review.
    #[tool(description = "Build a full AuditBundle for one LLR containing implementation/test spans, triviality findings, optional mutation and coverage scores, and a prompt hint for an external LLM reviewer. Returns the AuditBundle as JSON.")]
    pub fn req_audit_export_context(&self, Parameters(input): Parameters<AuditExportContextInput>) -> Result<CallToolResult, ErrorData> {
        let mutation_path = input.mutation.as_ref().map(PathBuf::from);
        let coverage_path = input.coverage.as_ref().map(PathBuf::from);
        let bundle = self.with_engine(|e| {
            e.audit_export(
                &input.id,
                mutation_path.as_deref(),
                coverage_path.as_deref(),
            )
        })?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&bundle)?)]))
    }

    /// Check author independence between implementation and test spans.
    #[tool(description = "Check author independence between implementation (REQ:) and test (VERIFIES:) spans for one LLR (optional `id`) or all. Requires the git feature for blame data. Returns the IndependenceResult as JSON.")]
    pub fn req_audit_independence(&self, Parameters(input): Parameters<OptionalIdInput>) -> Result<CallToolResult, ErrorData> {
        let result = self.with_engine(|e| e.audit_independence(input.id.as_deref()))?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&result)?)]))
    }

    /// Validate that all requirements have valid provenance.
    #[tool(description = "Check provenance of all requirements — verifies tool_version attributes and detects manual editing. Returns an array of provenance violations as JSON.")]
    pub fn req_check_provenance(&self) -> Result<CallToolResult, ErrorData> {
        let violations = self.with_engine(|e| {
            provenance::check_all_provenance(&e.base, e.cache())
        })?;
        Ok(CallToolResult::success(vec![Content::text(Self::to_json(&violations)?)]))
    }

    /// Initialise a new requirements project structure (no-op if already initialised).
    #[tool(description = "Initialise a new requirements project in the configured base directory. Creates .req/ and requirements/ structure. Returns a status message; no-op if already initialised.")]
    pub fn req_init(&self) -> Result<CallToolResult, ErrorData> {
        let base = {
            let guard = self
                .engine
                .lock()
                .map_err(|_| ErrorData::internal_error("engine mutex poisoned", None))?;
            guard.base.clone()
        };
        if req_engine::config::is_initialized(&base) {
            return Ok(CallToolResult::success(vec![Content::text(
                serde_json::json!({
                    "initialized": true,
                    "base": base.to_string_lossy(),
                    "message": "Project already initialized"
                })
                .to_string(),
            )]));
        }
        ReqEngine::init(&base, None).map_err(engine_err)?;
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::json!({
                "initialized": true,
                "base": base.to_string_lossy(),
                "message": "Project initialized"
            })
            .to_string(),
        )]))
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
            "req MCP server: full requirement lifecycle and traceability. \
             Create, list, remove, import, and export requirements; scan code tags; \
             query coverage, gaps, trace trees, and impact analysis; run AI-output \
             integrity audits (triviality, criteria, mutation, coverage, independence, \
             export-context); check provenance; and migrate schema. \
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
                let gaps = self.with_engine(|e| e.gaps_full())?;
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

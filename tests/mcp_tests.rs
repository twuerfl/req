//! Integration tests for the MCP server implementation.
// REQ: TST-0011
// REQ: LLR-0025
// VERIFIES: LLR-0025

use req_engine::{Requirement, RequirementType, ReqEngine};
use req_mcp::server::{
    AuditCoverageInput, AuditExportContextInput, AuditMutationInput, CreateRequirementInput,
    ExportInput, IdInput, ImportAiInput, ImportInput, ListInput, MigrateInput, OptionalIdInput,
    RemoveInput, ReqServer,
};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, RawContent};
use tempfile::TempDir;

// ── Fixture helpers ────────────────────────────────────────────────────────

/// Initialize a project in a temp dir and seed the cache with one HLR + one LLR.
fn make_engine(dir: &TempDir) -> ReqEngine {
    let base = dir.path();
    ReqEngine::init(base, Some("test")).unwrap();
    let engine = ReqEngine::open(base).unwrap();

    let hlr = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "Top-Level".to_string());
    engine.cache().upsert_requirement(&hlr).unwrap();

    let mut llr = Requirement::new("LLR-0001".to_string(), RequirementType::Llr, "Low-Level".to_string());
    llr.parent = Some("HLR-0001".to_string());
    engine.cache().upsert_requirement(&llr).unwrap();

    engine
}

/// Extract the text of the first content item in a `CallToolResult`.
fn result_text(r: &CallToolResult) -> &str {
    match &r.content[0].raw {
        RawContent::Text(t) => &t.text,
        other => panic!("expected text content, got {:?}", other),
    }
}

/// Assert success and return the text content.
fn ok_text(r: CallToolResult) -> String {
    assert!(
        r.is_error != Some(true),
        "expected success, got error: {}",
        result_text(&r)
    );
    result_text(&r).to_owned()
}

// ── TC-001: req_coverage ───────────────────────────────────────────────────

/// TC-001: req_coverage returns a JSON object with expected coverage fields.
// VERIFIES: TST-0011 TC-003
#[test]
fn test_mcp_tool_coverage() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server.req_coverage().unwrap();
    let text = ok_text(result);

    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["hlr_total"], 1, "expected 1 HLR");
    assert_eq!(json["llr_total"], 1, "expected 1 LLR");
    assert_eq!(json["hlr_with_llr"], 1, "HLR should be covered by LLR");
}

// ── TC-002: req_gaps ───────────────────────────────────────────────────────

/// TC-002: req_gaps returns a JSON object; LLR-0001 without code ref appears in llr_without_code.
// VERIFIES: TST-0011 TC-004
#[test]
fn test_mcp_tool_gaps() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server.req_gaps().unwrap();
    let text = ok_text(result);

    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let without_code = json["llr_without_code"].as_array().unwrap();
    assert!(
        without_code.iter().any(|v| v == "LLR-0001"),
        "LLR-0001 should appear in llr_without_code"
    );
}

// ── TC-003: req_list ───────────────────────────────────────────────────────

/// TC-003a: req_list with type=hlr returns only HLRs.
// VERIFIES: TST-0011 TC-005
#[test]
fn test_mcp_tool_list_type_filter() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server
        .req_list(Parameters(ListInput { r#type: Some("hlr".to_string()), status: None }))
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], "HLR-0001");
}

/// TC-003b: req_list with status=approved returns all approved requirements.
// VERIFIES: TST-0011 TC-005
#[test]
fn test_mcp_tool_list_status_filter() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    // Both seeded requirements default to RequirementStatus::Draft (Requirement::new uses Draft)
    let draft = server
        .req_list(Parameters(ListInput { r#type: None, status: Some("draft".to_string()) }))
        .unwrap();
    let draft_arr = serde_json::from_str::<serde_json::Value>(&ok_text(draft)).unwrap();
    assert_eq!(draft_arr.as_array().unwrap().len(), 2, "both reqs should be draft");

    let approved = server
        .req_list(Parameters(ListInput { r#type: None, status: Some("approved".to_string()) }))
        .unwrap();
    let approved_arr = serde_json::from_str::<serde_json::Value>(&ok_text(approved)).unwrap();
    assert_eq!(approved_arr.as_array().unwrap().len(), 0, "no approved reqs");
}

// ── TC-004: req_check ─────────────────────────────────────────────────────

/// TC-004: req_check returns a JSON array.
// VERIFIES: TST-0011
#[test]
fn test_mcp_tool_check() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server.req_check().unwrap();
    let json: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    assert!(json.is_array(), "req_check must return a JSON array");
}

// ── TC-005: req_export ────────────────────────────────────────────────────

/// TC-005: req_export with format=json returns a JSON array of requirements.
// VERIFIES: TST-0011
#[test]
fn test_mcp_tool_export_json() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server
        .req_export(Parameters(ExportInput { format: "json".to_string(), id: None }))
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    assert!(json.is_array(), "json export should return an array");
    assert_eq!(json.as_array().unwrap().len(), 2);
}

/// TC-005b: req_export with format=ai-context returns an AiExport object.
// VERIFIES: TST-0011
#[test]
fn test_mcp_tool_export_ai_context() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server
        .req_export(Parameters(ExportInput { format: "ai-context".to_string(), id: None }))
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    assert!(json.get("requirements").is_some(), "ai-context must have requirements key");
    assert!(json.get("coverage").is_some(), "ai-context must have coverage key");
}

// ── TC-006: resources ─────────────────────────────────────────────────────

/// TC-006a: req://coverage resource returns JSON with coverage fields.
// VERIFIES: TST-0011 TC-006
#[test]
fn test_mcp_resource_coverage() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let json_str = server.dispatch_resource_pub("req://coverage").unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert!(json.get("hlr_total").is_some());
}

/// TC-006b: req://requirements/hlr returns all HLRs.
// VERIFIES: TST-0011 TC-006
#[test]
fn test_mcp_resource_requirements_type() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let json_str = server.dispatch_resource_pub("req://requirements/hlr").unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], "HLR-0001");
}

/// TC-006c: req://requirements/hlr/HLR-0001 returns a single requirement.
// VERIFIES: TST-0011 TC-006
#[test]
fn test_mcp_resource_single_requirement() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let json_str = server
        .dispatch_resource_pub("req://requirements/hlr/HLR-0001")
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(json["id"], "HLR-0001");
    assert_eq!(json["type"], "hlr");
}

/// TC-006d: Unknown resource URI returns an error.
// VERIFIES: TST-0011 TC-006
#[test]
fn test_mcp_resource_unknown_uri_errors() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    assert!(server.dispatch_resource_pub("req://nonexistent").is_err());
}

// ── TC-007: req_audit_coverage ────────────────────────────────────────────

/// TC-007a: req_audit_coverage returns a JSON array of LineCoverageScore for a valid report.
// VERIFIES: TST-0011
// VERIFIES: LLR-0025
#[test]
fn test_mcp_audit_coverage_valid_report() {
    use std::io::Write;
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    // Minimal llvm-cov JSON report: one file, one segment, one function
    let json = r#"{
        "data": [{
            "files": [{
                "filename": "src/lib.rs",
                "segments": [[1, 0, 3, true, true], [5, 0, 0, true, false]]
            }],
            "functions": []
        }]
    }"#;
    let report_path = dir.path().join("cov.json");
    let mut f = std::fs::File::create(&report_path).unwrap();
    f.write_all(json.as_bytes()).unwrap();

    // Seed a code ref so audit_coverage has something to score
    let cr = req_engine::CodeRef {
        req_id: "LLR-0001".to_string(),
        file: std::path::PathBuf::from("src/lib.rs"),
        line: 1,
        line_end: Some(5),
        hash: None,
        symbol: None,
    };
    make_engine(&dir).cache().insert_code_ref(&cr).unwrap();

    let result = server
        .req_audit_coverage(Parameters(AuditCoverageInput {
            report: report_path.to_string_lossy().into_owned(),
        }))
        .unwrap();

    let text = ok_text(result);
    let scores: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(scores.is_array(), "req_audit_coverage must return a JSON array");
}

/// TC-007b: req_audit_coverage returns an error when the report file is missing.
// VERIFIES: TST-0011
// VERIFIES: LLR-0025
#[test]
fn test_mcp_audit_coverage_missing_file_errors() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let err = server
        .req_audit_coverage(Parameters(AuditCoverageInput {
            report: dir.path().join("no-such-file.json").to_string_lossy().into_owned(),
        }))
        .unwrap_err();

    assert!(
        err.message.contains("IO") || err.message.contains("error") || !err.message.is_empty(),
        "missing report should produce a non-empty error message"
    );
}

// ── TC-008: req_create_requirement ─────────────────────────────────────────

/// TC-008: req_create_requirement creates a new LLR and returns it as JSON.
// VERIFIES: TST-0011
// VERIFIES: LLR-0025
#[test]
fn test_mcp_create_requirement() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server
        .req_create_requirement(Parameters(CreateRequirementInput {
            r#type: "llr".to_string(),
            title: "Created via MCP".to_string(),
            parent: Some("HLR-0001".to_string()),
            status: "draft".to_string(),
        }))
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    assert_eq!(json["parent"], "HLR-0001");
    assert_eq!(json["title"], "Created via MCP");
    assert_eq!(json["status"], "draft");
    assert_eq!(json["type"], "llr");
}

/// TC-008b: req_create_requirement rejects an unknown type.
#[test]
fn test_mcp_create_requirement_bad_type() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let err = server
        .req_create_requirement(Parameters(CreateRequirementInput {
            r#type: "bogus".to_string(),
            title: "x".to_string(),
            parent: None,
            status: "draft".to_string(),
        }))
        .unwrap_err();
    assert!(!err.message.is_empty());
}

// ── TC-009: req_remove ─────────────────────────────────────────────────────

/// TC-009: req_remove purges a requirement from the cache.
// VERIFIES: TST-0011
// VERIFIES: LLR-0025
#[test]
fn test_mcp_remove() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server
        .req_remove(Parameters(RemoveInput {
            id: "LLR-0001".to_string(),
            cache_only: true,
            force: true,
        }))
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    assert_eq!(json["id"], "LLR-0001");
    assert_eq!(json["file_deleted"], false);
}

/// TC-009b: req_remove on an unknown ID returns an error.
#[test]
fn test_mcp_remove_unknown_id() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let _err = server
        .req_remove(Parameters(RemoveInput {
            id: "LLR-9999".to_string(),
            cache_only: true,
            force: true,
        }))
        .unwrap_err();
}

// ── TC-010: req_import ─────────────────────────────────────────────────────

/// TC-010: req_import imports requirements from a JSON file.
// VERIFIES: TST-0011
// VERIFIES: LLR-0025
#[test]
fn test_mcp_import_json() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let json = r#"[{
        "id": "HLR-0099",
        "type": "hlr",
        "title": "Imported HLR",
        "status": "approved",
        "text": "",
        "parent": null,
        "source_file": null,
        "created_at": null,
        "updated_at": null,
        "attributes": {}
    }]"#;
    let path = dir.path().join("import.json");
    std::fs::write(&path, json).unwrap();

    let result = server
        .req_import(Parameters(ImportInput {
            input: path.to_string_lossy().into_owned(),
            format: "json".to_string(),
            provenance: None,
        }))
        .unwrap();
    let arr: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    assert!(arr.is_array());
    assert_eq!(arr.as_array().unwrap().len(), 1);
    assert_eq!(arr[0]["id"], "HLR-0099");
}

// ── TC-011: req_import_ai ──────────────────────────────────────────────────

/// TC-011: req_import_ai imports suggestions and forces draft status.
// VERIFIES: TST-0011
// VERIFIES: LLR-0025
#[test]
fn test_mcp_import_ai() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let json = r#"{
        "source": "test-model",
        "suggestions": [{
            "type": "llr",
            "title": "AI-suggested LLR",
            "parent": "HLR-0001",
            "text": "Shall do X"
        }]
    }"#;
    let path = dir.path().join("suggestions.json");
    std::fs::write(&path, json).unwrap();

    let result = server
        .req_import_ai(Parameters(ImportAiInput {
            input: path.to_string_lossy().into_owned(),
            dry_run: false,
            provenance: Some("test-model".to_string()),
        }))
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    let imported = json["imported"].as_array().unwrap();
    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0]["status"], "draft", "AI imports must be forced to draft");
    assert_eq!(imported[0]["parent"], "HLR-0001");
}

/// TC-011b: req_import_ai dry-run does not write.
#[test]
fn test_mcp_import_ai_dry_run() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let json = r#"{
        "source": "test-model",
        "suggestions": [{
            "type": "llr",
            "title": "Dry-run LLR",
            "parent": "HLR-0001",
            "text": "Shall do Y"
        }]
    }"#;
    let path = dir.path().join("suggestions_dry.json");
    std::fs::write(&path, json).unwrap();

    let result = server
        .req_import_ai(Parameters(ImportAiInput {
            input: path.to_string_lossy().into_owned(),
            dry_run: true,
            provenance: None,
        }))
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    assert_eq!(json["imported"].as_array().unwrap().len(), 1);
    // The requirement should not have been written to disk
    let req_dir = dir.path().join("requirements/llr");
    let files: Vec<_> = std::fs::read_dir(&req_dir)
        .map(|it| it.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    assert!(files.is_empty(), "dry-run should not write files");
}

// ── TC-012: req_migrate ────────────────────────────────────────────────────

/// TC-012: req_migrate dry-run reports without writing.
// VERIFIES: TST-0011
// VERIFIES: LLR-0025
#[test]
fn test_mcp_migrate_dry_run() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server
        .req_migrate(Parameters(MigrateInput { dry_run: true }))
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    assert_eq!(json["dry_run"], true);
    assert!(json["total"].as_u64().is_some());
}

// ── TC-013: req_audit_triviality ───────────────────────────────────────────

/// TC-013: req_audit_triviality returns a JSON array.
// VERIFIES: TST-0011
// VERIFIES: LLR-0025
#[test]
fn test_mcp_audit_triviality() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server
        .req_audit_triviality(Parameters(OptionalIdInput { id: None }))
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    assert!(json.is_array(), "req_audit_triviality must return a JSON array");
}

// ── TC-014: req_audit_criteria ─────────────────────────────────────────────

/// TC-014: req_audit_criteria returns a CriteriaReport for an LLR.
// VERIFIES: TST-0011
// VERIFIES: LLR-0025
#[test]
fn test_mcp_audit_criteria() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server
        .req_audit_criteria(Parameters(IdInput { id: "LLR-0001".to_string() }))
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    assert_eq!(json["req_id"], "LLR-0001");
    assert!(json.get("criteria").is_some());
}

// ── TC-015: req_audit_mutation ─────────────────────────────────────────────

/// TC-015: req_audit_mutation parses a valid cargo-mutants report.
// VERIFIES: TST-0011
// VERIFIES: LLR-0025
#[test]
fn test_mcp_audit_mutation() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let json = r#"[
        { "file": "src/lib.rs", "line": 1, "kind": "ReplaceWithDefault", "outcome": "Caught" }
    ]"#;
    let path = dir.path().join("mutants.json");
    std::fs::write(&path, json).unwrap();

    let result = server
        .req_audit_mutation(Parameters(AuditMutationInput {
            report: path.to_string_lossy().into_owned(),
        }))
        .unwrap();
    let val: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    assert!(val.get("scores").is_some(), "MutationReport should have a scores field");
}

/// TC-015b: req_audit_mutation errors on a missing report file.
#[test]
fn test_mcp_audit_mutation_missing_file() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let _err = server
        .req_audit_mutation(Parameters(AuditMutationInput {
            report: dir.path().join("nope.json").to_string_lossy().into_owned(),
        }))
        .unwrap_err();
}

// ── TC-016: req_audit_export_context ───────────────────────────────────────

/// TC-016: req_audit_export_context returns an AuditBundle for an LLR.
// VERIFIES: TST-0011
// VERIFIES: LLR-0025
#[test]
fn test_mcp_audit_export_context() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server
        .req_audit_export_context(Parameters(AuditExportContextInput {
            id: "LLR-0001".to_string(),
            mutation: None,
            coverage: None,
        }))
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    assert_eq!(json["llr"]["id"], "LLR-0001");
    assert!(json.get("prompt_hint").is_some());
    assert!(json.get("schema_version").is_some());
}

// ── TC-017: req_audit_independence ─────────────────────────────────────────

/// TC-017: req_audit_independence returns an IndependenceResult.
// VERIFIES: TST-0011
// VERIFIES: LLR-0025
#[test]
fn test_mcp_audit_independence() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server
        .req_audit_independence(Parameters(OptionalIdInput { id: None }))
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    assert!(json.get("violations").is_some());
    assert!(json.get("warnings").is_some());
}

// ── TC-018: req_check_provenance ───────────────────────────────────────────

/// TC-018: req_check_provenance returns an array of violations.
// VERIFIES: TST-0011
// VERIFIES: LLR-0025
#[test]
fn test_mcp_check_provenance() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server.req_check_provenance().unwrap();
    let json: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    assert!(json.is_array(), "req_check_provenance must return a JSON array");
}

// ── TC-019: req_init ───────────────────────────────────────────────────────

/// TC-019: req_init reports already-initialised on an existing project.
// VERIFIES: TST-0011
// VERIFIES: LLR-0025
#[test]
fn test_mcp_init_already_initialized() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server.req_init().unwrap();
    let json: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    assert_eq!(json["initialized"], true);
    assert_eq!(json["message"], "Project already initialized");
}

// ── TC-020: req_export with id and markdown ────────────────────────────────

/// TC-020: req_export with an id exports a single requirement.
// VERIFIES: TST-0011
// VERIFIES: LLR-0025
#[test]
fn test_mcp_export_single_by_id() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server
        .req_export(Parameters(ExportInput {
            format: "json".to_string(),
            id: Some("HLR-0001".to_string()),
        }))
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&ok_text(result)).unwrap();
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 1);
    assert_eq!(json[0]["id"], "HLR-0001");
}

/// TC-020b: req_export with markdown format returns text.
#[test]
fn test_mcp_export_markdown() {
    let dir = TempDir::new().unwrap();
    let server = ReqServer::new(make_engine(&dir));

    let result = server
        .req_export(Parameters(ExportInput {
            format: "markdown".to_string(),
            id: Some("HLR-0001".to_string()),
        }))
        .unwrap();
    let text = ok_text(result);
    assert!(text.contains("HLR-0001"), "markdown export should contain the ID");
}

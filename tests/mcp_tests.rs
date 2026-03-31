//! Integration tests for the MCP server implementation.
// REQ: TST-0011
// REQ: LLR-0025
// VERIFIES: LLR-0025

use req_engine::{Requirement, RequirementType, ReqEngine};
use req_mcp::server::{AuditCoverageInput, ExportInput, ListInput, ReqServer};
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
        .req_export(Parameters(ExportInput { format: "json".to_string() }))
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
        .req_export(Parameters(ExportInput { format: "ai-context".to_string() }))
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

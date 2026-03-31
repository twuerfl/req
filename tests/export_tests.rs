//! Integration tests for export formats, JSON adapter, attributes, and schema versioning.
// REQ: TST-0020
// REQ: TST-0021
// REQ: TST-0024
// REQ: TST-0025
// VERIFIES: LLR-0017
// VERIFIES: LLR-0007
// VERIFIES: LLR-0015
// VERIFIES: LLR-0014

use req_engine::adapter::markdown::MarkdownAdapter;
use req_engine::{Coverage, ReqEngine, Requirement, RequirementStatus, RequirementType, SCHEMA_VERSION};
use req_lib::AiExport;
use tempfile::TempDir;

// ── helpers ───────────────────────────────────────────────────────────────────

fn make_engine() -> (ReqEngine, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    ReqEngine::init(dir.path(), Some("test-project")).unwrap();
    let engine = ReqEngine::open(dir.path()).unwrap();
    (engine, dir)
}

fn seed_hlr(engine: &ReqEngine, id: &str) {
    let req = Requirement::new(id.to_string(), RequirementType::Hlr, format!("HLR {id}"));
    engine.cache().upsert_requirement(&req).unwrap();
}

fn seed_llr(engine: &ReqEngine, id: &str, parent: &str) {
    let mut req = Requirement::new(id.to_string(), RequirementType::Llr, format!("LLR {id}"));
    req.parent = Some(parent.to_string());
    req.status = RequirementStatus::Approved;
    engine.cache().upsert_requirement(&req).unwrap();
}

fn make_req_with_attrs(id: &str, attrs: &[(&str, &str)]) -> Requirement {
    let mut req = Requirement::new(id.to_string(), RequirementType::Llr, "Test".to_string());
    for (k, v) in attrs {
        req.attributes.insert(k.to_string(), v.to_string());
    }
    req
}

fn make_coverage() -> Coverage {
    Coverage {
        hlr_total: 0,
        hlr_with_llr: 0,
        llr_total: 0,
        llr_implemented: 0,
        llr_tested: 0,
        orphan_code: 0,
    }
}

// ── TST-0020: Export Format Tests ─────────────────────────────────────────────

/// TC-001 — JSON export returns a valid JSON array with all seeded requirements.
// VERIFIES: TST-0020
#[test]
fn export_json_returns_valid_array() {
    let (engine, _dir) = make_engine();
    seed_hlr(&engine, "HLR-0001");
    seed_llr(&engine, "LLR-0001", "HLR-0001");

    let json = engine.export("json", None).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(parsed.is_array(), "export json should return an array");
    assert!(parsed.as_array().unwrap().len() >= 2);
}

/// TC-002 — JSON export with --id returns exactly one requirement.
// VERIFIES: TST-0020
#[test]
fn export_json_single_id() {
    let (engine, _dir) = make_engine();
    seed_hlr(&engine, "HLR-0001");

    let json = engine.export("json", Some("HLR-0001")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let arr = parsed.as_array().unwrap();

    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], "HLR-0001");
}

/// TC-003 — JSON export with unknown ID returns Err.
// VERIFIES: TST-0020
#[test]
fn export_json_unknown_id_returns_err() {
    let (engine, _dir) = make_engine();
    assert!(engine.export("json", Some("HLR-9999")).is_err());
}

/// TC-004 — Markdown export contains YAML frontmatter delimiters and id field.
// VERIFIES: TST-0020
#[test]
fn export_markdown_contains_frontmatter() {
    let (engine, _dir) = make_engine();
    seed_hlr(&engine, "HLR-0001");

    let md = engine.export("markdown", None).unwrap();

    assert!(md.contains("---"), "expected YAML frontmatter delimiter");
    assert!(md.contains("id: HLR-0001"), "expected id field in frontmatter");
}

/// TC-005 — Markdown export with --id contains exactly one id occurrence.
// VERIFIES: TST-0020
#[test]
fn export_markdown_single_id() {
    let (engine, _dir) = make_engine();
    seed_hlr(&engine, "HLR-0001");
    seed_hlr(&engine, "HLR-0002");

    let md = engine.export("markdown", Some("HLR-0001")).unwrap();

    let count = md.matches("id: HLR-0001").count();
    assert_eq!(count, 1, "expected exactly one id: HLR-0001");
    assert!(!md.contains("id: HLR-0002"), "should not contain HLR-0002");
}

/// TC-006 — Unknown format returns Err containing "Unknown export format".
// VERIFIES: TST-0020
#[test]
fn export_unknown_format_returns_err() {
    let (engine, _dir) = make_engine();
    let err = engine.export("xml", None).unwrap_err();
    assert!(
        err.to_string().contains("Unknown export format"),
        "unexpected error: {err}"
    );
}

// ── TST-0021: JSON Adapter Roundtrip ──────────────────────────────────────────

/// TC-001 — Export then import via temp file preserves all core fields.
// VERIFIES: TST-0021
#[test]
fn json_roundtrip_preserves_fields() {
    let (engine, dir) = make_engine();
    let mut req = Requirement::new("HLR-RT-01".to_string(), RequirementType::Hlr, "Roundtrip".to_string());
    req.text = "Body text.".to_string();
    req.status = RequirementStatus::Approved;
    engine.cache().upsert_requirement(&req).unwrap();

    // Export to file
    let json = engine.export("json", Some("HLR-RT-01")).unwrap();
    let export_path = dir.path().join("export.json");
    std::fs::write(&export_path, &json).unwrap();

    // Import into a fresh engine
    let (engine2, _dir2) = make_engine();
    let imported = engine2.import(&export_path, "json", None).unwrap();

    assert_eq!(imported.len(), 1);
    let r = &imported[0];
    assert_eq!(r.id, "HLR-RT-01");
    assert_eq!(r.title, "Roundtrip");
    assert_eq!(r.text, "Body text.");
    assert_eq!(r.status, RequirementStatus::Approved);
}

/// TC-002 — Import from JSON file adds requirements to the store.
// VERIFIES: TST-0021
#[test]
fn json_import_adds_to_store() {
    let (engine, dir) = make_engine();

    let json = serde_json::json!([
        {"id": "HLR-IMP-01", "type": "hlr", "title": "Imported HLR", "text": "", "status": "draft",
         "parent": null, "aliases": [], "attributes": {}},
        {"id": "HLR-IMP-02", "type": "hlr", "title": "Imported HLR 2", "text": "", "status": "draft",
         "parent": null, "aliases": [], "attributes": {}}
    ]);
    let path = dir.path().join("import.json");
    std::fs::write(&path, json.to_string()).unwrap();

    let imported = engine.import(&path, "json", None).unwrap();
    assert_eq!(imported.len(), 2);

    let all = engine.list_requirements(None).unwrap();
    let ids: Vec<&str> = all.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"HLR-IMP-01"));
    assert!(ids.contains(&"HLR-IMP-02"));
}

/// TC-003 — Import of malformed JSON returns Err.
// VERIFIES: TST-0021
#[test]
fn json_import_malformed_returns_err() {
    let (engine, dir) = make_engine();
    let path = dir.path().join("bad.json");
    std::fs::write(&path, "{broken json").unwrap();
    assert!(engine.import(&path, "json", None).is_err());
}

// ── TST-0024: Flexible Metadata Attributes ────────────────────────────────────

/// TC-001 — Attributes roundtrip through Markdown serialisation.
// VERIFIES: TST-0024
#[test]
fn attributes_roundtrip_markdown() {
    let adapter = MarkdownAdapter::new();
    let req = make_req_with_attrs("LLR-ATTR-01", &[("risk", "high"), ("source", "DOORS-42")]);

    let md = adapter.format_to_string(&req).unwrap();
    let parsed = adapter.parse_content(&md, None).unwrap();

    assert_eq!(parsed.attributes.get("risk").map(String::as_str), Some("high"));
    assert_eq!(parsed.attributes.get("source").map(String::as_str), Some("DOORS-42"));
}

/// TC-002 — Attributes roundtrip through JSON serialisation.
// VERIFIES: TST-0024
#[test]
fn attributes_roundtrip_json() {
    let req = make_req_with_attrs("LLR-ATTR-02", &[("priority", "P1")]);
    let json = serde_json::to_string(&req).unwrap();
    let parsed: Requirement = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.attributes.get("priority").map(String::as_str), Some("P1"));
}

/// TC-003 — Requirement without attributes section parses cleanly.
// VERIFIES: TST-0024
#[test]
fn attributes_absent_parses_as_empty() {
    let content = "---\nid: LLR-NOATTR\ntype: llr\ntitle: \"No attrs\"\nstatus: draft\n---\n";
    let adapter = MarkdownAdapter::new();
    let req = adapter.parse_content(content, None).unwrap();
    assert!(req.attributes.is_empty());
}

/// TC-004 — Attribute keys are case-sensitive and preserved exactly.
// VERIFIES: TST-0024
#[test]
fn attributes_case_sensitive_keys() {
    let adapter = MarkdownAdapter::new();
    let req = make_req_with_attrs("LLR-ATTR-03", &[("CamelCase", "v1"), ("snake_case", "v2")]);
    let md = adapter.format_to_string(&req).unwrap();
    let parsed = adapter.parse_content(&md, None).unwrap();
    assert_eq!(parsed.attributes.get("CamelCase").map(String::as_str), Some("v1"));
    assert_eq!(parsed.attributes.get("snake_case").map(String::as_str), Some("v2"));
}

// ── TST-0025: Schema Versioning ───────────────────────────────────────────────

/// TC-001 — SCHEMA_VERSION constant equals "1.0.0".
// VERIFIES: TST-0025
#[test]
fn schema_version_constant_value() {
    assert_eq!(SCHEMA_VERSION, "1.0.0");
}

/// TC-002 — AiExport.schema_version equals SCHEMA_VERSION.
// VERIFIES: TST-0025
#[test]
fn ai_export_schema_version_matches_constant() {
    let cov = make_coverage();
    let export = AiExport::new(vec![], vec![], vec![], cov);
    assert_eq!(export.schema_version, SCHEMA_VERSION);
}

/// TC-003 — AiExport serialises schema_version into JSON output.
// VERIFIES: TST-0025
#[test]
fn ai_export_json_contains_schema_version() {
    let cov = make_coverage();
    let export = AiExport::new(vec![], vec![], vec![], cov);
    let json = serde_json::to_string(&export).unwrap();
    assert!(
        json.contains("\"schema_version\":\"1.0.0\""),
        "JSON should contain schema_version field"
    );
}

/// TC-004 — AiExport.tool_version is non-empty.
// VERIFIES: TST-0025
#[test]
fn ai_export_tool_version_non_empty() {
    let cov = make_coverage();
    let export = AiExport::new(vec![], vec![], vec![], cov);
    assert!(!export.tool_version.is_empty());
}

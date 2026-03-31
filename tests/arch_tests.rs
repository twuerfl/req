//! Architecture compliance tests for `req_lib` (TST-0030), `req_engine` (TST-0031),
//! `req_cli` (TST-0032), and `req_mcp` (TST-0033).
// REQ: TST-0030
// REQ: TST-0031
// REQ: TST-0032
// REQ: TST-0033
// VERIFIES: LLR-0021
// VERIFIES: LLR-0022
// VERIFIES: LLR-0023
// VERIFIES: LLR-0024

// ── TST-0030: req_lib architecture compliance ─────────────────────────────────

/// TC-001 — All core types are accessible from the crate root.
///
/// This test is purely a compilation check: if any re-export is missing the
/// file will not compile and the test suite will fail.
// VERIFIES: TST-0030
#[allow(unused_imports)]
#[test]
fn all_core_types_accessible_from_crate_root() {
    use req_lib::{
        AiExport, CodeRef, Coverage, Link, LinkType, Requirement, RequirementStatus,
        RequirementType, SCHEMA_VERSION,
    };
    // Reaching here means every name resolved at compile time.
    let _ = SCHEMA_VERSION;
}

/// TC-002 — Requirement round-trips through serde_json.
// VERIFIES: TST-0030
#[test]
fn requirement_serde_roundtrip() {
    use req_lib::{Requirement, RequirementStatus, RequirementType};

    let mut original = Requirement::new(
        "LLR-ARCH-01".to_string(),
        RequirementType::Llr,
        "Architecture compliance req".to_string(),
    );
    original.status = RequirementStatus::Approved;
    original.text = "Some body text.".to_string();
    original.parent = Some("HLR-ARCH-01".to_string());

    let json = serde_json::to_string(&original).expect("serialize should succeed");
    let deserialized: Requirement =
        serde_json::from_str(&json).expect("deserialize should succeed");

    assert_eq!(deserialized.id, original.id);
    assert_eq!(deserialized.req_type, original.req_type);
    assert_eq!(deserialized.title, original.title);
    assert_eq!(deserialized.status, original.status);
    assert_eq!(deserialized.text, original.text);
    assert_eq!(deserialized.parent, original.parent);
}

/// TC-003 — Coverage round-trips through serde_json.
// VERIFIES: TST-0030
#[test]
fn coverage_serde_roundtrip() {
    use req_lib::Coverage;

    let original = Coverage {
        hlr_total: 10,
        hlr_with_llr: 8,
        llr_total: 50,
        llr_implemented: 45,
        llr_tested: 30,
        orphan_code: 2,
    };

    let json = serde_json::to_string(&original).expect("serialize should succeed");
    let deserialized: Coverage =
        serde_json::from_str(&json).expect("deserialize should succeed");

    assert_eq!(deserialized.hlr_total, original.hlr_total);
    assert_eq!(deserialized.hlr_with_llr, original.hlr_with_llr);
    assert_eq!(deserialized.llr_total, original.llr_total);
    assert_eq!(deserialized.llr_implemented, original.llr_implemented);
    assert_eq!(deserialized.llr_tested, original.llr_tested);
    assert_eq!(deserialized.orphan_code, original.orphan_code);
}

/// TC-004 — SCHEMA_VERSION is a non-empty string.
// VERIFIES: TST-0030
#[test]
fn schema_version_is_non_empty() {
    assert!(
        req_lib::SCHEMA_VERSION.len() > 0,
        "SCHEMA_VERSION must not be empty"
    );
}

// ── TST-0031: req_engine architecture compliance ──────────────────────────────

/// TC-001 — req_lib public types are accessible via the req_engine crate root.
// VERIFIES: TST-0031
#[allow(unused_imports)]
#[test]
fn req_engine_reexports_req_lib_types() {
    use req_engine::{CodeRef, Coverage, Link, Requirement, SCHEMA_VERSION};
    let _ = SCHEMA_VERSION;
}

/// TC-002 — scan, coverage, gaps, validate, export, and import all return Ok on a fresh engine.
// VERIFIES: TST-0031
#[test]
fn req_engine_facade_methods_return_ok() {
    use req_engine::ReqEngine;
    use std::io::Write;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    ReqEngine::init(dir.path(), Some("arch-test")).unwrap();
    let engine = ReqEngine::open(dir.path()).unwrap();

    // scan
    engine.scan(None, false).expect("scan should return Ok");

    // coverage
    engine.coverage().expect("coverage should return Ok");

    // gaps
    engine.gaps().expect("gaps should return Ok");

    // validate
    engine.validate().expect("validate should return Ok");

    // export (json)
    engine.export("json", None).expect("export json should return Ok");

    // import — write a minimal JSON export file, then import it
    let export_path = dir.path().join("export.json");
    let json = engine.export("json", None).unwrap();
    let mut f = std::fs::File::create(&export_path).unwrap();
    f.write_all(json.as_bytes()).unwrap();
    drop(f);
    engine
        .import(&export_path, "json", None)
        .expect("import json should return Ok");
}

/// TC-003 — req_engine/Cargo.toml contains no terminal-presentation dependencies.
// VERIFIES: TST-0031
#[test]
fn req_engine_has_no_terminal_presentation_deps() {
    let cargo_toml_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("req_engine")
        .join("Cargo.toml");

    let content = std::fs::read_to_string(&cargo_toml_path)
        .expect("should be able to read req_engine/Cargo.toml");

    assert!(
        !content.contains("colored"),
        "req_engine must not depend on 'colored'"
    );
    assert!(
        !content.contains("indicatif"),
        "req_engine must not depend on 'indicatif'"
    );
}

// ── TST-0032: req_cli architecture compliance ─────────────────────────────────

/// TC-001 — The `req` binary artifact exists in `target/debug/` after a build.
// VERIFIES: TST-0032
#[test]
fn req_cli_binary_named_req_exists() {
    let exe = if cfg!(windows) { "req.exe" } else { "req" };
    let binary = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("debug")
        .join(exe);
    assert!(
        binary.exists(),
        "binary '{exe}' should exist at {}: run `cargo build` first",
        binary.display()
    );
}

/// TC-002 — `req_cli/Cargo.toml` declares `[[bin]] name = "req"`.
// VERIFIES: TST-0032
#[test]
fn req_cli_cargo_toml_declares_bin_name_req() {
    let cargo_toml = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("req_cli")
        .join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml)
        .expect("should be able to read req_cli/Cargo.toml");

    assert!(
        content.contains("[[bin]]"),
        "req_cli/Cargo.toml must have a [[bin]] section"
    );
    assert!(
        content.contains("name = \"req\""),
        "req_cli/Cargo.toml [[bin]] must declare name = \"req\""
    );
}

/// TC-003 — `req_cli/Cargo.toml` uses only `req_engine` as a path dependency (not req_lib directly).
// VERIFIES: TST-0032
#[test]
fn req_cli_depends_only_on_req_engine_not_req_lib() {
    let cargo_toml = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("req_cli")
        .join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml)
        .expect("should be able to read req_cli/Cargo.toml");

    assert!(
        content.contains("req_engine"),
        "req_cli/Cargo.toml must depend on req_engine"
    );
    assert!(
        !content.contains("path = \"../req_lib\""),
        "req_cli must not take a direct path dependency on req_lib"
    );
}

// ── TST-0033: req_mcp architecture compliance ─────────────────────────────────

fn read_req_mcp_cargo_toml() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("req_mcp")
        .join("Cargo.toml");
    std::fs::read_to_string(&path).expect("should be able to read req_mcp/Cargo.toml")
}

/// TC-001 — `req_mcp/Cargo.toml` declares `[[bin]] name = "req-mcp-server"`.
// VERIFIES: TST-0033
#[test]
fn req_mcp_cargo_toml_declares_bin_name_req_mcp_server() {
    let content = read_req_mcp_cargo_toml();
    assert!(
        content.contains("[[bin]]"),
        "req_mcp/Cargo.toml must have a [[bin]] section"
    );
    assert!(
        content.contains("name = \"req-mcp-server\""),
        "req_mcp/Cargo.toml [[bin]] must declare name = \"req-mcp-server\""
    );
}

/// TC-002 — `req_mcp/Cargo.toml` lists `rmcp` with `"server"` and `"macros"` features.
// VERIFIES: TST-0033
#[test]
fn req_mcp_cargo_toml_rmcp_has_server_and_macros_features() {
    let content = read_req_mcp_cargo_toml();
    assert!(
        content.contains("rmcp"),
        "req_mcp/Cargo.toml must depend on rmcp"
    );
    assert!(
        content.contains("\"server\""),
        "rmcp dependency must include feature \"server\""
    );
    assert!(
        content.contains("\"macros\""),
        "rmcp dependency must include feature \"macros\""
    );
}

/// TC-003 — The `req-mcp-server` binary exists in `target/debug/` after a build.
// VERIFIES: TST-0033
#[test]
fn req_mcp_server_binary_exists() {
    let exe = if cfg!(windows) {
        "req-mcp-server.exe"
    } else {
        "req-mcp-server"
    };
    let binary = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("debug")
        .join(exe);
    assert!(
        binary.exists(),
        "binary '{exe}' should exist at {}: run `cargo build` first",
        binary.display()
    );
}

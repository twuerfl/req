//! Integration tests for `engine.coverage()` — verifies correct HLR/LLR percentages.
// REQ: TST-0023
// VERIFIES: LLR-0018

use req_engine::{CodeRef, Link, LinkType, ReqEngine, Requirement, RequirementStatus, RequirementType};
use std::path::PathBuf;
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

fn add_code_ref(engine: &ReqEngine, req_id: &str) {
    let cr = CodeRef {
        req_id: req_id.to_string(),
        file: PathBuf::from("src/lib.rs"),
        line: 1,
        line_end: Some(5),
        hash: None,
        symbol: None,
    };
    engine.cache().insert_code_ref(&cr).unwrap();
}

fn add_verifies_link(engine: &ReqEngine, source: &str, target: &str) {
    let link = Link {
        source: source.to_string(),
        target: target.to_string(),
        link_type: LinkType::Verifies,
    };
    engine.cache().insert_link(&link).unwrap();
}

// ── TST-0023: Coverage Report Tests ──────────────────────────────────────────

/// TC-001 — Full chain: HLR → LLR → code ref → verifies link gives 100% coverage.
// VERIFIES: TST-0023
#[test]
fn coverage_full_chain_is_100_percent() {
    let (engine, _dir) = make_engine();
    seed_hlr(&engine, "HLR-COV-01");
    seed_llr(&engine, "LLR-COV-01", "HLR-COV-01");
    add_code_ref(&engine, "LLR-COV-01");
    add_verifies_link(&engine, "TST-COV-01", "LLR-COV-01");

    let cov = engine.coverage().unwrap();

    assert_eq!(cov.hlr_total, 1);
    assert_eq!(cov.hlr_with_llr, 1);
    assert_eq!(cov.llr_total, 1);
    assert_eq!(cov.llr_implemented, 1);
    assert_eq!(cov.llr_tested, 1);
}

/// TC-002 — HLR without any child LLR shows 0% HLR coverage.
// VERIFIES: TST-0023
#[test]
fn coverage_hlr_without_llr_is_zero() {
    let (engine, _dir) = make_engine();
    seed_hlr(&engine, "HLR-COV-02");

    let cov = engine.coverage().unwrap();

    assert_eq!(cov.hlr_total, 1);
    assert_eq!(cov.hlr_with_llr, 0);
    assert_eq!(cov.llr_total, 0);
}

/// TC-003 — LLR without any code ref is not implemented and not tested.
// VERIFIES: TST-0023
#[test]
fn coverage_llr_without_code_ref_not_implemented() {
    let (engine, _dir) = make_engine();
    seed_hlr(&engine, "HLR-COV-03");
    seed_llr(&engine, "LLR-COV-03", "HLR-COV-03");

    let cov = engine.coverage().unwrap();

    assert_eq!(cov.llr_total, 1);
    assert_eq!(cov.llr_implemented, 0);
    assert_eq!(cov.llr_tested, 0);
}

/// TC-004 — LLR with a code ref but no verifies link is implemented but not tested.
// VERIFIES: TST-0023
#[test]
fn coverage_llr_with_code_ref_but_no_verifies() {
    let (engine, _dir) = make_engine();
    seed_hlr(&engine, "HLR-COV-04");
    seed_llr(&engine, "LLR-COV-04", "HLR-COV-04");
    add_code_ref(&engine, "LLR-COV-04");

    let cov = engine.coverage().unwrap();

    assert_eq!(cov.llr_implemented, 1);
    assert_eq!(cov.llr_tested, 0);
}

/// TC-005 — Coverage percentages are correct fractions for a mixed project.
// VERIFIES: TST-0023
#[test]
fn coverage_percentage_calculation() {
    let (engine, _dir) = make_engine();
    seed_hlr(&engine, "HLR-COV-05");
    seed_llr(&engine, "LLR-COV-05A", "HLR-COV-05");
    seed_llr(&engine, "LLR-COV-05B", "HLR-COV-05");
    seed_llr(&engine, "LLR-COV-05C", "HLR-COV-05");
    seed_llr(&engine, "LLR-COV-05D", "HLR-COV-05");

    // 2 of 4 implemented
    add_code_ref(&engine, "LLR-COV-05A");
    add_code_ref(&engine, "LLR-COV-05B");

    // 1 of 4 tested
    add_verifies_link(&engine, "TST-COV-05", "LLR-COV-05A");

    let cov = engine.coverage().unwrap();

    assert_eq!(cov.llr_total, 4);
    assert_eq!(cov.llr_implemented, 2);
    assert_eq!(cov.llr_tested, 1);
    assert!((cov.llr_implementation_percent() - 50.0).abs() < 0.01);
    assert!((cov.llr_test_percent() - 25.0).abs() < 0.01);
}

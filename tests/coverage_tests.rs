//! Integration tests for coverage report generation.
// REQ: TST-0005
// VERIFIES: LLR-0005

use req_engine::{Requirement, RequirementType, CodeRef};
use req_engine::cache::Cache;
use req_engine::trace::TraceGraph;
use std::path::PathBuf;

fn make_cache(dir: &std::path::Path) -> Cache {
    Cache::open(&dir.join("test.db"), 0).unwrap()
}

/// TC-001: HLR coverage calculation — HLR with LLR child counts as covered.
// VERIFIES: TST-0005 TC-001
#[test]
fn test_hlr_coverage_with_llr() {
    let dir = tempfile::tempdir().unwrap();
    let cache = make_cache(dir.path());

    let hlr = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "Top".to_string());
    cache.upsert_requirement(&hlr).unwrap();

    let mut llr = Requirement::new("LLR-0001".to_string(), RequirementType::Llr, "Sub".to_string());
    llr.parent = Some("HLR-0001".to_string());
    cache.upsert_requirement(&llr).unwrap();

    let graph = TraceGraph::from_cache(&cache).unwrap();
    let cov = graph.calculate_coverage();

    assert_eq!(cov.hlr_total, 1);
    assert_eq!(cov.hlr_with_llr, 1);
    assert!(cov.hlr_coverage_percent() > 99.0);
}

/// TC-001b: HLR without LLR is not covered.
// VERIFIES: TST-0005 TC-001
#[test]
fn test_hlr_coverage_without_llr() {
    let dir = tempfile::tempdir().unwrap();
    let cache = make_cache(dir.path());

    let hlr = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "Top".to_string());
    cache.upsert_requirement(&hlr).unwrap();

    let graph = TraceGraph::from_cache(&cache).unwrap();
    let cov = graph.calculate_coverage();

    assert_eq!(cov.hlr_total, 1);
    assert_eq!(cov.hlr_with_llr, 0);
    assert!(cov.hlr_coverage_percent() < 1.0);
}

/// TC-002: LLR implementation coverage — LLR with code ref counts as implemented.
// VERIFIES: TST-0005 TC-002
#[test]
fn test_llr_implementation_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let cache = make_cache(dir.path());

    let llr = Requirement::new("LLR-0001".to_string(), RequirementType::Llr, "Impl".to_string());
    cache.upsert_requirement(&llr).unwrap();

    let code_ref = CodeRef {
        req_id: "LLR-0001".to_string(),
        file: PathBuf::from("src/lib.rs"),
        line: 1,
        line_end: None,
        hash: None,
        symbol: None,
    };
    cache.insert_code_ref(&code_ref).unwrap();

    let graph = TraceGraph::from_cache(&cache).unwrap();
    let cov = graph.calculate_coverage();

    assert_eq!(cov.llr_total, 1);
    assert_eq!(cov.llr_implemented, 1);
    assert!(cov.llr_implementation_percent() > 99.0);
}

/// TC-002b: LLR without code ref is not implemented.
// VERIFIES: TST-0005 TC-002
#[test]
fn test_llr_not_implemented_without_code_ref() {
    let dir = tempfile::tempdir().unwrap();
    let cache = make_cache(dir.path());

    let llr = Requirement::new("LLR-0001".to_string(), RequirementType::Llr, "Impl".to_string());
    cache.upsert_requirement(&llr).unwrap();

    let graph = TraceGraph::from_cache(&cache).unwrap();
    let cov = graph.calculate_coverage();

    assert_eq!(cov.llr_total, 1);
    assert_eq!(cov.llr_implemented, 0);
    assert!(cov.llr_implementation_percent() < 1.0);
}

/// TC-004: Coverage is JSON-serialisable.
// VERIFIES: TST-0005 TC-004
#[test]
fn test_coverage_json_serialisation() {
    let dir = tempfile::tempdir().unwrap();
    let cache = make_cache(dir.path());
    let graph = TraceGraph::from_cache(&cache).unwrap();
    let cov = graph.calculate_coverage();

    let json = serde_json::to_string(&cov).unwrap();
    assert!(json.contains("hlr_total"));
    assert!(json.contains("llr_total"));
}

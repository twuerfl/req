//! Integration tests for traceability graph and impact analysis.
// REQ: TST-0006
// REQ: TST-0010
// VERIFIES: LLR-0005
// VERIFIES: LLR-0019
// VERIFIES: LLR-0020
// VERIFIES: TST-0010

use req_engine::{Requirement, RequirementType, CodeRef};
use req_engine::cache::Cache;
use req_engine::trace::TraceGraph;
use std::path::PathBuf;

fn populated_cache(dir: &std::path::Path) -> Cache {
    let cache = Cache::open(&dir.join("test.db"), 0).unwrap();

    let hlr = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "Root".to_string());
    cache.upsert_requirement(&hlr).unwrap();

    let mut llr1 = Requirement::new("LLR-0001".to_string(), RequirementType::Llr, "Sub1".to_string());
    llr1.parent = Some("HLR-0001".to_string());
    cache.upsert_requirement(&llr1).unwrap();

    let mut llr2 = Requirement::new("LLR-0002".to_string(), RequirementType::Llr, "Sub2".to_string());
    llr2.parent = Some("HLR-0001".to_string());
    cache.upsert_requirement(&llr2).unwrap();

    let code_ref = CodeRef {
        req_id: "LLR-0001".to_string(),
        file: PathBuf::from("src/impl.rs"),
        line: 42,
        line_end: None,
        hash: None,
        symbol: Some("impl_fn".to_string()),
    };
    cache.insert_code_ref(&code_ref).unwrap();

    cache
}

/// TC-001: Trace tree for HLR returns all child LLRs.
// VERIFIES: TST-0010 TC-001
#[test]
fn test_trace_tree_hlr_has_children() {
    let dir = tempfile::tempdir().unwrap();
    let cache = populated_cache(dir.path());
    let graph = TraceGraph::from_cache(&cache).unwrap();

    let children = graph.get_children("HLR-0001");
    let ids: Vec<&str> = children.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&"LLR-0001"));
    assert!(ids.contains(&"LLR-0002"));
}

/// TC-001b: Code refs are reachable from LLR.
// VERIFIES: TST-0010 TC-001
#[test]
fn test_trace_tree_llr_has_code_refs() {
    let dir = tempfile::tempdir().unwrap();
    let cache = populated_cache(dir.path());
    let graph = TraceGraph::from_cache(&cache).unwrap();

    let refs = graph.get_code_refs("LLR-0001");
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].line, 42);
}

/// TC-002: Trace tree for LLR returns parent HLR.
// VERIFIES: TST-0010 TC-002
#[test]
fn test_trace_tree_llr_parent() {
    let dir = tempfile::tempdir().unwrap();
    let cache = populated_cache(dir.path());
    let graph = TraceGraph::from_cache(&cache).unwrap();

    let parent = graph.get_parent("LLR-0001");
    assert!(parent.is_some());
    assert_eq!(parent.unwrap().id, "HLR-0001");
}

/// TC-003: Impact analysis for HLR includes its LLRs and their code refs.
// VERIFIES: TST-0010 TC-003
#[test]
fn test_impact_analysis_direct() {
    let dir = tempfile::tempdir().unwrap();
    let cache = populated_cache(dir.path());
    let graph = TraceGraph::from_cache(&cache).unwrap();

    let impact = graph.impact_analysis("HLR-0001");
    assert!(impact.affected_requirements.iter().any(|id| id == "LLR-0001"));
    assert!(impact.affected_requirements.iter().any(|id| id == "LLR-0002"));
}

/// TC-001 (gap detection): HLR without LLR is reported as a gap.
// VERIFIES: TST-0006 TC-001
#[test]
fn test_gap_hlr_without_llr() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::open(&dir.path().join("test.db"), 0).unwrap();

    let hlr = Requirement::new("HLR-GAP".to_string(), RequirementType::Hlr, "Unimplemented".to_string());
    cache.upsert_requirement(&hlr).unwrap();

    let graph = TraceGraph::from_cache(&cache).unwrap();
    let gaps = graph.find_gaps();

    assert!(gaps.hlr_without_llr.iter().any(|id| id == "HLR-GAP"));
}

/// TC-002 (gap detection): LLR without code ref is reported as a gap.
// VERIFIES: TST-0006 TC-002
#[test]
fn test_gap_llr_without_code() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::open(&dir.path().join("test.db"), 0).unwrap();

    let llr = Requirement::new("LLR-GAP".to_string(), RequirementType::Llr, "No code".to_string());
    cache.upsert_requirement(&llr).unwrap();

    let graph = TraceGraph::from_cache(&cache).unwrap();
    let gaps = graph.find_gaps();

    assert!(gaps.llr_without_code.iter().any(|id| id == "LLR-GAP"));
}

/// TC-004: No gaps when full chain HLR→LLR→Code is present.
// VERIFIES: TST-0006 TC-004
#[test]
fn test_no_gaps_full_chain() {
    let dir = tempfile::tempdir().unwrap();
    let cache = populated_cache(dir.path());

    // Also implement LLR-0002
    let code_ref2 = CodeRef {
        req_id: "LLR-0002".to_string(),
        file: PathBuf::from("src/impl2.rs"),
        line: 10,
        line_end: None,
        hash: None,
        symbol: None,
    };
    cache.insert_code_ref(&code_ref2).unwrap();

    let graph = TraceGraph::from_cache(&cache).unwrap();
    let gaps = graph.find_gaps();

    assert!(!gaps.hlr_without_llr.iter().any(|id| id == "HLR-0001"));
    assert!(!gaps.llr_without_code.iter().any(|id| id == "LLR-0001"));
    assert!(!gaps.llr_without_code.iter().any(|id| id == "LLR-0002"));
}

// ── TST-0010: trace_tree tests ────────────────────────────────────────────────

/// TC-001 (trace_tree): trace_tree for HLR includes ID, title, and children.
// VERIFIES: TST-0010 TC-001
// VERIFIES: LLR-0019
#[test]
fn test_trace_tree_hlr_output() {
    let dir = tempfile::tempdir().unwrap();
    let cache = populated_cache(dir.path());
    let graph = TraceGraph::from_cache(&cache).unwrap();

    let output = graph.trace_tree("HLR-0001", 0).expect("trace_tree should return Some for known ID");

    assert!(output.contains("HLR-0001"), "output should include the requirement ID");
    assert!(output.contains("Root"), "output should include the title");
    assert!(output.contains("LLR-0001") || output.contains("children"), "output should mention children");
}

/// TC-002 (trace_tree): trace_tree for LLR includes parent and code refs.
// VERIFIES: TST-0010 TC-002
// VERIFIES: LLR-0019
#[test]
fn test_trace_tree_llr_output() {
    let dir = tempfile::tempdir().unwrap();
    let cache = populated_cache(dir.path());
    let graph = TraceGraph::from_cache(&cache).unwrap();

    let output = graph.trace_tree("LLR-0001", 0).expect("trace_tree should return Some for LLR-0001");

    assert!(output.contains("LLR-0001"));
    assert!(output.contains("HLR-0001"), "output should reference the parent");
    assert!(output.contains("src/impl.rs"), "output should show the code ref file");
}

/// TC-003 (trace_tree): trace_tree returns None for unknown IDs.
// VERIFIES: TST-0010 TC-001
// VERIFIES: LLR-0019
#[test]
fn test_trace_tree_unknown_id_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let cache = populated_cache(dir.path());
    let graph = TraceGraph::from_cache(&cache).unwrap();

    let result = graph.trace_tree("HLR-UNKNOWN", 0);
    assert!(result.is_none(), "trace_tree should return None for unknown requirement ID");
}

/// TC-004 (trace_tree): trace_tree with depth > 0 applies indentation.
// VERIFIES: TST-0010 TC-002
// VERIFIES: LLR-0019
#[test]
fn test_trace_tree_depth_indentation() {
    let dir = tempfile::tempdir().unwrap();
    let cache = populated_cache(dir.path());
    let graph = TraceGraph::from_cache(&cache).unwrap();

    let output_depth0 = graph.trace_tree("LLR-0001", 0).unwrap();
    let output_depth2 = graph.trace_tree("LLR-0001", 2).unwrap();

    // depth=2 means 4 leading spaces on the first line
    assert!(
        output_depth2.starts_with("    "),
        "depth=2 should add 4 spaces of indentation"
    );
    assert!(
        !output_depth0.starts_with("  "),
        "depth=0 should have no leading indentation"
    );
}

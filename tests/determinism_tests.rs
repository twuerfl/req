//! Integration tests for deterministic structured output.
// VERIFIES: LLR-0034

use req_engine::{ReqEngine, Requirement, RequirementType};
use tempfile::TempDir;

fn setup(tmp: &TempDir) -> ReqEngine {
    let base = tmp.path();
    ReqEngine::init(base, Some("test")).unwrap();
    ReqEngine::open(base).unwrap()
}

fn seed(engine: &ReqEngine, id: &str, ty: RequirementType) {
    engine
        .cache()
        .upsert_requirement(&Requirement::new(id.to_string(), ty, format!("Title {id}")))
        .unwrap();
}

/// TC-0039-01: list_requirements returns requirements sorted by ID
// VERIFIES: LLR-0034
#[test]
fn test_list_requirements_sorted_by_id() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);

    // Insert in reverse order to expose any HashMap-order leakage
    for (id, ty) in [
        ("LLR-0003", RequirementType::Llr),
        ("HLR-0002", RequirementType::Hlr),
        ("HLR-0001", RequirementType::Hlr),
        ("LLR-0001", RequirementType::Llr),
    ] {
        seed(&engine, id, ty);
    }

    let reqs = engine.list_requirements(None).unwrap();
    let ids: Vec<&str> = reqs.iter().map(|r| r.id.as_str()).collect();
    let mut expected = ids.clone();
    expected.sort();
    assert_eq!(ids, expected, "list_requirements must return IDs in lexicographic order");
}

/// TC-0039-02: export "json" is byte-identical across two calls
// VERIFIES: LLR-0034
#[test]
fn test_export_json_is_deterministic() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);

    seed(&engine, "HLR-0001", RequirementType::Hlr);
    seed(&engine, "LLR-0001", RequirementType::Llr);
    seed(&engine, "LLR-0002", RequirementType::Llr);

    let out1 = engine.export("json", None).unwrap();
    let out2 = engine.export("json", None).unwrap();
    assert_eq!(out1, out2, "export json must be byte-identical on repeated calls");
}

/// TC-0039-02: export "ai-context" contains no "exported_at" timestamp field
// VERIFIES: LLR-0034
#[test]
fn test_export_ai_context_has_no_timestamp() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    seed(&engine, "HLR-0001", RequirementType::Hlr);

    let out = engine.export("ai-context", None).unwrap();
    assert!(
        !out.contains("exported_at"),
        "ai-context export must not contain 'exported_at'"
    );
}

/// TC-0039-03: gaps output vectors are lexicographically sorted
// VERIFIES: LLR-0034
#[test]
fn test_gaps_vectors_are_sorted() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);

    // LLRs with no parent → each appears in llr_without_parent gap
    for id in ["LLR-0003", "LLR-0001", "LLR-0002"] {
        let mut req = Requirement::new(id.to_string(), RequirementType::Llr, format!("T {id}"));
        req.parent = None;
        engine.cache().upsert_requirement(&req).unwrap();
    }

    let gaps = engine.gaps().unwrap();

    let mut sorted = gaps.llr_without_parent.clone();
    sorted.sort();
    assert_eq!(
        gaps.llr_without_parent, sorted,
        "gaps.llr_without_parent must be sorted"
    );
}

/// TC-0039-04: validate issues sorted by requirement_id
// VERIFIES: LLR-0034
#[test]
fn test_validate_issues_sorted_by_requirement_id() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);

    for id in ["LLR-0003", "LLR-0001", "LLR-0002"] {
        let mut req = Requirement::new(id.to_string(), RequirementType::Llr, format!("T {id}"));
        req.parent = None;
        engine.cache().upsert_requirement(&req).unwrap();
    }

    let issues = engine.validate().unwrap();
    let ids: Vec<&str> = issues
        .iter()
        .filter_map(|i| i.requirement_id.as_deref())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "validate issues must be sorted by requirement_id");
}

/// TC-0039-05: ai-context export paths contain only forward slashes
// VERIFIES: LLR-0034
#[test]
fn test_export_ai_context_paths_forward_slashes() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    let base = tmp.path();

    seed(&engine, "LLR-0001", RequirementType::Llr);

    // Insert a code ref with a platform-native path that may contain backslashes
    engine
        .cache()
        .insert_code_ref(&req_engine::CodeRef {
            req_id: "LLR-0001".to_string(),
            file: base.join("src").join("impl.rs"),
            line: 1,
            line_end: None,
            hash: None,
            symbol: None,
        })
        .unwrap();

    let out = engine.export("ai-context", None).unwrap();
    assert!(
        !out.contains('\\'),
        "ai-context export must not contain backslashes; got:\n{out}"
    );
}

/// TC-0039-06: export "json" is byte-identical for identical repository state
// VERIFIES: LLR-0034
#[test]
fn test_export_json_stable_for_same_state() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);

    seed(&engine, "HLR-0001", RequirementType::Hlr);
    seed(&engine, "LLR-0001", RequirementType::Llr);

    let a = engine.export("json", None).unwrap();
    let b = engine.export("json", None).unwrap();
    assert_eq!(a, b, "export json must be stable across calls");
}

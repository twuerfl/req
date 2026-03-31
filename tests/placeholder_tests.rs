//! Tests for documentation placeholder requirements (TST-0035).
// REQ: TST-0035
// VERIFIES: LLR-0123

use req_engine::{ReqEngine, Requirement, RequirementType};
use req_lib::Requirement as LibRequirement;

// ── TST-0035: Documentation placeholder tests ─────────────────────────────────

/// TC-001 — "LLR-0123" parses as a valid LLR identifier with number 123.
// VERIFIES: LLR-0123
#[test]
fn placeholder_id_parses_as_valid_llr() {
    let parsed = LibRequirement::parse_id("LLR-0123");
    assert!(parsed.is_some(), "LLR-0123 should parse as a valid requirement ID");
    let (req_type, number) = parsed.unwrap();
    assert_eq!(req_type, RequirementType::Llr);
    assert_eq!(number, 123);
}

/// TC-002 — Placeholder requirement can be stored and does not cause Error-severity issues.
// VERIFIES: LLR-0123
#[test]
fn placeholder_requirement_stores_without_error_issues() {
    use req_engine::Severity;

    let dir = tempfile::tempdir().unwrap();
    ReqEngine::init(dir.path(), Some("placeholder-test")).unwrap();
    let engine = ReqEngine::open(dir.path()).unwrap();

    // Seed a minimal parent so LLR-0123 has a valid parent reference
    let hlr = Requirement::new(
        "HLR-0002".to_string(),
        RequirementType::Hlr,
        "Example HLR (documentation placeholder)".to_string(),
    );
    engine.cache().upsert_requirement(&hlr).unwrap();

    let mut llr = Requirement::new(
        "LLR-0123".to_string(),
        RequirementType::Llr,
        "Example LLR (documentation placeholder)".to_string(),
    );
    llr.parent = Some("HLR-0002".to_string());
    engine.cache().upsert_requirement(&llr).unwrap();

    let issues = engine.validate().unwrap();
    let errors_for_placeholder: Vec<_> = issues
        .iter()
        .filter(|i| {
            i.severity == Severity::Error
                && i.requirement_id.as_deref() == Some("LLR-0123")
        })
        .collect();

    assert!(
        errors_for_placeholder.is_empty(),
        "LLR-0123 should not produce Error-severity validation issues: {:?}",
        errors_for_placeholder
    );
}

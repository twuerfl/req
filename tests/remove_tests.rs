//! Integration tests for the requirement removal command.
// VERIFIES: LLR-0035

use req_engine::{Error, ReqEngine, RequirementStatus, RequirementType};
use std::fs;
use tempfile::TempDir;

fn setup(tmp: &TempDir) -> ReqEngine {
    let base = tmp.path();
    ReqEngine::init(base, Some("test")).unwrap();
    ReqEngine::open(base).unwrap()
}

/// TC-0036-01: normal removal deletes the .md file and purges all cache entries
// VERIFIES: LLR-0035
#[test]
fn test_remove_deletes_file_and_cache() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    let base = tmp.path();

    let req = engine
        .create_requirement(RequirementType::Hlr, "Test HLR", None, RequirementStatus::Approved)
        .unwrap();
    let id = req.id.clone();
    let file = base.join("requirements/hlr").join(format!("{id}.md"));

    assert!(file.exists(), "md file must exist before removal");

    let result = engine.remove_requirement(&id, false, false).unwrap();

    assert!(result.file_deleted, "file_deleted must be true");
    assert!(!file.exists(), "md file must be gone after removal");
    assert!(
        engine.cache().get_requirement(&id).unwrap().is_none(),
        "cache entry must be absent after removal"
    );
}

/// TC-0036-02: --cache-only leaves the .md file on disk
// VERIFIES: LLR-0035
#[test]
fn test_remove_cache_only_leaves_file_on_disk() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    let base = tmp.path();

    let req = engine
        .create_requirement(RequirementType::Hlr, "Test HLR", None, RequirementStatus::Approved)
        .unwrap();
    let id = req.id.clone();
    let file = base.join("requirements/hlr").join(format!("{id}.md"));

    let result = engine.remove_requirement(&id, true, false).unwrap();

    assert!(!result.file_deleted, "file_deleted must be false for cache_only");
    assert!(file.exists(), "md file must remain when cache_only=true");
    assert!(
        engine.cache().get_requirement(&id).unwrap().is_none(),
        "cache entry must be purged even in cache_only mode"
    );
}

/// TC-0036-03: non-existent ID returns RequirementNotFound error
// VERIFIES: LLR-0035
#[test]
fn test_remove_nonexistent_id_returns_error() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);

    let err = engine.remove_requirement("LLR-9999", false, false).unwrap_err();
    assert!(
        matches!(err, Error::RequirementNotFound(_)),
        "must return RequirementNotFound for unknown ID, got: {err:?}"
    );
}

/// TC-0036-04: child requirement ID appears in dependents_warned
// VERIFIES: LLR-0035
#[test]
fn test_remove_with_child_warns_dependent() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);

    let hlr = engine
        .create_requirement(RequirementType::Hlr, "Parent HLR", None, RequirementStatus::Approved)
        .unwrap();
    let llr = engine
        .create_requirement(
            RequirementType::Llr,
            "Child LLR",
            Some(&hlr.id),
            RequirementStatus::Draft,
        )
        .unwrap();

    let result = engine.remove_requirement(&hlr.id, false, false).unwrap();
    assert!(
        result.dependents_warned.contains(&llr.id),
        "dependents_warned must list the child LLR ID"
    );
}

/// TC-0036-05: --strict with dependents is enforced at CLI level; engine still
///             removes but returns non-empty dependents_warned for the CLI to act on
// VERIFIES: LLR-0035
#[test]
fn test_remove_returns_dependents_for_strict_enforcement() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);

    let hlr = engine
        .create_requirement(RequirementType::Hlr, "Parent HLR", None, RequirementStatus::Approved)
        .unwrap();
    engine
        .create_requirement(
            RequirementType::Llr,
            "Child LLR",
            Some(&hlr.id),
            RequirementStatus::Draft,
        )
        .unwrap();

    let result = engine.remove_requirement(&hlr.id, false, false).unwrap();
    assert!(
        !result.dependents_warned.is_empty(),
        "engine must return dependents so CLI can enforce --strict"
    );
}

/// TC-0036-06: force=true bypasses dependent check and removes unconditionally
// VERIFIES: LLR-0035
#[test]
fn test_remove_force_bypasses_dependency_check() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);

    let hlr = engine
        .create_requirement(RequirementType::Hlr, "Parent HLR", None, RequirementStatus::Approved)
        .unwrap();
    engine
        .create_requirement(
            RequirementType::Llr,
            "Child LLR",
            Some(&hlr.id),
            RequirementStatus::Draft,
        )
        .unwrap();

    let result = engine.remove_requirement(&hlr.id, false, true).unwrap();
    assert!(
        result.dependents_warned.is_empty(),
        "force=true must skip dependent check, dependents_warned must be empty"
    );
    assert!(
        engine.cache().get_requirement(&hlr.id).unwrap().is_none(),
        "requirement must be removed from cache"
    );
}

/// TC-0036-07: import_sources row is also purged on removal
// VERIFIES: LLR-0035
#[test]
fn test_remove_purges_import_sources_row() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    let base = tmp.path();

    let req = engine
        .create_requirement(RequirementType::Hlr, "Imported HLR", None, RequirementStatus::Approved)
        .unwrap();

    let fake_src = base.join("source.json");
    fs::write(&fake_src, "[]").unwrap();
    engine
        .cache()
        .upsert_import_source(&req.id, &fake_src, "deadbeef")
        .unwrap();

    assert!(
        engine
            .cache()
            .get_all_import_sources()
            .unwrap()
            .iter()
            .any(|s| s.req_id == req.id),
        "import_sources must have a row before removal"
    );

    engine.remove_requirement(&req.id, false, false).unwrap();

    assert!(
        !engine
            .cache()
            .get_all_import_sources()
            .unwrap()
            .iter()
            .any(|s| s.req_id == req.id),
        "import_sources row must be purged on removal"
    );
}

//! Integration tests for AI suggestion import and provenance tracking.
// REQ: TST-0007
// VERIFIES: LLR-0012
// VERIFIES: LLR-0013

use req_engine::{Requirement, RequirementType, RequirementStatus};
use req_engine::ai_import::{AiSuggestion, AiSuggestions, ImportOptions, import_suggestions};
use req_engine::cache::Cache;

fn make_cache(dir: &std::path::Path) -> Cache {
    Cache::open(&dir.join("test.db"), 0).unwrap()
}

fn suggestions(items: Vec<AiSuggestion>) -> AiSuggestions {
    AiSuggestions { source: Some("test-model".to_string()), generated_at: None, suggestions: items }
}

/// TC-001: All AI-imported requirements are forced to draft status.
// VERIFIES: TST-0007 TC-001
#[test]
fn test_import_forces_draft_status() {
    let dir = tempfile::tempdir().unwrap();
    let cache = make_cache(dir.path());

    let batch = suggestions(vec![AiSuggestion {
        id: None,
        req_type: "hlr".to_string(),
        title: "AI-generated HLR".to_string(),
        text: "Some requirement text.".to_string(),
        parent: None,
        rationale: None,
    }]);

    let result = import_suggestions(batch, &cache, &ImportOptions::default()).unwrap();

    assert_eq!(result.imported.len(), 1);
    assert_eq!(result.imported[0].status, RequirementStatus::Draft);
}

/// TC-002: Provided IDs with invalid format are rejected.
// VERIFIES: TST-0007 TC-002
#[test]
fn test_import_rejects_invalid_id_format() {
    let dir = tempfile::tempdir().unwrap();
    let cache = make_cache(dir.path());

    let batch = suggestions(vec![AiSuggestion {
        id: Some("INVALID-FORMAT".to_string()),
        req_type: "hlr".to_string(),
        title: "Bad ID".to_string(),
        text: String::new(),
        parent: None,
        rationale: None,
    }]);

    let result = import_suggestions(batch, &cache, &ImportOptions::default()).unwrap();

    assert_eq!(result.imported.len(), 0);
    assert!(!result.errors.is_empty());
}

/// TC-003: LLR without parent is rejected.
// VERIFIES: TST-0007 TC-003
#[test]
fn test_import_rejects_llr_without_parent() {
    let dir = tempfile::tempdir().unwrap();
    let cache = make_cache(dir.path());

    let batch = suggestions(vec![AiSuggestion {
        id: None,
        req_type: "llr".to_string(),
        title: "Orphan LLR".to_string(),
        text: String::new(),
        parent: None,
        rationale: None,
    }]);

    let result = import_suggestions(batch, &cache, &ImportOptions::default()).unwrap();

    assert_eq!(result.imported.len(), 0);
    assert!(!result.skipped.is_empty() || !result.errors.is_empty());
}

/// TC-004: Duplicate IDs are rejected.
// VERIFIES: TST-0007 TC-004
#[test]
fn test_import_rejects_duplicate_id() {
    let dir = tempfile::tempdir().unwrap();
    let cache = make_cache(dir.path());

    // Pre-populate cache with an existing requirement
    let existing = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "Existing".to_string());
    cache.upsert_requirement(&existing).unwrap();

    let batch = suggestions(vec![AiSuggestion {
        id: Some("HLR-0001".to_string()),
        req_type: "hlr".to_string(),
        title: "Duplicate".to_string(),
        text: String::new(),
        parent: None,
        rationale: None,
    }]);

    let result = import_suggestions(batch, &cache, &ImportOptions::default()).unwrap();

    assert_eq!(result.imported.len(), 0);
    assert!(!result.skipped.is_empty() || !result.errors.is_empty());
}

/// TC-004b: LLR with valid parent is accepted.
// VERIFIES: TST-0007 TC-003
#[test]
fn test_import_accepts_llr_with_valid_parent() {
    let dir = tempfile::tempdir().unwrap();
    let cache = make_cache(dir.path());

    // Parent must exist
    let hlr = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "Parent".to_string());
    cache.upsert_requirement(&hlr).unwrap();

    let batch = suggestions(vec![AiSuggestion {
        id: None,
        req_type: "llr".to_string(),
        title: "Valid LLR".to_string(),
        text: "Some text.".to_string(),
        parent: Some("HLR-0001".to_string()),
        rationale: None,
    }]);

    let result = import_suggestions(batch, &cache, &ImportOptions::default()).unwrap();

    assert_eq!(result.imported.len(), 1);
    assert_eq!(result.imported[0].parent, Some("HLR-0001".to_string()));
}

/// TC-001 dry-run: dry_run=true does not write to cache.
// VERIFIES: TST-0007 TC-001
#[test]
fn test_import_dry_run_does_not_persist() {
    let dir = tempfile::tempdir().unwrap();
    let cache = make_cache(dir.path());

    let batch = suggestions(vec![AiSuggestion {
        id: None,
        req_type: "hlr".to_string(),
        title: "Dry-run HLR".to_string(),
        text: String::new(),
        parent: None,
        rationale: None,
    }]);

    let opts = ImportOptions { dry_run: true, ..Default::default() };
    let result = import_suggestions(batch, &cache, &opts).unwrap();

    // Result reports would-be imported, but nothing is in cache
    let all = cache.get_all_requirements().unwrap();
    assert!(all.is_empty(), "dry_run must not write to cache");
    // The result should report what would have been imported
    assert_eq!(result.imported.len(), 1);
}

// ── TST-0007 TC-006: Provenance tracking ─────────────────────────────────────

use req_engine::provenance;

/// TC-006a: lock_requirements makes all .md files read-only and creates .locked marker.
// VERIFIES: TST-0007 TC-006
#[test]
fn test_lock_requirements() {
    let dir = tempfile::tempdir().unwrap();
    let req_dir = dir.path().join("requirements").join("hlr");
    std::fs::create_dir_all(&req_dir).unwrap();
    std::fs::create_dir_all(dir.path().join(".req")).unwrap();
    std::fs::write(req_dir.join("HLR-0001.md"), "---\nid: HLR-0001\n---\n").unwrap();

    let count = provenance::lock_requirements(dir.path()).unwrap();
    assert_eq!(count, 1, "should have locked 1 file");
    assert!(
        provenance::is_locked(dir.path()),
        ".locked marker should exist after lock"
    );
}

/// TC-006b: unlock_requirements restores write access and removes .locked marker.
// VERIFIES: TST-0007 TC-006
#[test]
fn test_unlock_requirements() {
    let dir = tempfile::tempdir().unwrap();
    let req_dir = dir.path().join("requirements").join("hlr");
    std::fs::create_dir_all(&req_dir).unwrap();
    std::fs::create_dir_all(dir.path().join(".req")).unwrap();
    std::fs::write(req_dir.join("HLR-0001.md"), "---\nid: HLR-0001\n---\n").unwrap();

    provenance::lock_requirements(dir.path()).unwrap();
    assert!(provenance::is_locked(dir.path()));

    let count = provenance::unlock_requirements(dir.path()).unwrap();
    assert_eq!(count, 1, "should have unlocked 1 file");
    assert!(
        !provenance::is_locked(dir.path()),
        ".locked marker should be gone after unlock"
    );
}

/// TC-006c: verify_provenance returns file_exists=false for a missing requirement.
// VERIFIES: TST-0007 TC-006
#[test]
fn test_verify_provenance_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("requirements").join("hlr")).unwrap();
    std::fs::create_dir_all(dir.path().join(".req")).unwrap();

    let status = provenance::verify_provenance(dir.path(), "HLR-0099", None).unwrap();
    assert!(!status.file_exists, "non-existent requirement should show file_exists=false");
}

/// TC-006d: verify_provenance detects hash match and tool_version presence.
// VERIFIES: TST-0007 TC-006
#[test]
fn test_verify_provenance_hash_match() {
    let dir = tempfile::tempdir().unwrap();
    let req_dir = dir.path().join("requirements").join("hlr");
    std::fs::create_dir_all(&req_dir).unwrap();
    std::fs::create_dir_all(dir.path().join(".req")).unwrap();

    let content = "---\nid: HLR-0001\ntool_version: 0.1.0\n---\n# Title\n";
    std::fs::write(req_dir.join("HLR-0001.md"), content).unwrap();

    // No cached hash → hash_matches defaults to true
    let status = provenance::verify_provenance(dir.path(), "HLR-0001", None).unwrap();
    assert!(status.file_exists);
    assert!(status.hash_matches);
    assert!(status.has_tool_version);

    // Verify with the real current hash → still matches
    let real_hash = status.current_hash.as_deref().unwrap();
    let status2 = provenance::verify_provenance(dir.path(), "HLR-0001", Some(real_hash)).unwrap();
    assert!(status2.hash_matches);
    assert!(status2.is_tool_created());
}

/// TC-006e: check_all_provenance detects a requirement lacking tool_version.
// VERIFIES: TST-0007 TC-006
#[test]
fn test_check_all_provenance_detects_manual_edit() {
    let dir = tempfile::tempdir().unwrap();
    let req_dir = dir.path().join("requirements").join("hlr");
    std::fs::create_dir_all(&req_dir).unwrap();
    std::fs::create_dir_all(dir.path().join(".req")).unwrap();

    // Write a requirement without tool_version (simulates manual creation)
    std::fs::write(
        req_dir.join("HLR-0001.md"),
        "---\nid: HLR-0001\ntype: hlr\ntitle: Manual\nstatus: draft\n---\n",
    )
    .unwrap();

    let cache = req_engine::cache::Cache::open(&dir.path().join(".req").join("cache.db"), 0).unwrap();
    let req = Requirement::new(
        "HLR-0001".to_string(),
        RequirementType::Hlr,
        "Manual".to_string(),
    );
    cache.upsert_requirement(&req).unwrap();

    let violations = provenance::check_all_provenance(dir.path(), &cache).unwrap();
    assert!(
        violations.iter().any(|v| v.req_id == "HLR-0001"),
        "manually-created requirement should be a provenance violation"
    );
}

//! Integration tests for import source staleness detection.
// VERIFIES: LLR-0037

use req_engine::{ReqEngine, Requirement, RequirementStatus, RequirementType};
use std::fs;
use tempfile::TempDir;

fn setup(tmp: &TempDir) -> ReqEngine {
    let base = tmp.path();
    ReqEngine::init(base, Some("test")).unwrap();
    ReqEngine::open(base).unwrap()
}

fn write_json_import(path: &std::path::Path, reqs: &[Requirement]) {
    fs::write(path, serde_json::to_string(reqs).unwrap()).unwrap();
}

/// TC-0038-01: import populates import_sources with correct path and sha256
// VERIFIES: LLR-0037
#[test]
fn test_import_populates_import_sources() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    let base = tmp.path();

    let req = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "Imported".to_string());
    let json_path = base.join("import.json");
    write_json_import(&json_path, &[req]);

    engine.import(&json_path, "json", None).unwrap();

    let sources = engine.cache().get_all_import_sources().unwrap();
    let src = sources.iter().find(|s| s.req_id == "HLR-0001");
    assert!(src.is_some(), "import_sources must contain a row for HLR-0001");
    let src = src.unwrap();
    assert!(!src.sha256.is_empty(), "sha256 must be non-empty");
}

/// TC-0038-02: unchanged source stays clean (import_status NULL) after scan
// VERIFIES: LLR-0037
#[test]
fn test_unchanged_source_stays_clean_after_scan() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    let base = tmp.path();

    let req = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "Imported".to_string());
    let json_path = base.join("import.json");
    write_json_import(&json_path, &[req]);
    engine.import(&json_path, "json", None).unwrap();

    engine.scan(None, false).unwrap();

    let flagged = engine.cache().get_flagged_imports().unwrap();
    assert!(
        !flagged.iter().any(|(id, _)| id == "HLR-0001"),
        "unchanged source must not appear in flagged imports"
    );
}

/// TC-0038-03: modified source triggers import_status = 'stale'
// VERIFIES: LLR-0037
#[test]
fn test_modified_source_triggers_stale() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    let base = tmp.path();

    let req = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "Imported".to_string());
    let json_path = base.join("import.json");
    let content = serde_json::to_string(&[&req]).unwrap();
    fs::write(&json_path, &content).unwrap();
    engine.import(&json_path, "json", None).unwrap();

    // Modify the file so SHA-256 changes
    fs::write(&json_path, format!("{content} ")).unwrap();
    engine.scan(None, false).unwrap();

    let flagged = engine.cache().get_flagged_imports().unwrap();
    let status = flagged.iter().find(|(id, _)| id == "HLR-0001").map(|(_, s)| s.as_str());
    assert_eq!(status, Some("stale"), "modified source must set import_status = stale");
}

/// TC-0038-04: deleted source triggers import_status = 'orphaned'
// VERIFIES: LLR-0037
#[test]
fn test_deleted_source_triggers_orphaned() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    let base = tmp.path();

    let req = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "Imported".to_string());
    let json_path = base.join("import.json");
    write_json_import(&json_path, &[req]);
    engine.import(&json_path, "json", None).unwrap();

    fs::remove_file(&json_path).unwrap();
    engine.scan(None, false).unwrap();

    let flagged = engine.cache().get_flagged_imports().unwrap();
    let status = flagged.iter().find(|(id, _)| id == "HLR-0001").map(|(_, s)| s.as_str());
    assert_eq!(status, Some("orphaned"), "deleted source must set import_status = orphaned");
}

/// TC-0038-05: re-import of updated file clears the stale flag
// VERIFIES: LLR-0037
#[test]
fn test_reimport_clears_stale_flag() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    let base = tmp.path();

    let req = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "Imported".to_string());
    let json_path = base.join("import.json");
    let v1 = serde_json::to_string(&[&req]).unwrap();
    fs::write(&json_path, &v1).unwrap();
    engine.import(&json_path, "json", None).unwrap();

    // Make stale
    let v2 = format!("{v1} ");
    fs::write(&json_path, &v2).unwrap();
    engine.scan(None, false).unwrap();

    // Re-import with updated file
    engine.import(&json_path, "json", None).unwrap();
    // scan picks up the new hash and clears stale
    engine.scan(None, false).unwrap();

    let flagged = engine.cache().get_flagged_imports().unwrap();
    assert!(
        !flagged.iter().any(|(id, _)| id == "HLR-0001"),
        "re-import must clear the stale flag"
    );
}

/// TC-0038-06: req remove purges the import_sources row
// VERIFIES: LLR-0037
#[test]
fn test_remove_purges_import_sources_row() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    let base = tmp.path();

    let req = engine
        .create_requirement(RequirementType::Hlr, "Imported HLR", None, RequirementStatus::Approved)
        .unwrap();

    let json_path = base.join("source.json");
    fs::write(&json_path, "[]").unwrap();
    engine.cache().upsert_import_source(&req.id, &json_path, "abc").unwrap();

    engine.remove_requirement(&req.id, false, false).unwrap();

    assert!(
        !engine.cache().get_all_import_sources().unwrap().iter().any(|s| s.req_id == req.id),
        "import_sources row must be deleted on requirement removal"
    );
}

/// TC-0038-07: gaps_full surfaces stale/orphaned requirements
// VERIFIES: LLR-0037
#[test]
fn test_gaps_full_surfaces_stale_and_orphaned() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);

    for (id, status) in [("HLR-0001", "stale"), ("HLR-0002", "orphaned")] {
        engine.cache().upsert_requirement(&Requirement::new(
            id.to_string(), RequirementType::Hlr, format!("T {id}"),
        )).unwrap();
        engine.cache().set_import_status(id, Some(status)).unwrap();
    }

    let gaps = engine.gaps_full().unwrap();
    assert!(gaps.import_stale.contains(&"HLR-0001".to_string()), "gaps_full must include stale");
    assert!(gaps.import_orphaned.contains(&"HLR-0002".to_string()), "gaps_full must include orphaned");
}

/// TC-0038-08: validate includes error issues for stale and orphaned requirements
// VERIFIES: LLR-0037
#[test]
fn test_validate_reports_stale_and_orphaned_as_errors() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);

    engine.cache().upsert_requirement(&Requirement::new(
        "HLR-0001".to_string(), RequirementType::Hlr, "Stale".to_string(),
    )).unwrap();
    engine.cache().set_import_status("HLR-0001", Some("stale")).unwrap();

    engine.cache().upsert_requirement(&Requirement::new(
        "HLR-0002".to_string(), RequirementType::Hlr, "Orphaned".to_string(),
    )).unwrap();
    engine.cache().set_import_status("HLR-0002", Some("orphaned")).unwrap();

    let issues = engine.validate().unwrap();
    let msgs: Vec<&str> = issues.iter().map(|i| i.message.as_str()).collect();

    assert!(
        msgs.iter().any(|m| m.contains("import_stale")),
        "validate must report import_stale error"
    );
    assert!(
        msgs.iter().any(|m| m.contains("import_orphaned")),
        "validate must report import_orphaned error"
    );
}

/// TC-0038-09: import_ai with source_path populates import_sources
// VERIFIES: LLR-0037
#[test]
fn test_import_ai_populates_import_sources() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    let base = tmp.path();

    // Seed a parent HLR first
    engine.cache().upsert_requirement(&Requirement::new(
        "HLR-0001".to_string(), RequirementType::Hlr, "Parent".to_string(),
    )).unwrap();

    let ai_json = serde_json::json!({
        "source": "test-model",
        "suggestions": [{
            "type": "llr",
            "title": "AI suggested LLR",
            "text": "Do something",
            "parent": "HLR-0001"
        }]
    });
    let ai_path = base.join("ai_suggestions.json");
    fs::write(&ai_path, ai_json.to_string()).unwrap();

    let suggestions: req_engine::ai_import::AiSuggestions =
        serde_json::from_str(&ai_json.to_string()).unwrap();

    let options = req_engine::ai_import::ImportOptions {
        dry_run: false,
        provenance: Some("test-model".to_string()),
        source_path: Some(ai_path.clone()),
    };

    let result = engine.import_ai(suggestions, options).unwrap();
    assert!(!result.imported.is_empty(), "AI import must produce at least one requirement");

    let sources = engine.cache().get_all_import_sources().unwrap();
    for req in &result.imported {
        assert!(
            sources.iter().any(|s| s.req_id == req.id),
            "import_sources must have a row for AI-imported requirement {}", req.id
        );
    }
}

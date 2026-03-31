//! Tests for the SQLite Cache Layer.
// REQ: TST-0004
// VERIFIES: LLR-0004

use req_engine::cache::Cache;
use req_engine::{CodeRef, Link, LinkType, Requirement, RequirementStatus, RequirementType};
use std::path::PathBuf;

/// Test that cache creates proper schema with all required tables
// VERIFIES: LLR-0004
#[test]
fn test_schema_creation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache_path = temp_dir.path().join("test.db");
    
    let cache = Cache::open(&cache_path, 0).unwrap();
    
    // Verify database file was created
    assert!(cache_path.exists());
    
    // Verify we can perform operations (tables exist)
    let req = Requirement::new("HLR-TEST".to_string(), RequirementType::Hlr, "Test".to_string());
    cache.upsert_requirement(&req).unwrap();
    
    let retrieved = cache.get_requirement("HLR-TEST").unwrap();
    assert!(retrieved.is_some());
}

/// Test upsert operation for requirements (insert new)
// VERIFIES: LLR-0004
#[test]
fn test_upsert_requirement_insert() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache_path = temp_dir.path().join("test.db");
    let cache = Cache::open(&cache_path, 0).unwrap();

    let req = Requirement {
        id: "LLR-0001".to_string(),
        req_type: RequirementType::Llr,
        title: "Test Requirement".to_string(),
        text: "This is a test requirement.".to_string(),
        status: RequirementStatus::Approved,
        parent: Some("HLR-0001".to_string()),
        aliases: vec!["alias-1".to_string()],
        attributes: {
            let mut attrs = std::collections::HashMap::new();
            attrs.insert("priority".to_string(), "high".to_string());
            attrs
        },
        source_file: Some(PathBuf::from("test.md")),
        created: chrono::Utc::now(),
        modified: chrono::Utc::now(),
    };

    cache.upsert_requirement(&req).unwrap();

    let retrieved = cache.get_requirement("LLR-0001").unwrap();
    assert!(retrieved.is_some());
    
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, "LLR-0001");
    assert_eq!(retrieved.title, "Test Requirement");
    assert_eq!(retrieved.parent, Some("HLR-0001".to_string()));
}

/// Test upsert operation for requirements (update existing)
// VERIFIES: LLR-0004
#[test]
fn test_upsert_requirement_update() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache_path = temp_dir.path().join("test.db");
    let cache = Cache::open(&cache_path, 0).unwrap();

    // Insert initial requirement
    let mut req = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "Original Title".to_string());
    req.status = RequirementStatus::Draft;
    cache.upsert_requirement(&req).unwrap();

    // Update the requirement
    req.title = "Updated Title".to_string();
    req.status = RequirementStatus::Approved;
    cache.upsert_requirement(&req).unwrap();

    // Verify update
    let retrieved = cache.get_requirement("HLR-0001").unwrap().unwrap();
    assert_eq!(retrieved.title, "Updated Title");
    assert_eq!(retrieved.status, RequirementStatus::Approved);
}

/// Test file hash tracking for incremental scans
// VERIFIES: LLR-0004
#[test]
fn test_file_hash_tracking() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache_path = temp_dir.path().join("test.db");
    let cache = Cache::open(&cache_path, 0).unwrap();

    let file_path = temp_dir.path().join("test.rs");
    std::fs::write(&file_path, "fn main() {}").unwrap();

    let hash = "abc123";
    
    // Store hash
    cache.update_file_hash(&file_path, hash).unwrap();
    
    // Retrieve hash
    let retrieved = cache.get_file_hash(&file_path).unwrap();
    assert_eq!(retrieved, Some(hash.to_string()));
    
    // Check if file has changed
    assert!(!cache.file_has_changed(&file_path, hash).unwrap());
    assert!(cache.file_has_changed(&file_path, "different_hash").unwrap());
}

/// Test code reference operations
// VERIFIES: LLR-0004
#[test]
fn test_code_ref_operations() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache_path = temp_dir.path().join("test.db");
    let cache = Cache::open(&cache_path, 0).unwrap();

    // First create a requirement (needed for FK constraint)
    let req = Requirement::new("LLR-0001".to_string(), RequirementType::Llr, "Test".to_string());
    cache.upsert_requirement(&req).unwrap();

    // Insert code reference
    let code_ref = CodeRef {
        req_id: "LLR-0001".to_string(),
        file: PathBuf::from("src/main.rs"),
        line: 10,
        line_end: Some(20),
        hash: Some("hash123".to_string()),
        symbol: Some("main".to_string()),
    };
    
    cache.insert_code_ref(&code_ref).unwrap();
    
    // Retrieve code refs
    let refs = cache.get_code_refs_for_requirement("LLR-0001").unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].file, PathBuf::from("src/main.rs"));
    assert_eq!(refs[0].line, 10);
}

/// Test link operations
// VERIFIES: LLR-0004
#[test]
fn test_link_operations() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache_path = temp_dir.path().join("test.db");
    let cache = Cache::open(&cache_path, 0).unwrap();

    // Create requirements first
    let hlr = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "HLR".to_string());
    cache.upsert_requirement(&hlr).unwrap();
    
    let llr = Requirement::new("LLR-0001".to_string(), RequirementType::Llr, "LLR".to_string());
    cache.upsert_requirement(&llr).unwrap();

    // Insert link
    let link = Link {
        source: "HLR-0001".to_string(),
        target: "LLR-0001".to_string(),
        link_type: LinkType::References,
    };
    
    cache.insert_link(&link).unwrap();
    
    // Retrieve all links
    let links = cache.get_all_links().unwrap();
    assert!(!links.is_empty());
    
    let found = links.iter().find(|l| 
        l.source == "HLR-0001" && 
        l.target == "LLR-0001" && 
        l.link_type == LinkType::References
    );
    assert!(found.is_some());
}

/// Test get all requirements
// VERIFIES: LLR-0004
#[test]
fn test_get_all_requirements() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache_path = temp_dir.path().join("test.db");
    let cache = Cache::open(&cache_path, 0).unwrap();

    // Insert multiple requirements
    for i in 1..=3 {
        let req = Requirement::new(
            format!("HLR-000{}", i),
            RequirementType::Hlr,
            format!("Requirement {}", i),
        );
        cache.upsert_requirement(&req).unwrap();
    }

    let all_reqs = cache.get_all_requirements().unwrap();
    assert_eq!(all_reqs.len(), 3);
}

/// Test get requirements by type
// VERIFIES: LLR-0004
#[test]
fn test_get_requirements_by_type() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache_path = temp_dir.path().join("test.db");
    let cache = Cache::open(&cache_path, 0).unwrap();

    // Insert HLR and LLR
    let hlr = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "HLR".to_string());
    cache.upsert_requirement(&hlr).unwrap();
    
    let llr = Requirement::new("LLR-0001".to_string(), RequirementType::Llr, "LLR".to_string());
    cache.upsert_requirement(&llr).unwrap();

    let hlr_reqs = cache.get_requirements_by_type(RequirementType::Hlr).unwrap();
    assert_eq!(hlr_reqs.len(), 1);
    assert_eq!(hlr_reqs[0].id, "HLR-0001");

    let llr_reqs = cache.get_requirements_by_type(RequirementType::Llr).unwrap();
    assert_eq!(llr_reqs.len(), 1);
    assert_eq!(llr_reqs[0].id, "LLR-0001");
}

/// Test requirement exists check
// VERIFIES: LLR-0004
#[test]
fn test_requirement_exists() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache_path = temp_dir.path().join("test.db");
    let cache = Cache::open(&cache_path, 0).unwrap();

    let req = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "Test".to_string());
    cache.upsert_requirement(&req).unwrap();

    assert!(cache.requirement_exists("HLR-0001").unwrap());
    assert!(!cache.requirement_exists("NONEXISTENT").unwrap());
}

/// Test clear cache
// VERIFIES: LLR-0004
#[test]
fn test_clear_cache() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache_path = temp_dir.path().join("test.db");
    let cache = Cache::open(&cache_path, 0).unwrap();

    // Insert data
    let req = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "Test".to_string());
    cache.upsert_requirement(&req).unwrap();

    // Clear
    cache.clear().unwrap();

    // Verify empty
    let all_reqs = cache.get_all_requirements().unwrap();
    assert!(all_reqs.is_empty());
}

/// Test coverage calculation
// VERIFIES: LLR-0004
#[test]
fn test_calculate_coverage() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache_path = temp_dir.path().join("test.db");
    let cache = Cache::open(&cache_path, 0).unwrap();

    // Create HLR with LLR child
    let hlr = Requirement::new("HLR-0001".to_string(), RequirementType::Hlr, "HLR".to_string());
    cache.upsert_requirement(&hlr).unwrap();
    
    let mut llr = Requirement::new("LLR-0001".to_string(), RequirementType::Llr, "LLR".to_string());
    llr.parent = Some("HLR-0001".to_string());
    cache.upsert_requirement(&llr).unwrap();

    // Add code reference
    let code_ref = CodeRef {
        req_id: "LLR-0001".to_string(),
        file: PathBuf::from("src/main.rs"),
        line: 1,
        line_end: None,
        hash: None,
        symbol: None,
    };
    cache.insert_code_ref(&code_ref).unwrap();

    let coverage = cache.calculate_coverage().unwrap();
    
    assert_eq!(coverage.hlr_total, 1);
    assert_eq!(coverage.hlr_with_llr, 1);
    assert_eq!(coverage.llr_total, 1);
    assert_eq!(coverage.llr_implemented, 1);
}
//! Integration tests for the data models.
// REQ: TST-0001
// VERIFIES: LLR-0001

use req_engine::{Requirement, RequirementType, RequirementStatus};

/// Test requirement struct fields exist
/// Verifies: TST-0001 TC-001
#[test]
fn test_requirement_struct_fields() {
    let mut req = Requirement::new(
        "LLR-0001".to_string(),
        RequirementType::Llr,
        "Test Requirement".to_string(),
    );
    
    // Verify all fields exist and can be set
    assert_eq!(req.id, "LLR-0001");
    assert_eq!(req.req_type, RequirementType::Llr);
    assert_eq!(req.title, "Test Requirement");
    assert_eq!(req.status, RequirementStatus::Draft); // Default status
    assert!(req.parent.is_none());
    assert!(req.aliases.is_empty());
    assert!(req.attributes.is_empty());
    
    // Set optional fields
    req.parent = Some("HLR-0001".to_string());
    req.aliases.push("reqif-uuid-123".to_string());
    req.attributes.insert("priority".to_string(), "high".to_string());
    
    assert_eq!(req.parent, Some("HLR-0001".to_string()));
    assert_eq!(req.aliases.len(), 1);
    assert_eq!(req.attributes.get("priority"), Some(&"high".to_string()));
}

/// Test ID format validation
/// Verifies: TST-0001 TC-002
#[test]
fn test_id_format_validation() {
    // Valid IDs
    assert!(Requirement::parse_id("HLR-0001").is_some());
    assert!(Requirement::parse_id("LLR-0123").is_some());
    assert!(Requirement::parse_id("TST-0035").is_some());
    
    // Invalid IDs
    assert!(Requirement::parse_id("invalid").is_none());
    assert!(Requirement::parse_id("XYZ-001").is_none());
    assert!(Requirement::parse_id("HLR-ABC").is_none());
    assert!(Requirement::parse_id("HLR").is_none());
    assert!(Requirement::parse_id("").is_none());
}

/// Test auto-generation of sequential IDs
/// Verifies: TST-0001 TC-003
#[test]
fn test_id_generation() {
    // Empty list - first ID
    let empty: Vec<String> = Vec::new();
    let id1 = Requirement::generate_id(RequirementType::Hlr, &empty);
    assert_eq!(id1, "HLR-0001");
    
    // With existing IDs - next in sequence
    let existing = vec!["HLR-0001".to_string(), "HLR-0002".to_string()];
    let id2 = Requirement::generate_id(RequirementType::Hlr, &existing);
    assert_eq!(id2, "HLR-0003");
    
    // Different types have separate sequences
    let llr_existing = vec!["LLR-0001".to_string(), "LLR-0002".to_string()];
    let id3 = Requirement::generate_id(RequirementType::Llr, &llr_existing);
    assert_eq!(id3, "LLR-0003");
    
    // TST type
    let tst_id = Requirement::generate_id(RequirementType::Tst, &empty);
    assert_eq!(tst_id, "TST-0001");
}

/// Test requirement type conversion
#[test]
fn test_requirement_type_conversion() {
    assert_eq!(RequirementType::from_str("hlr"), Some(RequirementType::Hlr));
    assert_eq!(RequirementType::from_str("llr"), Some(RequirementType::Llr));
    assert_eq!(RequirementType::from_str("tst"), Some(RequirementType::Tst));
    assert_eq!(RequirementType::from_str("test"), Some(RequirementType::Tst));
    assert_eq!(RequirementType::from_str("invalid"), None);
    
    // As str
    assert_eq!(RequirementType::Hlr.as_str(), "hlr");
    assert_eq!(RequirementType::Llr.as_str(), "llr");
    assert_eq!(RequirementType::Tst.as_str(), "tst");
}

/// Test requirement status conversion
#[test]
fn test_requirement_status_conversion() {
    assert_eq!(RequirementStatus::from_str("draft"), Some(RequirementStatus::Draft));
    assert_eq!(RequirementStatus::from_str("approved"), Some(RequirementStatus::Approved));
    assert_eq!(RequirementStatus::from_str("deprecated"), Some(RequirementStatus::Deprecated));
    assert_eq!(RequirementStatus::from_str("rejected"), Some(RequirementStatus::Rejected));
    assert_eq!(RequirementStatus::from_str("invalid"), None);
    
    // Default
    let default_status = RequirementStatus::default();
    assert_eq!(default_status, RequirementStatus::Approved);
}
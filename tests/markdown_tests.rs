//! Tests for the Markdown Adapter.
// REQ: TST-0003
// VERIFIES: LLR-0003

use req_engine::adapter::markdown::MarkdownAdapter;
use req_engine::adapter::RequirementAdapter;
use req_engine::{Requirement, RequirementStatus, RequirementType};

/// Test that parser extracts YAML frontmatter and body text
// VERIFIES: LLR-0003
#[test]
fn test_parse_frontmatter_and_body() {
    let content = r#"---
id: LLR-0001
type: llr
title: "ADC Sampling Rate"
status: approved
parent: HLR-0001
---

# ADC Sampling Rate

The ADC shall sample at 10 kHz ±1%.
"#;
    let adapter = MarkdownAdapter::new();
    let req = adapter.parse_content(content, None).unwrap();

    assert_eq!(req.id, "LLR-0001");
    assert_eq!(req.req_type, RequirementType::Llr);
    assert_eq!(req.title, "ADC Sampling Rate");
    assert_eq!(req.status, RequirementStatus::Approved);
    assert_eq!(req.parent, Some("HLR-0001".to_string()));
    assert!(req.text.contains("10 kHz"));
}

/// Test that parser handles different requirement types
// VERIFIES: LLR-0003
#[test]
fn test_parse_different_types() {
    let hlr_content = r#"---
id: HLR-0001
type: hlr
title: "High Level Requirement"
status: draft
---

HLR content here.
"#;
    let adapter = MarkdownAdapter::new();
    let req = adapter.parse_content(hlr_content, None).unwrap();
    
    assert_eq!(req.req_type, RequirementType::Hlr);
    assert_eq!(req.status, RequirementStatus::Draft);
}

/// Test that parser handles aliases and attributes
// VERIFIES: LLR-0003
#[test]
fn test_parse_aliases_and_attributes() {
    let content = r#"---
id: LLR-0002
type: llr
title: "Test Requirement"
status: approved
aliases:
  - reqif-uuid-12345
  - old-id-LLR-99
attributes:
  priority: high
  safety: ASIL-B
---

Requirement text.
"#;
    let adapter = MarkdownAdapter::new();
    let req = adapter.parse_content(content, None).unwrap();

    assert_eq!(req.aliases.len(), 2);
    assert!(req.aliases.contains(&"reqif-uuid-12345".to_string()));
    assert_eq!(req.attributes.get("priority"), Some(&"high".to_string()));
    assert_eq!(req.attributes.get("safety"), Some(&"ASIL-B".to_string()));
}

/// Test that parser rejects missing frontmatter
// VERIFIES: LLR-0003
#[test]
fn test_parse_missing_frontmatter() {
    let content = r#"# No Frontmatter

This has no YAML frontmatter.
"#;
    let adapter = MarkdownAdapter::new();
    let result = adapter.parse_content(content, None);
    
    assert!(result.is_err());
}

/// Test that parser rejects invalid YAML
// VERIFIES: LLR-0003
#[test]
fn test_parse_invalid_yaml() {
    let content = r#"---
id: LLR-0001
type: llr
title: [invalid yaml
---

Content here.
"#;
    let adapter = MarkdownAdapter::new();
    let result = adapter.parse_content(content, None);
    
    assert!(result.is_err());
}

/// Test that writer generates valid markdown
// VERIFIES: LLR-0003
#[test]
fn test_write_requirement() {
    let temp_dir = tempfile::tempdir().unwrap();
    let adapter = MarkdownAdapter::new();
    
    let req = Requirement {
        id: "HLR-TEST".to_string(),
        req_type: RequirementType::Hlr,
        title: "Test Title".to_string(),
        text: "Test requirement text.".to_string(),
        status: RequirementStatus::Approved,
        parent: None,
        aliases: vec![],
        attributes: std::collections::HashMap::new(),
        source_file: None,
        created: chrono::Utc::now(),
        modified: chrono::Utc::now(),
    };

    adapter.write(&[req.clone()], temp_dir.path()).unwrap();

    // Verify file was created
    let file_path = temp_dir.path().join("requirements/hlr/HLR-TEST.md");
    assert!(file_path.exists());

    // Read and verify content
    let content = std::fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("id: HLR-TEST"));
    assert!(content.contains("type: hlr"));
    assert!(content.contains("Test requirement text"));
}

/// Test roundtrip: write then read
// VERIFIES: LLR-0003
#[test]
fn test_roundtrip() {
    let temp_dir = tempfile::tempdir().unwrap();
    let adapter = MarkdownAdapter::new();
    
    let original = Requirement {
        id: "LLR-ROUNDTRIP".to_string(),
        req_type: RequirementType::Llr,
        title: "Roundtrip Test".to_string(),
        text: "This tests roundtrip serialization.".to_string(),
        status: RequirementStatus::Approved,
        parent: Some("HLR-001".to_string()),
        aliases: vec!["alias-1".to_string()],
        attributes: {
            let mut attrs = std::collections::HashMap::new();
            attrs.insert("key".to_string(), "value".to_string());
            attrs
        },
        source_file: None,
        created: chrono::Utc::now(),
        modified: chrono::Utc::now(),
    };

    // Write
    adapter.write(&[original.clone()], temp_dir.path()).unwrap();

    // Read back
    let file_path = temp_dir.path().join("requirements/llr/LLR-ROUNDTRIP.md");
    let read_reqs = adapter.read(&file_path).unwrap();
    
    assert_eq!(read_reqs.len(), 1);
    let read_req = &read_reqs[0];
    
    assert_eq!(read_req.id, original.id);
    assert_eq!(read_req.req_type, original.req_type);
    assert_eq!(read_req.title, original.title);
    assert_eq!(read_req.parent, original.parent);
}

/// Test directory scanning
// VERIFIES: LLR-0003
#[test]
fn test_directory_scanning() {
    let temp_dir = tempfile::tempdir().unwrap();
    let adapter = MarkdownAdapter::new();
    
    // Create multiple requirement files
    let req1 = Requirement {
        id: "HLR-001".to_string(),
        req_type: RequirementType::Hlr,
        title: "First".to_string(),
        text: "First requirement".to_string(),
        status: RequirementStatus::Approved,
        parent: None,
        aliases: vec![],
        attributes: std::collections::HashMap::new(),
        source_file: None,
        created: chrono::Utc::now(),
        modified: chrono::Utc::now(),
    };

    let req2 = Requirement {
        id: "LLR-001".to_string(),
        req_type: RequirementType::Llr,
        title: "Second".to_string(),
        text: "Second requirement".to_string(),
        status: RequirementStatus::Draft,
        parent: Some("HLR-001".to_string()),
        aliases: vec![],
        attributes: std::collections::HashMap::new(),
        source_file: None,
        created: chrono::Utc::now(),
        modified: chrono::Utc::now(),
    };

    adapter.write(&[req1, req2], temp_dir.path()).unwrap();

    // Scan directory
    let requirements = adapter.read(temp_dir.path()).unwrap();
    
    assert!(requirements.len() >= 2);
}

/// Test can_handle detection
// VERIFIES: LLR-0003
#[test]
fn test_can_handle() {
    let temp_dir = tempfile::tempdir().unwrap();
    let adapter = MarkdownAdapter::new();
    
    // Create a .md file
    let md_file = temp_dir.path().join("test.md");
    std::fs::write(&md_file, "---\nid: TEST-001\ntype: hlr\n---\n\nContent").unwrap();
    
    // Create non-md files
    let txt_file = temp_dir.path().join("test.txt");
    std::fs::write(&txt_file, "content").unwrap();
    
    // Should handle .md files that exist
    assert!(adapter.can_handle(&md_file));
    
    // Should not handle other files
    assert!(!adapter.can_handle(&txt_file));
}

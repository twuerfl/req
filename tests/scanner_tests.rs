//! Integration tests for the code scanner.
// REQ: TST-0002
// VERIFIES: LLR-0002
//!
//! Test requirement: TST-0035
//! VERIFIES: LLR-0002

use req_engine::scanner::{CodeScanner, TagType, validate_req_id};
use std::io::Write;
use tempfile::NamedTempFile;

/// Test single-line REQ tags are parsed correctly
/// Verifies: LLR-0002
#[test]
fn test_scan_req_tags() {
    let scanner = CodeScanner::new();
    let code = r#"
// REQ: LLR-0001
void adc_init() {
    // implementation
}

/* REQ: LLR-0002, LLR-0003 */
void motor_control() {
}

// VERIFIES: TST-0035
void test_adc() {
}
"#;
    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(code.as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let tags = scanner.scan_file(temp_file.path()).unwrap();

    assert_eq!(tags.len(), 4);
    assert!(tags.iter().any(|t| t.req_id == "LLR-0001" && t.tag_type == TagType::Req));
    assert!(tags.iter().any(|t| t.req_id == "LLR-0002" && t.tag_type == TagType::Req));
    assert!(tags.iter().any(|t| t.req_id == "LLR-0003" && t.tag_type == TagType::Req));
    assert!(tags.iter().any(|t| t.req_id == "TST-0035" && t.tag_type == TagType::Verifies));
}

/// Test requirement ID validation
/// Verifies: LLR-0002
#[test]
fn test_validate_req_id() {
    assert!(validate_req_id("HLR-0001").is_ok());
    assert!(validate_req_id("LLR-0123").is_ok());
    assert!(validate_req_id("TST-0035").is_ok());
    assert!(validate_req_id("invalid").is_err());
    assert!(validate_req_id("XYZ-001").is_err());
}

/// Test BEGIN-REQ/END-REQ block tags
/// Verifies: Block tag feature
#[test]
fn test_scan_block_tags() {
    let scanner = CodeScanner::new();
    let code = r#"// BEGIN-REQ: LLR-0016
fn import_requirement() {
    // parse file
    // validate id
    // store in cache
}
// END-REQ: LLR-0016

// BEGIN-REQ: LLR-0017
fn export_requirement() {
    // generate output
}
// END-REQ: LLR-0017
"#;
    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(code.as_bytes()).unwrap();
    temp_file.flush().unwrap();
    
    let tags = scanner.scan_file(temp_file.path()).unwrap();

    assert_eq!(tags.len(), 2);
    
    // Check first block
    let llr_16 = tags.iter().find(|t| t.req_id == "LLR-0016").unwrap();
    assert_eq!(llr_16.line, 1);
    assert_eq!(llr_16.line_end, Some(7));
    
    // Check second block
    let llr_17 = tags.iter().find(|t| t.req_id == "LLR-0017").unwrap();
    assert_eq!(llr_17.line, 9);
    assert_eq!(llr_17.line_end, Some(13));
}

/// Test mixed single-line and block tags
/// Verifies: Block tag feature
#[test]
fn test_mixed_tags() {
    let scanner = CodeScanner::new();
    let code = r#"
// REQ: LLR-0001
void single_line() {}

// BEGIN-REQ: LLR-0002
void block_impl() {
    // multiple lines
}
// END-REQ: LLR-0002
"#;
    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(code.as_bytes()).unwrap();
    temp_file.flush().unwrap();
    
    let tags = scanner.scan_file(temp_file.path()).unwrap();

    assert_eq!(tags.len(), 2);
    
    // Single line tag — lookahead advances from comment line 2 to executable line 3.
    // void syntax has no `fn` keyword so no function-end extension.
    let single = tags.iter().find(|t| t.req_id == "LLR-0001").unwrap();
    assert_eq!(single.line, 3);
    assert_eq!(single.line_end, None);
    
    // Block tag
    let block = tags.iter().find(|t| t.req_id == "LLR-0002").unwrap();
    assert_eq!(block.line, 5);
    assert_eq!(block.line_end, Some(9));
}

/// Test unclosed block tags are handled gracefully
/// Verifies: Block tag feature - error handling
#[test]
fn test_unclosed_block_tag() {
    let scanner = CodeScanner::new();
    let code = r#"
// BEGIN-REQ: LLR-0001
fn unclosed_function() {
    // no END-REQ
}
"#;
    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(code.as_bytes()).unwrap();
    temp_file.flush().unwrap();
    
    let tags = scanner.scan_file(temp_file.path()).unwrap();

    // Should treat as single-line tag. Lookahead advances from the BEGIN comment
    // (line 2) to the first executable line (line 3, the fn signature).
    assert_eq!(tags.len(), 1);
    let tag = &tags[0];
    assert_eq!(tag.req_id, "LLR-0001");
    assert_eq!(tag.line, 3);
    // Lookahead found a fn definition → line_end extended to closing brace (line 5) + 1 = 6.
    assert_eq!(tag.line_end, Some(6));
}

/// Test END-REQ without matching BEGIN-REQ is ignored
/// Verifies: Block tag feature - error handling
#[test]
fn test_orphan_end_tag() {
    let scanner = CodeScanner::new();
    let code = r#"
// END-REQ: LLR-0001
fn some_function() {
}
"#;
    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(code.as_bytes()).unwrap();
    temp_file.flush().unwrap();
    
    let tags = scanner.scan_file(temp_file.path()).unwrap();

    // Orphan END-REQ should be ignored
    assert_eq!(tags.len(), 0);
}
//! Core data models for the requirement traceability system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Requirement type - HLR (High-Level), LLR (Low-Level), or TST (Test)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequirementType {
    /// High-Level Requirement
    Hlr,
    /// Low-Level Requirement
    Llr,
    /// Test specification
    Tst,
}

impl RequirementType {
    /// Returns the lowercase string representation (e.g., `"hlr"`)
    pub fn as_str(&self) -> &'static str {
        match self {
            RequirementType::Hlr => "hlr",
            RequirementType::Llr => "llr",
            RequirementType::Tst => "tst",
        }
    }

    /// Parses a string into a `RequirementType`; returns `None` for unknown values
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "hlr" => Some(RequirementType::Hlr),
            "llr" => Some(RequirementType::Llr),
            "tst" | "test" => Some(RequirementType::Tst),
            _ => None,
        }
    }

    /// Get the ID prefix for this requirement type
    pub fn id_prefix(&self) -> &'static str {
        match self {
            RequirementType::Hlr => "HLR",
            RequirementType::Llr => "LLR",
            RequirementType::Tst => "TST",
        }
    }
}

impl std::fmt::Display for RequirementType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Requirement status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RequirementStatus {
    /// Work in progress; not yet reviewed
    Draft,
    #[default]
    /// Reviewed and accepted
    Approved,
    /// No longer valid; superseded or removed
    Deprecated,
    /// Reviewed and explicitly not accepted
    Rejected,
}

impl RequirementStatus {
    /// Returns the lowercase string representation (e.g., `"draft"`)
    pub fn as_str(&self) -> &'static str {
        match self {
            RequirementStatus::Draft => "draft",
            RequirementStatus::Approved => "approved",
            RequirementStatus::Deprecated => "deprecated",
            RequirementStatus::Rejected => "rejected",
        }
    }

    /// Parses a string into a `RequirementStatus`; returns `None` for unknown values
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "draft" => Some(RequirementStatus::Draft),
            "approved" => Some(RequirementStatus::Approved),
            "deprecated" => Some(RequirementStatus::Deprecated),
            "rejected" => Some(RequirementStatus::Rejected),
            _ => None,
        }
    }
}

impl std::fmt::Display for RequirementStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Link type between requirements
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkType {
    /// LLR refines HLR
    Refines,
    /// Code implements LLR
    Implements,
    /// Test verifies LLR
    Verifies,
    /// Generic reference
    References,
}

impl LinkType {
    /// Returns the lowercase string representation (e.g., `"refines"`)
    pub fn as_str(&self) -> &'static str {
        match self {
            LinkType::Refines => "refines",
            LinkType::Implements => "implements",
            LinkType::Verifies => "verifies",
            LinkType::References => "references",
        }
    }
}

impl std::fmt::Display for LinkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl LinkType {
    /// Parses a string into a `LinkType`; returns `None` for unknown values
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "refines" => Some(LinkType::Refines),
            "implements" => Some(LinkType::Implements),
            "verifies" => Some(LinkType::Verifies),
            "references" => Some(LinkType::References),
            _ => None,
        }
    }
}

/// A link between requirements or between requirement and code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    /// Source requirement ID
    pub source: String,
    /// Target requirement ID or code reference
    pub target: String,
    /// Type of link
    pub link_type: LinkType,
}

/// Reference to code implementing a requirement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRef {
    /// The requirement ID
    pub req_id: String,
    /// Source file path
    pub file: PathBuf,
    /// Line number where the tag was found
    pub line: usize,
    /// Optional end line (for blocks)
    pub line_end: Option<usize>,
    /// Hash of the code content for change detection
    pub hash: Option<String>,
    /// Optional function/symbol name
    pub symbol: Option<String>,
}

/// The core requirement structure
///
/// This is the universal data format that flows through the "USB bus"
/// architecture. It is format-agnostic and can be converted to/from
/// Markdown, ReqIF, JSON, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    /// Unique requirement ID (e.g., "HLR-0001", "LLR-0123")
    pub id: String,
    /// Type of requirement
    #[serde(rename = "type")]
    pub req_type: RequirementType,
    /// Short title/summary
    pub title: String,
    /// Full requirement text
    pub text: String,
    /// Current status
    #[serde(default)]
    pub status: RequirementStatus,
    /// Parent requirement ID (for LLR referencing HLR)
    pub parent: Option<String>,
    /// External UUIDs (for ReqIF round-trip)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// Additional metadata/attributes
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<String, String>,
    /// Source file where this requirement is stored
    #[serde(skip)]
    pub source_file: Option<PathBuf>,
    /// Creation timestamp
    #[serde(default = "Utc::now", skip_serializing)]
    pub created: DateTime<Utc>,
    /// Last modification timestamp
    #[serde(default = "Utc::now", skip_serializing)]
    pub modified: DateTime<Utc>,
}

impl Requirement {
    // REQ: LLR-0001
    // REQ: LLR-0015
    /// Create a new requirement with the given ID and type
    pub fn new(id: String, req_type: RequirementType, title: String) -> Self {
        Self {
            id,
            req_type,
            title,
            text: String::new(),
            status: RequirementStatus::Draft,
            parent: None,
            aliases: Vec::new(),
            attributes: HashMap::new(),
            source_file: None,
            created: Utc::now(),
            modified: Utc::now(),
        }
    }

    /// Parse requirement ID and extract type and number
    pub fn parse_id(id: &str) -> Option<(RequirementType, u32)> {
        let parts: Vec<&str> = id.split('-').collect();
        if parts.len() != 2 {
            return None;
        }

        let req_type = RequirementType::from_str(parts[0])?;
        let number: u32 = parts[1].parse().ok()?;

        Some((req_type, number))
    }

    /// Generate a new requirement ID with the next available number
    pub fn generate_id(req_type: RequirementType, existing_ids: &[String]) -> String {
        let prefix = req_type.id_prefix();
        let max_num = existing_ids
            .iter()
            .filter_map(|id| {
                if id.starts_with(prefix) {
                    id.split('-').nth(1).and_then(|n| n.parse::<u32>().ok())
                } else {
                    None
                }
            })
            .max()
            .unwrap_or(0);

        format!("{}-{:04}", prefix, max_num + 1)
    }
}

impl std::fmt::Display for Requirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.id, self.title)
    }
}

/// Coverage statistics for a set of requirements
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Coverage {
    /// Total HLR count
    pub hlr_total: usize,
    /// HLR with at least one LLR child
    pub hlr_with_llr: usize,
    /// Total LLR count
    pub llr_total: usize,
    /// LLR with code implementation
    pub llr_implemented: usize,
    /// LLR with tests
    pub llr_tested: usize,
    /// Code files without any requirement tag
    pub orphan_code: usize,
}

impl Coverage {
    /// Percentage of HLRs that have at least one LLR child
    pub fn hlr_coverage_percent(&self) -> f64 {
        if self.hlr_total == 0 {
            return 100.0;
        }
        (self.hlr_with_llr as f64 / self.hlr_total as f64) * 100.0
    }

    /// Percentage of LLRs that have at least one code reference
    pub fn llr_implementation_percent(&self) -> f64 {
        if self.llr_total == 0 {
            return 100.0;
        }
        (self.llr_implemented as f64 / self.llr_total as f64) * 100.0
    }

    /// Percentage of LLRs that have at least one test reference
    pub fn llr_test_percent(&self) -> f64 {
        if self.llr_total == 0 {
            return 100.0;
        }
        (self.llr_tested as f64 / self.llr_total as f64) * 100.0
    }
}

// BEGIN-REQ: LLR-0014
/// Current schema version for exports
pub const SCHEMA_VERSION: &str = "1.0.0";

/// AI Export format for LLM consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiExport {
    /// Schema version for migration support
    pub schema_version: String,
    /// Tool version that created this export
    pub tool_version: String,
    /// All requirements
    pub requirements: Vec<Requirement>,
    /// All trace links
    pub links: Vec<Link>,
    /// All code references
    pub code_refs: Vec<CodeRef>,
    /// Coverage summary
    pub coverage: Coverage,
    /// Export timestamp (not serialised — wall-clock value breaks determinism)
    #[serde(skip_serializing)]
    pub exported_at: DateTime<Utc>,
}

impl AiExport {
    /// Constructs a new `AiExport` snapshot with the current timestamp
    pub fn new(
        requirements: Vec<Requirement>,
        links: Vec<Link>,
        code_refs: Vec<CodeRef>,
        coverage: Coverage,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            requirements,
            links,
            code_refs,
            coverage,
            exported_at: Utc::now(),
        }
    }
}
// END-REQ: LLR-0014

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_id() {
        assert_eq!(
            Requirement::parse_id("HLR-0001"),
            Some((RequirementType::Hlr, 1))
        );
        assert_eq!(
            Requirement::parse_id("LLR-0123"),
            Some((RequirementType::Llr, 123))
        );
        assert_eq!(
            Requirement::parse_id("TST-0042"),
            Some((RequirementType::Tst, 42))
        );
        assert_eq!(Requirement::parse_id("invalid"), None);
        assert_eq!(Requirement::parse_id("XYZ-001"), None);
    }

    #[test]
    fn test_generate_id() {
        let existing = vec!["HLR-0001".to_string(), "HLR-0002".to_string()];
        let new_id = Requirement::generate_id(RequirementType::Hlr, &existing);
        assert_eq!(new_id, "HLR-0003");
    }

    #[test]
    fn test_coverage_percent() {
        let cov = Coverage {
            hlr_total: 10,
            hlr_with_llr: 8,
            llr_total: 50,
            llr_implemented: 45,
            llr_tested: 30,
            orphan_code: 5,
        };

        assert!((cov.hlr_coverage_percent() - 80.0).abs() < 0.01);
        assert!((cov.llr_implementation_percent() - 90.0).abs() < 0.01);
        assert!((cov.llr_test_percent() - 60.0).abs() < 0.01);
    }
}
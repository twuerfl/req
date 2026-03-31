// REQ: LLR-0030
//! Audit data types for AI agent output integrity verification.
//!
//! These types are in `req_lib` (qualified layer) so they can be shared
//! between `req_engine` (which produces them) and `req_mcp` (which may
//! expose them as MCP resources).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Schema ────────────────────────────────────────────────────────────────

/// Schema version for `AuditBundle` JSON output.
pub const AUDIT_SCHEMA_VERSION: &str = "2.0.0";

/// Static prompt hint embedded in every `AuditBundle`.
///
/// This constant is part of the qualified layer so the audit output is
/// fully deterministic and auditable as tool-qualification evidence.
pub const AUDIT_PROMPT_HINT: &str =
    "You are an independent reviewer of AI-generated safety-critical code. \
     For each item in `acceptance_criteria`: \
     (1) locate the corresponding test spans via `criterion_coverage`; \
     (2) determine whether the test genuinely exercises the criterion, \
         not merely a tautology or stub; \
     (3) review `implementation_spans` for any `triviality_findings`; \
     (4) flag criteria where `linked` is false (no CRITERION tag present); \
     (5) consider the `mutation_score` and `line_coverage_score` if present; \
     (6) return structured JSON: \
         { \"verdict\": \"pass\" | \"fail\" | \"needs_review\", \
           \"findings\": [ { \"criterion_index\": N, \"issue\": \"...\", \
                             \"severity\": \"error\" | \"warning\" | \"info\" } ] }.";

// ── Triviality ────────────────────────────────────────────────────────────

/// Pattern matched by the triviality detector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrivialityPattern {
    /// `assert!(true)`, `assert_eq!(x, x)` — tautological assertion
    TautologicalAssertion,
    /// Sole body is `Ok(())`, `unimplemented!()`, `todo!()`, or `panic!()`
    StubBody,
    /// Tagged span has no non-comment, non-blank lines
    ZeroSubstantiveLines,
    /// Function returns only a bare literal (`true`, `false`, `0`, `""`)
    SingleLiteralReturn,
    /// `#[test]` function with no assertions and no function calls
    EmptyTestBody,
    /// `assert_eq!(f(x), x)` — output mirrors input literally
    ArgumentEchoAssertion,
}

impl TrivialityPattern {
    /// Human-readable description of the pattern.
    pub fn description(self) -> &'static str {
        match self {
            Self::TautologicalAssertion => "tautological assertion (assert!(true) or equivalent)",
            Self::StubBody => "stub body (Ok(()), unimplemented!(), or todo!())",
            Self::ZeroSubstantiveLines => "no substantive lines in tagged span",
            Self::SingleLiteralReturn => "function returns only a bare literal",
            Self::EmptyTestBody => "test function has no assertions or function calls",
            Self::ArgumentEchoAssertion => "assert_eq mirrors its input argument",
        }
    }
}

/// Severity of a triviality finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingSeverity {
    /// Informational — worth noting but not blocking
    Info,
    /// Warning — likely an issue, should be reviewed
    Warning,
    /// Error — definitely indicates a hollow implementation or test
    Error,
}

impl std::fmt::Display for FindingSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "info"),
            Self::Warning => write!(f, "warning"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// A single triviality finding for one tagged span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrivialityFinding {
    /// Requirement ID the tagged span belongs to
    pub req_id: String,
    /// Source file
    pub file: PathBuf,
    /// Line number of the tag
    pub line: usize,
    /// Which pattern was matched
    pub pattern: TrivialityPattern,
    /// Severity
    pub severity: FindingSeverity,
    /// The matched source text (the triggering line or excerpt)
    pub matched_text: String,
}

/// Aggregated triviality findings for one LLR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrivialityReport {
    /// Requirement ID
    pub req_id: String,
    /// All findings for this requirement
    pub findings: Vec<TrivialityFinding>,
}

// ── Mutation testing ──────────────────────────────────────────────────────

/// Per-LLR mutation score derived from `cargo mutants --json` output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationScore {
    /// Requirement ID
    pub req_id: String,
    /// Total mutants in the tagged span (including Unviable)
    pub mutants_total: usize,
    /// Mutants with outcome `Caught`
    pub caught: usize,
    /// Mutants with outcome `Missed`
    pub missed: usize,
    /// `caught / (caught + missed) * 100.0`; `None` when `caught + missed == 0`
    pub score_percent: Option<f64>,
}

/// Overall mutation report summary (returned alongside per-LLR scores).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationReport {
    /// Per-LLR scores
    pub scores: Vec<MutationScore>,
    /// Mutants that did not overlap any CodeRef range
    pub untagged_mutants: usize,
}

// ── Line coverage ─────────────────────────────────────────────────────────

/// Per-LLR line coverage score derived from `cargo llvm-cov --json` output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineCoverageScore {
    /// Requirement ID
    pub req_id: String,
    /// Source file
    pub file: PathBuf,
    /// Total lines in the tagged span
    pub lines_total: usize,
    /// Lines with at least one execution hit
    pub lines_hit: usize,
    /// `lines_hit / lines_total * 100.0`; `None` when `lines_total == 0`
    pub hit_percent: Option<f64>,
}

// ── Criterion linkage ─────────────────────────────────────────────────────

/// Status of a single acceptance criterion item within an LLR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriterionStatus {
    /// 1-based index matching the `- [ ]` bullet position in the LLR file
    pub index: usize,
    /// Text of the criterion bullet (without the `- [ ] ` prefix)
    pub text: String,
    /// Whether at least one `CRITERION: <id> #N` tag exists for this index
    pub linked: bool,
    /// Source file containing the linking tag, if any
    pub test_file: Option<PathBuf>,
    /// Line of the linking tag, if any
    pub test_line: Option<usize>,
}

/// Per-LLR criterion linkage report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriteriaReport {
    /// Requirement ID
    pub req_id: String,
    /// Status of each criterion item
    pub criteria: Vec<CriterionStatus>,
    /// Warnings (e.g., `#N` reference exceeds criterion count)
    pub warnings: Vec<String>,
}

// ── Audit bundle ──────────────────────────────────────────────────────────

/// A source span with its extracted text — used inside `AuditBundle`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpan {
    /// Requirement ID the tag belongs to
    pub req_id: String,
    /// Source file path
    pub file: PathBuf,
    /// Start line (1-based)
    pub line: usize,
    /// End line (1-based, inclusive), if known
    pub line_end: Option<usize>,
    /// Extracted source text for the span (empty if file could not be read)
    pub source_text: String,
    /// Warning if the file could not be read
    pub read_warning: Option<String>,
}

/// One criterion item as it appears in the bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditCriterion {
    /// 1-based index
    pub index: usize,
    /// Full text of the criterion
    pub text: String,
    /// Whether a CRITERION tag exists for this index
    pub linked: bool,
    /// Locations of CRITERION tags for this index
    pub test_locations: Vec<SourceSpan>,
}

/// Full audit context bundle for one LLR — intended as LLM input payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditBundle {
    /// Schema version — always `AUDIT_SCHEMA_VERSION`
    pub schema_version: String,
    /// Tool version from `CARGO_PKG_VERSION`
    pub tool_version: String,
    /// ISO 8601 generation timestamp
    pub generated_at: String,
    /// The LLR under review (from cache)
    pub llr: crate::Requirement,
    /// The parent HLR, if present
    pub hlr: Option<crate::Requirement>,
    /// Acceptance criteria with linkage status
    pub acceptance_criteria: Vec<AuditCriterion>,
    /// Implementation spans (`REQ:` tagged)
    pub implementation_spans: Vec<SourceSpan>,
    /// Test spans (`VERIFIES:` tagged)
    pub test_spans: Vec<SourceSpan>,
    /// Triviality findings for this LLR
    pub triviality_findings: Vec<TrivialityFinding>,
    /// Mutation score, if a mutants report was supplied
    pub mutation_score: Option<MutationScore>,
    /// Line coverage score, if a coverage report was supplied
    pub line_coverage_score: Option<LineCoverageScore>,
    /// Criterion-level linkage (mirrors `acceptance_criteria[*].linked`)
    pub criterion_coverage: Vec<CriterionStatus>,
    /// Static prompt for the LLM reviewer — always `AUDIT_PROMPT_HINT`
    pub prompt_hint: String,
    /// Non-fatal warnings encountered while building the bundle
    pub warnings: Vec<String>,
}

// ── Independence ──────────────────────────────────────────────────────────

/// An independence violation: implementation and test share an author identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndependenceViolation {
    /// Requirement ID
    pub req_id: String,
    /// Identities found in the implementation spans
    pub impl_identities: Vec<String>,
    /// Identities found in the test spans
    pub test_identities: Vec<String>,
    /// The overlapping identities
    pub shared: Vec<String>,
}

/// A non-fatal warning from the independence check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndependenceWarning {
    /// Requirement ID (may be empty for file-level warnings)
    pub req_id: String,
    /// Affected file
    pub file: PathBuf,
    /// Human-readable reason (e.g., "uncommitted changes", "git feature disabled")
    pub reason: String,
}

/// Result of the full independence check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndependenceResult {
    /// Actual violations (implementation and test share an identity)
    pub violations: Vec<IndependenceViolation>,
    /// Non-fatal warnings (dirty files, feature disabled, etc.)
    pub warnings: Vec<IndependenceWarning>,
}

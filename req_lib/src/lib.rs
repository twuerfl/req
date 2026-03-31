//! # req_lib — Core domain types for the req traceability tool.
//!
//! This crate contains ONLY pure data types and their invariants.
//! It has no I/O, no database, no parsing dependencies.
//!
//! ## Qualification boundary
//!
//! `req_lib` is part of the qualifiable kernel under ISO 26262 / DO-178C
//! tool qualification. Nothing in this crate performs side effects.

// REQ: LLR-0021
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod audit;
pub mod models;

pub use audit::{
    AuditBundle, AuditCriterion, AUDIT_PROMPT_HINT, AUDIT_SCHEMA_VERSION,
    CriteriaReport, CriterionStatus, FindingSeverity, IndependenceResult,
    IndependenceViolation, IndependenceWarning, LineCoverageScore,
    MutationReport, MutationScore, SourceSpan, TrivialityFinding,
    TrivialityPattern, TrivialityReport,
};
pub use models::{
    AiExport, CodeRef, Coverage, Link, LinkType, Requirement,
    RequirementStatus, RequirementType, SCHEMA_VERSION,
};

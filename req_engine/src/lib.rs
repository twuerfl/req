//! # req_engine — Business logic for the req traceability tool.
//!
//! This crate contains all qualified engine logic:
//! - SQLite cache
//! - Code scanner
//! - Markdown parser
//! - Traceability graph
//! - Format adapters (Markdown, JSON, ReqIF)
//! - Provenance tracking
//! - AI import
//!
//! ## Qualification boundary
//!
//! Together with `req_lib`, this crate forms the qualifiable software
//! component under ISO 26262 / DO-178C tool qualification.
//! No presentation logic (colors, interactive I/O) lives here.
//!
//! ## Usage
//!
//! All operations go through [`ReqEngine`]:
//!
//! ```no_run
//! use req_engine::ReqEngine;
//! use std::path::Path;
//!
//! let engine = ReqEngine::open(Path::new(".")).unwrap();
//! let coverage = engine.coverage().unwrap();
//! println!("LLR implementation: {:.1}%", coverage.llr_implementation_percent());
//! ```

// REQ: LLR-0022
pub mod adapter;
pub mod ai_import;
pub mod audit;
pub mod cache;
pub mod config;
pub mod engine;
pub mod error;
pub mod parser;
pub mod provenance;
pub mod scanner;
pub mod trace;

pub use engine::{MigrateResult, ReqEngine, ScanResult};
pub use trace::{ImpactResult, Severity, TraceGaps, ValidationIssue};
pub use error::{Error, Result};

// Re-export req_lib types so dependents need only one import
pub use req_lib::{
    AiExport, AuditBundle, CodeRef, Coverage, CriteriaReport, FindingSeverity,
    IndependenceResult, Link, LinkType, LineCoverageScore, MutationReport, Requirement,
    RequirementStatus, RequirementType, SCHEMA_VERSION, TrivialityReport,
};

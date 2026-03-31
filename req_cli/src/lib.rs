//! # req_cli — CLI presentation shell for the req traceability tool.
//!
//! This crate contains ONLY argument parsing and output formatting.
//! All business logic lives in `req_engine`.
//!
//! This crate is NOT in scope for tool qualification.

// REQ: LLR-0023
pub mod cli;
pub mod hooks;
pub mod output;

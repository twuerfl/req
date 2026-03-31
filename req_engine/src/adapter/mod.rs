//! Adapter pattern for requirement import/export ("USB-Port" architecture).
//!
//! This module defines the trait all format adapters implement.
//! The engine is format-agnostic — it works with `Requirement` structs.
//! Adapters convert between external formats (Markdown, ReqIF, JSON) and
//! the internal representation.

use crate::Result;
use req_lib::Requirement;
use std::path::Path;

/// Trait for requirement format adapters.
///
/// This is the "USB port" — a universal interface that allows
/// plugging in different format handlers.
pub trait RequirementAdapter: Send + Sync {
    /// Human-readable name of the adapter
    fn name(&self) -> &'static str;

    /// Read requirements from a source (file or directory)
    fn read(&self, source: &Path) -> Result<Vec<Requirement>>;

    /// Write requirements to a target (file or directory)
    fn write(&self, requirements: &[Requirement], target: &Path) -> Result<()>;

    /// Check if this adapter can handle the given source
    fn can_handle(&self, source: &Path) -> bool;
}

/// Markdown adapter — the default format for Git-native requirements
pub mod markdown;

/// JSON adapter — for AI integration and CI pipelines
pub mod json;

/// ReqIF adapter — for DOORS/Polarion interoperability (optional feature).
/// The struct is always compiled; the PyO3-dependent methods are gated inside.
pub mod reqif;

pub use json::JsonAdapter;
pub use markdown::MarkdownAdapter;
pub use reqif::ReqIfAdapter;

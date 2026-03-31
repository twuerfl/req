//! Parser coordination — high-level parsing functions.

use crate::adapter::{MarkdownAdapter, RequirementAdapter};
use crate::{Error, Result};
use req_lib::Requirement;
use std::path::Path;

/// Parse all requirements from a directory or single file
pub fn parse_requirements(source: &Path) -> Result<Vec<Requirement>> {
    let md_adapter = MarkdownAdapter::new();

    if md_adapter.can_handle(source) || source.is_dir() {
        return md_adapter.read(source);
    }

    Err(Error::Parse(format!(
        "Cannot parse requirements from: {}",
        source.display()
    )))
}

/// Parse a single requirement file
pub fn parse_requirement_file(path: &Path) -> Result<Requirement> {
    let md_adapter = MarkdownAdapter::new();

    if md_adapter.can_handle(path) {
        let mut requirements = md_adapter.read(path)?;
        return requirements.pop().ok_or_else(|| {
            Error::Parse(format!("No requirement found in {}", path.display()))
        });
    }

    Err(Error::Parse(format!(
        "Cannot parse file: {}",
        path.display()
    )))
}

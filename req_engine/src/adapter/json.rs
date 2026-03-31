//! JSON adapter for AI integration and CI pipelines.
// REQ: LLR-0007
//!
//! Provides structured JSON export for LLM consumption and
//! import for AI-generated requirements.

use crate::adapter::RequirementAdapter;
use crate::{Error, Result};
use req_lib::{AiExport, CodeRef, Link, Requirement};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;

/// JSON adapter for reading/writing requirements
pub struct JsonAdapter;

impl JsonAdapter {
    /// Create a new JsonAdapter
    pub fn new() -> Self {
        Self
    }
}

impl Default for JsonAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RequirementAdapter for JsonAdapter {
    fn name(&self) -> &'static str {
        "json"
    }

    fn read(&self, source: &Path) -> Result<Vec<Requirement>> {
        if source.is_file() {
            let content = fs::read_to_string(source)?;
            let requirements: Vec<Requirement> = serde_json::from_str(&content)?;
            Ok(requirements)
        } else {
            let json_file = source.join("requirements.json");
            if json_file.exists() {
                let content = fs::read_to_string(&json_file)?;
                let requirements: Vec<Requirement> = serde_json::from_str(&content)?;
                Ok(requirements)
            } else {
                Err(Error::NoRequirementsDir)
            }
        }
    }

    fn write(&self, requirements: &[Requirement], target: &Path) -> Result<()> {
        let output_path = if target.is_dir() {
            target.join("requirements.json")
        } else {
            target.to_path_buf()
        };

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = fs::File::create(&output_path)?;
        let mut writer = BufWriter::new(file);
        let json = serde_json::to_string_pretty(requirements)?;
        writer.write_all(json.as_bytes())?;
        writer.flush()?;

        Ok(())
    }

    fn can_handle(&self, source: &Path) -> bool {
        if source.is_file() {
            source.extension().map(|e| e == "json").unwrap_or(false)
        } else if source.is_dir() {
            source.join("requirements.json").exists()
        } else {
            false
        }
    }
}

/// Build a full AI export from requirements, code refs, links, and coverage.
pub fn export_ai_context(
    requirements: &[Requirement],
    code_refs: &[CodeRef],
    links: &[Link],
) -> Result<String> {
    let export = AiExport::new(
        requirements.to_vec(),
        links.to_vec(),
        code_refs.to_vec(),
        Default::default(),
    );

    serde_json::to_string_pretty(&export).map_err(Error::Json)
}

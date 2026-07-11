//! AI suggestion import with safety guarantees.
//!
//! Provides structured import of AI-generated requirement suggestions,
//! with validation, duplicate detection, and forced-draft status.
//!
//! The `provenance` field records the origin system (e.g. "claude-3-7",
//! "gpt-4o", "doors-export") for audit trails.

use crate::cache::Cache;
use crate::Result;
use req_lib::{Requirement, RequirementStatus, RequirementType};
use std::collections::HashSet;
use std::path::Path;

/// A single AI-generated requirement suggestion
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiSuggestion {
    /// Optional suggested ID — validated if provided
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Requirement type (hlr, llr, tst)
    #[serde(rename = "type")]
    pub req_type: String,
    /// Requirement title
    pub title: String,
    /// Requirement text/description
    #[serde(default)]
    pub text: String,
    /// Parent requirement ID (required for LLR)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Rationale for the suggestion (stored as attribute)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

/// Container for a batch of AI suggestions
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiSuggestions {
    /// Source/model identifier (stored as provenance on each requirement)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Generation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    /// The suggestions
    pub suggestions: Vec<AiSuggestion>,
}

/// Result of importing AI suggestions
#[derive(Debug, Default, serde::Serialize)]
pub struct ImportResult {
    /// Successfully imported requirements (all forced to `draft`)
    pub imported: Vec<Requirement>,
    /// Skipped items: (id/title, reason)
    pub skipped: Vec<(String, String)>,
    /// Errors encountered
    pub errors: Vec<String>,
}

/// Options controlling the import behaviour
#[derive(Debug, Default)]
pub struct ImportOptions {
    /// If true, validate but do not write to cache or disk
    pub dry_run: bool,
    /// Origin identifier stored as `source` attribute (replaces REQ_TOKEN concept)
    pub provenance: Option<String>,
    /// Path of the source file being imported (used for staleness tracking)
    pub source_path: Option<std::path::PathBuf>,
}

// REQ: LLR-0012
/// Load suggestions from a JSON file
pub fn load_suggestions(path: &Path) -> Result<AiSuggestions> {
    let content = std::fs::read_to_string(path)?;
    let suggestions: AiSuggestions = serde_json::from_str(&content)?;
    Ok(suggestions)
}

/// Import AI suggestions with safety guarantees.
///
/// Safety rules enforced here (not in the CLI):
/// - All imported requirements are forced to `status: draft`
/// - Duplicate IDs are rejected
/// - LLR without a parent is rejected
/// - Parent must exist in cache
pub fn import_suggestions(
    suggestions: AiSuggestions,
    cache: &Cache,
    options: &ImportOptions,
) -> Result<ImportResult> {
    let mut result = ImportResult::default();

    let existing_ids: HashSet<String> = cache.get_all_ids()?.into_iter().collect();

    // Derive provenance from options or from the suggestions source field
    let provenance = options
        .provenance
        .as_deref()
        .or(suggestions.source.as_deref());

    for suggestion in suggestions.suggestions {
        let req_type = match RequirementType::from_str(&suggestion.req_type) {
            Some(t) => t,
            None => {
                result.errors.push(format!(
                    "Invalid type '{}' for '{}'",
                    suggestion.req_type, suggestion.title
                ));
                continue;
            }
        };

        let id = match &suggestion.id {
            Some(provided_id) => {
                if !is_valid_id_format(provided_id) {
                    result
                        .errors
                        .push(format!("Invalid ID format: '{}'", provided_id));
                    continue;
                }
                if existing_ids.contains(provided_id) {
                    result.skipped.push((
                        provided_id.clone(),
                        "ID already exists".to_string(),
                    ));
                    continue;
                }
                provided_id.clone()
            }
            None => {
                let ids: Vec<String> = existing_ids.iter().cloned().collect();
                Requirement::generate_id(req_type, &ids)
            }
        };

        if req_type == RequirementType::Llr {
            match &suggestion.parent {
                Some(parent_id) => {
                    if !cache.requirement_exists(parent_id)? {
                        result.errors.push(format!(
                            "Parent '{}' not found for '{}'",
                            parent_id, suggestion.title
                        ));
                        continue;
                    }
                }
                None => {
                    result.skipped.push((
                        id.clone(),
                        "LLR without parent — specify parent HLR".to_string(),
                    ));
                    continue;
                }
            }
        }

        let mut req = Requirement::new(id.clone(), req_type, suggestion.title.clone());
        req.text = suggestion.text.clone();
        req.parent = suggestion.parent.clone();
        req.status = RequirementStatus::Draft; // ALWAYS draft — safety invariant

        req.attributes
            .insert("source".to_string(), "ai-suggestion".to_string());

        if let Some(p) = provenance {
            req.attributes
                .insert("provenance".to_string(), p.to_string());
        }

        if let Some(ref rationale) = suggestion.rationale {
            req.attributes
                .insert("ai_rationale".to_string(), rationale.clone());
        }

        result.imported.push(req);
    }

    if !options.dry_run {
        for req in &result.imported {
            cache.upsert_requirement(req)?;
        }
    }

    Ok(result)
}

fn is_valid_id_format(id: &str) -> bool {
    let parts: Vec<&str> = id.split('-').collect();
    if parts.len() != 2 {
        return false;
    }
    ["HLR", "LLR", "TST"].contains(&parts[0]) && parts[1].chars().all(|c| c.is_ascii_digit())
}

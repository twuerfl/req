//! Error types for the req engine.

use thiserror::Error;

/// Engine-level result type.
pub type Result<T> = std::result::Result<T, Error>;

/// All errors that can occur in the req engine.
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("YAML parsing error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML parsing error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Requirement not found: {0}")]
    RequirementNotFound(String),

    #[error("Duplicate requirement ID: {0}")]
    DuplicateId(String),

    #[error("Invalid requirement ID format: {0}")]
    InvalidIdFormat(String),

    #[error("Invalid requirement type: {0}")]
    InvalidRequirementType(String),

    #[error("Parent requirement not found: {0}")]
    ParentNotFound(String),

    #[error("Project not initialized. Run 'req init' first.")]
    NotInitialized,

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("No requirements directory found")]
    NoRequirementsDir,

    #[error("No source code found")]
    NoSourceCode,

    #[error(
        "cache.db is locked (likely held by the MCP server).\n\
         Stop the MCP server or use --wait <seconds> to retry."
    )]
    DatabaseLocked,
}

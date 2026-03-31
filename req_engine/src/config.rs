//! Configuration management for the req tool.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Main project configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Project name
    #[serde(default)]
    pub project: String,
    /// Source directories to scan
    #[serde(default = "default_source_dirs")]
    pub source_dirs: Vec<String>,
    /// Requirements directory
    #[serde(default = "default_requirements_dir")]
    pub requirements_dir: String,
    /// Adapter profiles for named import/export configurations
    #[serde(default)]
    pub profiles: std::collections::HashMap<String, Profile>,
    /// Default adapter
    #[serde(default = "default_adapter")]
    pub default_adapter: String,
}

fn default_source_dirs() -> Vec<String> {
    vec!["src".to_string(), "tests".to_string()]
}

fn default_requirements_dir() -> String {
    "requirements".to_string()
}

fn default_adapter() -> String {
    "markdown".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            project: String::new(),
            source_dirs: default_source_dirs(),
            requirements_dir: default_requirements_dir(),
            profiles: std::collections::HashMap::new(),
            default_adapter: "markdown".to_string(),
        }
    }
}

impl Config {
    // REQ: LLR-0008
    /// Load configuration from file, returning defaults if absent
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path).map_err(Error::Io)?;
        let config: Config = toml::from_str(&content).map_err(Error::Toml)?;
        Ok(config)
    }

    /// Save configuration to file
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)
            .map_err(|e| Error::Config(format!("Failed to serialize config: {}", e)))?;

        fs::write(path, content).map_err(Error::Io)?;
        Ok(())
    }

    /// Get the requirements directory path
    pub fn requirements_path(&self, base: &Path) -> PathBuf {
        base.join(&self.requirements_dir)
    }

    /// Get the SQLite cache path
    pub fn cache_path(&self, base: &Path) -> PathBuf {
        base.join(".req").join("cache.db")
    }

    /// Get the config file path for a project root
    pub fn config_path(base: &Path) -> PathBuf {
        base.join(".req").join("config.toml")
    }
}

/// Named import/export profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// Format adapter to use
    pub format: String,
    /// Optional mapping configuration file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping: Option<String>,
    /// Optional source path (for import profiles)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Check if a project has been initialized in a directory
pub fn is_initialized(base: &Path) -> bool {
    base.join(".req").exists() && base.join("requirements").exists()
}

/// Initialize a new project in a directory
pub fn init_project(base: &Path, project_name: Option<&str>) -> Result<()> {
    fs::create_dir_all(base.join(".req"))?;
    fs::create_dir_all(base.join("requirements/hlr"))?;
    fs::create_dir_all(base.join("requirements/llr"))?;
    fs::create_dir_all(base.join("requirements/tst"))?;

    let mut config = Config::default();
    if let Some(name) = project_name {
        config.project = name.to_string();
    }

    let config_path = Config::config_path(base);
    config.save(&config_path)?;

    fs::write(
        base.join(".req/.gitignore"),
        "# Cache database — can be regenerated\n*.db\n",
    )?;

    Ok(())
}

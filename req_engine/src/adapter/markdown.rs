//! Markdown adapter for Git-native requirement storage.
//!
//! Requirements are stored as Markdown files with YAML frontmatter:
//!
//! ```markdown
//! ---
//! id: LLR-0001
//! type: llr
//! parent: HLR-0001
//! status: approved
//! aliases:
//!   - reqif-uuid-123e4567...
//! ---
//!
//! # ADC Sampling Rate
//!
//! The ADC shall sample the motor current at 10 kHz...
//! ```

use crate::adapter::RequirementAdapter;
use crate::{Error, Result};
use req_lib::{Requirement, RequirementStatus, RequirementType};
use std::collections::HashMap;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;

/// YAML frontmatter structure for parsing
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct Frontmatter {
    id: String,
    #[serde(rename = "type")]
    req_type: String,
    title: Option<String>,
    #[serde(default)]
    status: String,
    parent: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    attributes: HashMap<String, String>,
}

/// Markdown adapter for reading/writing requirements
pub struct MarkdownAdapter;

impl MarkdownAdapter {
    /// Create a new MarkdownAdapter
    pub fn new() -> Self {
        Self
    }

    /// Parse a single markdown file into a requirement
    fn parse_file(&self, path: &Path) -> Result<Requirement> {
        let content = fs::read_to_string(path)?;
        self.parse_content(&content, Some(path.to_path_buf()))
    }

    // REQ: LLR-0003
    /// Parse markdown content into a requirement
    pub fn parse_content(
        &self,
        content: &str,
        source_file: Option<std::path::PathBuf>,
    ) -> Result<Requirement> {
        let (frontmatter, body) = self.extract_frontmatter(content)?;

        let req_type = RequirementType::from_str(&frontmatter.req_type)
            .ok_or_else(|| Error::InvalidRequirementType(frontmatter.req_type.clone()))?;

        let status = RequirementStatus::from_str(&frontmatter.status)
            .unwrap_or(RequirementStatus::Draft);

        let title = frontmatter.title.clone().unwrap_or_else(|| {
            self.extract_title(body)
                .unwrap_or_else(|| "Untitled".to_string())
        });

        let text = self.clean_body(body);

        Ok(Requirement {
            id: frontmatter.id,
            req_type,
            title,
            text,
            status,
            parent: frontmatter.parent,
            aliases: frontmatter.aliases,
            attributes: frontmatter.attributes,
            source_file,
            created: chrono::Utc::now(),
            modified: chrono::Utc::now(),
        })
    }

    fn extract_frontmatter<'a>(&self, content: &'a str) -> Result<(Frontmatter, &'a str)> {
        if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
            return Err(Error::Parse("No YAML frontmatter found".to_string()));
        }

        let start = if content.starts_with("---\r\n") { 5 } else { 4 };
        let rest = &content[start..];

        let end_pos = rest
            .find("\n---\n")
            .or_else(|| rest.find("\n---\r\n"))
            .or_else(|| rest.find("\r\n---\r\n"))
            .ok_or_else(|| Error::Parse("Unclosed YAML frontmatter".to_string()))?;

        let yaml_content = &rest[..end_pos];
        let body_start = if rest[end_pos..].starts_with("\n---\r\n") {
            end_pos + 6
        } else if rest[end_pos..].starts_with("\r\n---\r\n") {
            end_pos + 8
        } else {
            end_pos + 5
        };

        let body = &rest[body_start..];

        let frontmatter: Frontmatter = serde_yaml::from_str(yaml_content)
            .map_err(|e| Error::Parse(format!("YAML parsing error: {}", e)))?;

        Ok((frontmatter, body))
    }

    fn extract_title(&self, body: &str) -> Option<String> {
        for line in body.lines() {
            let line = line.trim();
            if line.starts_with("# ") {
                return Some(line[2..].trim().to_string());
            }
        }
        None
    }

    fn clean_body(&self, body: &str) -> String {
        let mut lines: Vec<&str> = body.lines().collect();

        if lines
            .first()
            .is_some_and(|first| first.trim().starts_with("# "))
        {
            lines.remove(0);
        }

        while lines.first().map(|l| l.trim().is_empty()).unwrap_or(false) {
            lines.remove(0);
        }
        while lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
            lines.pop();
        }

        lines.join("\n")
    }

    fn write_file(&self, req: &Requirement, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = fs::File::create(path)?;
        let mut writer = BufWriter::new(file);

        writeln!(writer, "---")?;
        writeln!(writer, "id: {}", req.id)?;
        writeln!(writer, "type: {}", req.req_type.as_str())?;
        writeln!(
            writer,
            "title: {}",
            serde_json::to_string(&req.title).unwrap()
        )?;
        writeln!(writer, "status: {}", req.status.as_str())?;

        if let Some(parent) = &req.parent {
            writeln!(writer, "parent: {}", parent)?;
        }

        if !req.aliases.is_empty() {
            writeln!(writer, "aliases:")?;
            for alias in &req.aliases {
                writeln!(writer, "  - {}", alias)?;
            }
        }

        if !req.attributes.is_empty() {
            writeln!(writer, "attributes:")?;
            for (key, value) in &req.attributes {
                writeln!(
                    writer,
                    "  {}: {}",
                    key,
                    serde_json::to_string(value).unwrap()
                )?;
            }
        }

        writeln!(writer, "---")?;
        writeln!(writer)?;

        let text = req.text.trim();
        if !text.is_empty() {
            writeln!(writer, "{}", text)?;
        }

        writer.flush()?;
        Ok(())
    }

    /// Serialise a requirement to a Markdown string (same format as write_file).
    pub fn format_to_string(&self, req: &Requirement) -> Result<String> {
        let mut buf: Vec<u8> = Vec::new();
        let mut w = BufWriter::new(&mut buf);

        writeln!(w, "---")?;
        writeln!(w, "id: {}", req.id)?;
        writeln!(w, "type: {}", req.req_type.as_str())?;
        writeln!(w, "title: {}", serde_json::to_string(&req.title).unwrap())?;
        writeln!(w, "status: {}", req.status.as_str())?;
        if let Some(parent) = &req.parent {
            writeln!(w, "parent: {}", parent)?;
        }
        if !req.aliases.is_empty() {
            writeln!(w, "aliases:")?;
            for alias in &req.aliases {
                writeln!(w, "  - {}", alias)?;
            }
        }
        if !req.attributes.is_empty() {
            writeln!(w, "attributes:")?;
            let mut attrs: Vec<_> = req.attributes.iter().collect();
            attrs.sort_by_key(|(k, _)| k.as_str());
            for (key, value) in attrs {
                writeln!(w, "  {}: {}", key, serde_json::to_string(value).unwrap())?;
            }
        }
        writeln!(w, "---")?;
        writeln!(w)?;
        let text = req.text.trim();
        if !text.is_empty() {
            writeln!(w, "{}", text)?;
        }
        w.flush()?;
        drop(w);
        Ok(String::from_utf8(buf).map_err(|e| Error::Parse(e.to_string()))?)
    }

    /// Get the file path for a requirement based on its type and base directory
    pub fn get_requirement_path(base: &Path, req: &Requirement) -> std::path::PathBuf {
        let subdir = req.req_type.as_str();
        let filename = format!("{}.md", req.id);
        base.join("requirements").join(subdir).join(filename)
    }
}

impl Default for MarkdownAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RequirementAdapter for MarkdownAdapter {
    fn name(&self) -> &'static str {
        "markdown"
    }

    fn read(&self, source: &Path) -> Result<Vec<Requirement>> {
        let mut requirements = Vec::new();

        if source.is_file() {
            let req = self.parse_file(source)?;
            requirements.push(req);
        } else if source.is_dir() {
            for entry in walkdir::WalkDir::new(source)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.extension().map(|e| e == "md").unwrap_or(false) {
                    if path
                        .file_name()
                        .map(|n| n.to_string_lossy().starts_with('.'))
                        .unwrap_or(true)
                    {
                        continue;
                    }

                    match self.parse_file(path) {
                        Ok(req) => requirements.push(req),
                        Err(e) => {
                            eprintln!("Warning: Failed to parse {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }

        Ok(requirements)
    }

    fn write(&self, requirements: &[Requirement], target: &Path) -> Result<()> {
        for req in requirements {
            let path = Self::get_requirement_path(target, req);
            self.write_file(req, &path)?;
        }
        Ok(())
    }

    fn can_handle(&self, source: &Path) -> bool {
        if source.is_file() {
            source.extension().map(|e| e == "md").unwrap_or(false)
        } else if source.is_dir() {
            source.join("requirements").exists()
                || walkdir::WalkDir::new(source)
                    .min_depth(1)
                    .max_depth(2)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .any(|e| e.path().extension().map(|e| e == "md").unwrap_or(false))
        } else {
            false
        }
    }
}

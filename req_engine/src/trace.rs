//! Traceability graph builder and analysis.
//!
//! Builds the traceability graph from requirements and code references,
//! providing gap detection, impact analysis, and validation.

use crate::cache::Cache;
use crate::Result;
use req_lib::{CodeRef, Coverage, Link, LinkType, Requirement, RequirementType};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Traceability graph representing all requirement relationships
pub struct TraceGraph {
    /// All requirements indexed by ID
    pub requirements: HashMap<String, Requirement>,
    /// Code references indexed by requirement ID
    pub code_refs: HashMap<String, Vec<CodeRef>>,
    /// Links between requirements / code
    pub links: Vec<Link>,
    /// Children index: parent ID → child IDs
    children: HashMap<String, Vec<String>>,
}

impl TraceGraph {
    // REQ: LLR-0005
    /// Build a trace graph from the cache
    pub fn from_cache(cache: &Cache) -> Result<Self> {
        let requirements_vec = cache.get_all_requirements()?;
        let code_refs_vec = cache.get_all_code_refs()?;
        let links = cache.get_all_links()?;

        let requirements: HashMap<String, Requirement> =
            requirements_vec.into_iter().map(|r| (r.id.clone(), r)).collect();

        let mut code_refs: HashMap<String, Vec<CodeRef>> = HashMap::new();
        for cr in code_refs_vec {
            code_refs.entry(cr.req_id.clone()).or_default().push(cr);
        }

        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        for (id, req) in &requirements {
            if let Some(parent) = &req.parent {
                children.entry(parent.clone()).or_default().push(id.clone());
            }
        }

        Ok(Self {
            requirements,
            code_refs,
            links,
            children,
        })
    }

    /// Get a requirement by ID
    pub fn get_requirement(&self, id: &str) -> Option<&Requirement> {
        self.requirements.get(id)
    }

    /// Get children of a requirement
    pub fn get_children(&self, id: &str) -> Vec<&Requirement> {
        self.children
            .get(id)
            .map(|ids| ids.iter().filter_map(|id| self.requirements.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get parent of a requirement
    pub fn get_parent(&self, id: &str) -> Option<&Requirement> {
        self.requirements
            .get(id)
            .and_then(|r| r.parent.as_ref())
            .and_then(|pid| self.requirements.get(pid))
    }

    /// Get code references for a requirement
    pub fn get_code_refs(&self, id: &str) -> Vec<&CodeRef> {
        self.code_refs
            .get(id)
            .map(|refs| refs.iter().collect())
            .unwrap_or_default()
    }

    /// Check if a requirement has any code implementation
    pub fn is_implemented(&self, id: &str) -> bool {
        self.code_refs.contains_key(id)
    }

    /// Calculate coverage statistics from the graph
    pub fn calculate_coverage(&self) -> Coverage {
        let mut coverage = Coverage::default();

        for (id, req) in &self.requirements {
            match req.req_type {
                RequirementType::Hlr => {
                    coverage.hlr_total += 1;
                    if self.children.contains_key(id) {
                        coverage.hlr_with_llr += 1;
                    }
                }
                RequirementType::Llr => {
                    coverage.llr_total += 1;
                    if self.code_refs.contains_key(id) {
                        coverage.llr_implemented += 1;
                    }
                }
                RequirementType::Tst => {}
            }
        }

        for link in &self.links {
            if link.link_type == LinkType::Verifies {
                if let Some(target_req) = self.requirements.get(&link.target) {
                    if target_req.req_type == RequirementType::Llr {
                        coverage.llr_tested += 1;
                    }
                }
            }
        }

        coverage
    }

    /// Find all traceability gaps
    // REQ: LLR-0034
    pub fn find_gaps(&self) -> TraceGaps {
        let mut gaps = TraceGaps::default();

        for (id, req) in &self.requirements {
            match req.req_type {
                RequirementType::Hlr => {
                    if !self.children.contains_key(id) {
                        gaps.hlr_without_llr.push(id.clone());
                    }
                }
                RequirementType::Llr => {
                    if req.parent.is_none() {
                        gaps.llr_without_parent.push(id.clone());
                    } else if let Some(parent) = &req.parent {
                        if !self.requirements.contains_key(parent) {
                            gaps.llr_missing_parent.push(id.clone());
                        }
                    }
                    if !self.code_refs.contains_key(id) {
                        gaps.llr_without_code.push(id.clone());
                    }
                }
                RequirementType::Tst => {}
            }
        }

        for link in &self.links {
            if link.link_type != LinkType::Verifies
                && !self.requirements.contains_key(&link.source)
            {
                gaps.undefined_ids.push(link.source.clone());
            }
            if !self.requirements.contains_key(&link.target) {
                gaps.undefined_ids.push(link.target.clone());
            }
        }

        for req_id in self.code_refs.keys() {
            if !self.requirements.contains_key(req_id) {
                gaps.code_refs_undefined.push(req_id.clone());
            }
        }

        gaps.hlr_without_llr.sort();
        gaps.llr_without_parent.sort();
        gaps.llr_missing_parent.sort();
        gaps.llr_without_code.sort();
        gaps.undefined_ids.sort();
        gaps.code_refs_undefined.sort();

        gaps
    }

    // REQ: LLR-0020
    /// Impact analysis: find everything affected when a requirement changes
    pub fn impact_analysis(&self, req_id: &str) -> ImpactResult {
        let mut affected_files: HashSet<PathBuf> = HashSet::new();
        let mut affected_requirements: HashSet<String> = HashSet::new();
        let mut affected_tests: HashSet<String> = HashSet::new();

        let mut queue: Vec<&str> = vec![req_id];
        while let Some(current_id) = queue.pop() {
            affected_requirements.insert(current_id.to_string());

            if let Some(children) = self.children.get(current_id) {
                for child_id in children {
                    queue.push(child_id);
                }
            }

            if let Some(refs) = self.code_refs.get(current_id) {
                for cr in refs {
                    affected_files.insert(cr.file.clone());
                }
            }

            for link in &self.links {
                if link.target == current_id && link.link_type == LinkType::Verifies {
                    affected_tests.insert(link.source.clone());
                }
            }
        }

        ImpactResult {
            requirement_id: req_id.to_string(),
            affected_requirements: affected_requirements.into_iter().collect(),
            affected_files: affected_files.into_iter().collect(),
            affected_tests: affected_tests.into_iter().collect(),
        }
    }

    /// Validate all traceability links and return issues.
    ///
    /// Calls `validate_with_gaps` using the graph's own structural gaps.
    pub fn validate(&self) -> Vec<ValidationIssue> {
        let gaps = self.find_gaps();
        self.validate_with_gaps(gaps)
    }

    /// Validate using a pre-built `TraceGaps` (which may include import
    /// staleness flags populated from the database by the engine layer).
    // REQ: LLR-0034
    // REQ: LLR-0037
    pub fn validate_with_gaps(&self, gaps: TraceGaps) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        for id in &gaps.hlr_without_llr {
            issues.push(ValidationIssue {
                severity: Severity::Warning,
                message: format!("HLR '{}' has no LLR children", id),
                requirement_id: Some(id.clone()),
            });
        }

        for id in &gaps.llr_without_parent {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!("LLR '{}' has no parent HLR", id),
                requirement_id: Some(id.clone()),
            });
        }

        for id in &gaps.llr_missing_parent {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!("LLR '{}' references non-existent parent", id),
                requirement_id: Some(id.clone()),
            });
        }

        for id in &gaps.llr_without_code {
            issues.push(ValidationIssue {
                severity: Severity::Warning,
                message: format!("LLR '{}' is not implemented in code", id),
                requirement_id: Some(id.clone()),
            });
        }

        for id in &gaps.undefined_ids {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!("Requirement '{}' is referenced but not defined", id),
                requirement_id: Some(id.clone()),
            });
        }

        for id in &gaps.code_refs_undefined {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!("Code references undefined requirement '{}'", id),
                requirement_id: Some(id.clone()),
            });
        }

        // REQ: LLR-0037
        for id in &gaps.import_stale {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!(
                    "'{}' was imported from a source that has since changed (import_stale)",
                    id
                ),
                requirement_id: Some(id.clone()),
            });
        }

        // REQ: LLR-0037
        for id in &gaps.import_orphaned {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!(
                    "'{}' was imported from a source that no longer exists (import_orphaned)",
                    id
                ),
                requirement_id: Some(id.clone()),
            });
        }

        for link in &self.links {
            if link.link_type != LinkType::Verifies
                && !self.requirements.contains_key(&link.source)
            {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    message: format!("Link source '{}' does not exist", link.source),
                    requirement_id: None,
                });
            }
            if !self.requirements.contains_key(&link.target) {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    message: format!("Link target '{}' does not exist", link.target),
                    requirement_id: None,
                });
            }
        }

        issues.sort_by(|a, b| {
            a.requirement_id.as_deref().unwrap_or("")
                .cmp(b.requirement_id.as_deref().unwrap_or(""))
                .then(a.message.cmp(&b.message))
        });

        issues
    }

    // REQ: LLR-0019
    /// Generate a human-readable trace tree string for a requirement
    pub fn trace_tree(&self, req_id: &str, depth: usize) -> Option<String> {
        let req = self.requirements.get(req_id)?;
        let indent = "  ".repeat(depth);

        let mut output = format!("{}{}: {}", indent, req.id, req.title);

        if let Some(parent) = &req.parent {
            output.push_str(&format!("\n{}  parent: {}", indent, parent));
        }

        if let Some(refs) = self.code_refs.get(req_id) {
            output.push_str(&format!("\n{}  implemented in:", indent));
            let mut seen = std::collections::HashSet::new();
            for cr in refs {
                let key = (cr.file.to_string_lossy().into_owned(), cr.line, cr.line_end);
                if !seen.insert(key) {
                    continue;
                }
                output.push_str(&format!(
                    "\n{}    - {}:{}{}",
                    indent,
                    cr.file.display(),
                    cr.line,
                    cr.line_end
                        .map(|e| format!("-{}", e))
                        .unwrap_or_default()
                ));
            }
        }

        let children = self.get_children(req_id);
        if !children.is_empty() {
            output.push_str(&format!("\n{}  children:", indent));
            for child in children {
                output.push_str(&format!("\n{}    - {}", indent, child.id));
            }
        }

        Some(output)
    }
}

/// Gaps found in the traceability graph
#[derive(Debug, Default, serde::Serialize)]
pub struct TraceGaps {
    pub hlr_without_llr: Vec<String>,
    pub llr_without_parent: Vec<String>,
    pub llr_missing_parent: Vec<String>,
    pub llr_without_code: Vec<String>,
    pub undefined_ids: Vec<String>,
    pub code_refs_undefined: Vec<String>,
    /// Requirements whose import source file has changed since import
    // REQ: LLR-0037
    pub import_stale: Vec<String>,
    /// Requirements whose import source file no longer exists
    // REQ: LLR-0037
    pub import_orphaned: Vec<String>,
}

impl TraceGaps {
    /// True if no gaps were found
    pub fn is_empty(&self) -> bool {
        self.hlr_without_llr.is_empty()
            && self.llr_without_parent.is_empty()
            && self.llr_missing_parent.is_empty()
            && self.llr_without_code.is_empty()
            && self.undefined_ids.is_empty()
            && self.code_refs_undefined.is_empty()
            && self.import_stale.is_empty()
            && self.import_orphaned.is_empty()
    }
}

/// Result of an impact analysis
#[derive(Debug, serde::Serialize)]
pub struct ImpactResult {
    pub requirement_id: String,
    pub affected_requirements: Vec<String>,
    pub affected_files: Vec<PathBuf>,
    pub affected_tests: Vec<String>,
}

/// A validation issue found during traceability check
#[derive(Debug, serde::Serialize)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub message: String,
    pub requirement_id: Option<String>,
}

/// Severity level for validation issues
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Severity {
    /// Traceability broken — must be fixed
    Error,
    /// Traceability incomplete — should be addressed
    Warning,
    /// Informational
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "ERROR"),
            Severity::Warning => write!(f, "WARNING"),
            Severity::Info => write!(f, "INFO"),
        }
    }
}

//! The `ReqEngine` — single public API surface for all engine operations.
//!
//! This is the qualification boundary: all business logic passes through
//! this struct. The CLI and MCP server are thin shells over this API.

use crate::adapter::{JsonAdapter, MarkdownAdapter, RequirementAdapter};
use crate::ai_import::{AiSuggestions, ImportOptions, ImportResult};
use crate::audit::{coverage_map, independence, mutation, triviality};
use crate::cache::Cache;
use crate::config::{self, Config};
use crate::scanner::CodeScanner;
use crate::trace::{ImpactResult, TraceGaps, TraceGraph, ValidationIssue};
use crate::{Error, Result};
use req_lib::{
    AiExport, AuditBundle, AuditCriterion, AUDIT_PROMPT_HINT, AUDIT_SCHEMA_VERSION,
    CodeRef, Coverage, CriteriaReport, CriterionStatus, IndependenceResult,
    Link, LinkType, LineCoverageScore, MutationReport, Requirement, RequirementStatus,
    RequirementType, SourceSpan, TrivialityReport,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Central engine for all req operations.
///
/// Construct with [`ReqEngine::open`] for an initialized project,
/// or use [`ReqEngine::init`] to create a new one.
pub struct ReqEngine {
    /// Project root directory
    pub base: PathBuf,
    /// Loaded configuration
    pub config: Config,
    /// SQLite cache
    cache: Cache,
}

impl ReqEngine {
    /// Open an existing project. Returns `Err(NotInitialized)` if not initialized.
    ///
    /// If `cache.db` is locked by another process (e.g. the MCP server) this
    /// returns `Err(Error::DatabaseLocked)` immediately.  Use
    /// [`ReqEngine::open_with_wait`] to retry for a given number of seconds.
    pub fn open(base: &Path) -> Result<Self> {
        Self::open_impl(base, 0)
    }

    /// Like [`ReqEngine::open`] but retries for up to `wait_secs` seconds when
    /// the database is locked before returning `Err(Error::DatabaseLocked)`.
    // REQ: LLR-0036
    pub fn open_with_wait(base: &Path, wait_secs: u64) -> Result<Self> {
        Self::open_impl(base, (wait_secs * 1000) as u32)
    }

    fn open_impl(base: &Path, busy_timeout_ms: u32) -> Result<Self> {
        if !config::is_initialized(base) {
            return Err(Error::NotInitialized);
        }

        let config = Config::load(&Config::config_path(base))?;
        let cache = Cache::open(&config.cache_path(base), busy_timeout_ms)?;

        Ok(Self {
            base: base.to_path_buf(),
            config,
            cache,
        })
    }

    /// Initialize a new project in `base`, then open it.
    pub fn init(base: &Path, name: Option<&str>) -> Result<()> {
        config::init_project(base, name)
    }

    /// Expose the cache for commands that need direct access.
    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    /// Build the traceability graph from cache.
    pub fn graph(&self) -> Result<TraceGraph> {
        TraceGraph::from_cache(&self.cache)
    }

    // ── Requirements ──────────────────────────────────────────────────────────

    /// Create a new requirement and write it to disk + cache.
    pub fn create_requirement(
        &self,
        req_type: RequirementType,
        title: &str,
        parent: Option<&str>,
        status: RequirementStatus,
    ) -> Result<Requirement> {
        let existing_ids = self.cache.get_all_ids()?;
        let id = Requirement::generate_id(req_type, &existing_ids);

        if let Some(pid) = parent {
            if !self.cache.requirement_exists(pid)? {
                return Err(Error::ParentNotFound(pid.to_string()));
            }
        }

        let mut req = Requirement::new(id, req_type, title.to_string());
        req.parent = parent.map(str::to_string);
        req.status = status;

        let md = MarkdownAdapter::new();
        md.write(&[req.clone()], &self.base)?;
        self.cache.upsert_requirement(&req)?;

        Ok(req)
    }

    /// Remove a requirement from disk and/or the cache.
    ///
    /// * `cache_only` — purge cache entries without deleting the `.md` file.
    /// * `force`      — skip the dependent-check; remove even if children or
    ///                  code references exist.
    ///
    /// Returns `Err(RequirementNotFound)` if the ID is unknown.
    // REQ: LLR-0035
    pub fn remove_requirement(
        &self,
        id: &str,
        cache_only: bool,
        force: bool,
    ) -> Result<RemoveResult> {
        let req = self
            .cache
            .get_requirement(id)?
            .ok_or_else(|| Error::RequirementNotFound(id.to_string()))?;

        let dependents = if force {
            Vec::new()
        } else {
            self.cache.get_dependents(id)?
        };

        self.cache.purge_requirement(id)?;

        let mut file_deleted = false;
        if !cache_only {
            if let Some(source) = &req.source_file {
                if source.exists() {
                    std::fs::remove_file(source)?;
                    file_deleted = true;
                }
            } else {
                // Derive path from the standard layout when source_file is absent
                let derived = crate::adapter::markdown::MarkdownAdapter::get_requirement_path(
                    &self.base,
                    &req,
                );
                if derived.exists() {
                    std::fs::remove_file(&derived)?;
                    file_deleted = true;
                }
            }
        }

        Ok(RemoveResult {
            id: id.to_string(),
            file_deleted,
            dependents_warned: dependents,
        })
    }

    /// List requirements, optionally filtered by type.
    // REQ: LLR-0034
    pub fn list_requirements(
        &self,
        filter: Option<RequirementType>,
    ) -> Result<Vec<Requirement>> {
        let mut reqs = match filter {
            Some(t) => self.cache.get_requirements_by_type(t),
            None => self.cache.get_all_requirements(),
        }?;
        reqs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(reqs)
    }

    // ── Scanning ──────────────────────────────────────────────────────────────

    /// Scan source directories for requirement tags.
    ///
    /// Returns a `ScanResult` with counts — no I/O to the terminal.
    pub fn scan(
        &self,
        source_override: Option<&Path>,
        clear: bool,
    ) -> Result<ScanResult> {
        if clear {
            self.cache.clear_code_refs()?;
        }

        let source_dirs: Vec<PathBuf> = if let Some(s) = source_override {
            vec![s.to_path_buf()]
        } else {
            self.config
                .source_dirs
                .iter()
                .map(|d| self.base.join(d))
                .collect()
        };

        let scanner = CodeScanner::new();
        let mut total_refs = 0usize;
        let mut total_files = 0usize;
        let mut undefined_refs: Vec<String> = Vec::new();
        let mut missing_dirs: Vec<PathBuf> = Vec::new();

        for dir in &source_dirs {
            if !dir.exists() {
                missing_dirs.push(dir.clone());
                continue;
            }

            for (file, tags) in scanner.scan_directory(dir)? {
                total_files += 1;

                let refs = CodeScanner::tags_to_code_refs(&file, &tags);

                for r in &refs {
                    if !self.cache.requirement_exists(&r.req_id)? {
                        undefined_refs.push(r.req_id.clone());
                    }
                }

                for code_ref in &refs {
                    self.cache.insert_code_ref(code_ref)?;
                }

                for tag in &tags {
                    if tag.tag_type == crate::scanner::TagType::Verifies {
                        let link = Link {
                            source: file.to_string_lossy().to_string(),
                            target: tag.req_id.clone(),
                            link_type: LinkType::Verifies,
                        };
                        self.cache.insert_link(&link)?;
                    }
                }

                self.cache.delete_criterion_links_for_file(&file)?;
                for tag in &tags {
                    if tag.tag_type == crate::scanner::TagType::Criterion {
                        if let Some(index) = tag.criterion_index {
                            self.cache.insert_criterion_link(&tag.req_id, index, &file, tag.line)?;
                        }
                    }
                }

                if let Ok(hash) = CodeScanner::hash_file(&file) {
                    self.cache.update_file_hash(&file, &hash)?;
                }

                total_refs += refs.len();
            }
        }

        // REQ: LLR-0037
        // Staleness check: compare each tracked import source against its
        // current on-disk state and update the import_status flag accordingly.
        for src in self.cache.get_all_import_sources()? {
            let status = if !src.source_path.exists() {
                Some("orphaned")
            } else {
                match sha256_file(&src.source_path) {
                    Ok(current_hash) if current_hash == src.sha256 => None,
                    _ => Some("stale"),
                }
            };
            self.cache.set_import_status(&src.req_id, status)?;
        }

        Ok(ScanResult {
            files_scanned: total_files,
            refs_found: total_refs,
            undefined_refs,
            missing_dirs,
        })
    }

    // ── Analysis ──────────────────────────────────────────────────────────────

    /// Calculate coverage statistics.
    // REQ: LLR-0034
    pub fn coverage(&self) -> Result<Coverage> {
        self.cache.calculate_coverage()
    }

    /// Find all traceability gaps (structural only — no import staleness).
    pub fn gaps(&self) -> Result<TraceGaps> {
        let graph = self.graph()?;
        Ok(graph.find_gaps())
    }

    /// Find all traceability gaps including import staleness flags.
    // REQ: LLR-0037
    pub fn gaps_full(&self) -> Result<TraceGaps> {
        let mut gaps = self.gaps()?;
        for (id, status) in self.cache.get_flagged_imports()? {
            match status.as_str() {
                "stale" => gaps.import_stale.push(id),
                "orphaned" => gaps.import_orphaned.push(id),
                _ => {}
            }
        }
        gaps.import_stale.sort();
        gaps.import_orphaned.sort();
        Ok(gaps)
    }

    /// Validate all traceability links, returning issues (including import staleness).
    // REQ: LLR-0037
    pub fn validate(&self) -> Result<Vec<ValidationIssue>> {
        let full_gaps = self.gaps_full()?;
        let graph = self.graph()?;
        // Re-use graph.validate() but inject the import flags from the full gaps
        let mut issues = graph.validate_with_gaps(full_gaps);
        issues.sort_by(|a, b| {
            a.requirement_id.as_deref().unwrap_or("")
                .cmp(b.requirement_id.as_deref().unwrap_or(""))
                .then(a.message.cmp(&b.message))
        });
        Ok(issues)
    }

    /// Trace tree for a requirement.
    pub fn trace_tree(&self, id: &str) -> Result<String> {
        let graph = self.graph()?;
        graph
            .trace_tree(id, 0)
            .ok_or_else(|| Error::RequirementNotFound(id.to_string()))
    }

    /// Impact analysis for a requirement.
    pub fn impact(&self, id: &str) -> Result<ImpactResult> {
        let graph = self.graph()?;
        if !graph.requirements.contains_key(id) {
            return Err(Error::RequirementNotFound(id.to_string()));
        }
        Ok(graph.impact_analysis(id))
    }

    // ── Audit ─────────────────────────────────────────────────────────────────

    /// Run the triviality detector across all code refs for one LLR, or all LLRs.
    // REQ: LLR-0026
    pub fn audit_triviality(&self, req_id: Option<&str>) -> Result<Vec<TrivialityReport>> {
        let refs: Vec<CodeRef> = if let Some(id) = req_id {
            self.cache.get_code_refs_for_requirement(id)?
        } else {
            self.cache.get_all_code_refs()?
        };

        let mut by_req: HashMap<String, Vec<req_lib::TrivialityFinding>> = HashMap::new();

        for cr in &refs {
            let findings = triviality::analyse_code_ref(&cr.file, cr.line, cr.line_end, &cr.req_id)?;
            by_req.entry(cr.req_id.clone()).or_default().extend(findings);
        }

        Ok(by_req
            .into_iter()
            .map(|(req_id, findings)| TrivialityReport { req_id, findings })
            .collect())
    }

    /// Build a criterion linkage report for a single LLR.
    // REQ: LLR-0029
    pub fn audit_criteria(&self, req_id: &str) -> Result<CriteriaReport> {
        let req = self
            .cache
            .get_requirement(req_id)?
            .ok_or_else(|| Error::RequirementNotFound(req_id.to_string()))?;

        // Parse acceptance criteria bullets from the requirement text
        let mut criteria_texts: Vec<String> = Vec::new();
        for line in req.text.lines() {
            let stripped = line.trim();
            if let Some(rest) = stripped.strip_prefix("- [ ]").or_else(|| stripped.strip_prefix("- [x]")).or_else(|| stripped.strip_prefix("- [X]")) {
                criteria_texts.push(rest.trim().to_string());
            }
        }

        let links = self.cache.get_criterion_links(req_id)?;
        let mut warnings: Vec<String> = Vec::new();

        let criteria: Vec<CriterionStatus> = criteria_texts
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let index = i + 1;
                let matching: Vec<_> = links.iter().filter(|(idx, _, _)| *idx == index).collect();
                let linked = !matching.is_empty();
                let test_file = matching.first().map(|(_, f, _)| f.clone());
                let test_line = matching.first().map(|(_, _, l)| *l);
                CriterionStatus { index, text: text.clone(), linked, test_file, test_line }
            })
            .collect();

        // Warn about any link indexes that exceed the criterion count
        let count = criteria_texts.len();
        for (idx, file, line) in &links {
            if *idx > count {
                warnings.push(format!(
                    "CRITERION tag #{}  at {}:{} references index beyond criterion count ({})",
                    idx,
                    file.display(),
                    line,
                    count
                ));
            }
        }

        Ok(CriteriaReport { req_id: req_id.to_string(), criteria, warnings })
    }

    /// Parse a `cargo mutants --json` report and return per-LLR scores.
    // REQ: LLR-0027
    pub fn audit_mutation(&self, report_path: &Path) -> Result<MutationReport> {
        let code_refs = self.build_code_refs_by_req()?;
        mutation::parse_mutants_report(report_path, &code_refs)
    }

    /// Parse a `cargo llvm-cov --json` report and return per-LLR line coverage.
    // REQ: LLR-0028
    pub fn audit_coverage(&self, report_path: &Path) -> Result<Vec<LineCoverageScore>> {
        let code_refs = self.build_code_refs_by_req()?;
        coverage_map::parse_llvm_cov_report(report_path, &code_refs)
    }

    /// Build a full `AuditBundle` for one LLR.
    // REQ: LLR-0030
    pub fn audit_export(
        &self,
        req_id: &str,
        mutation_report: Option<&Path>,
        coverage_report: Option<&Path>,
    ) -> Result<AuditBundle> {
        let llr = self
            .cache
            .get_requirement(req_id)?
            .ok_or_else(|| Error::RequirementNotFound(req_id.to_string()))?;

        let hlr = if let Some(parent_id) = &llr.parent {
            self.cache.get_requirement(parent_id)?
        } else {
            None
        };

        let impl_refs = self.cache.get_code_refs_for_requirement(req_id)?;
        let test_refs = self.cache.get_verifies_refs(req_id)?;

        let implementation_spans = impl_refs.iter().map(|cr| read_span(cr)).collect::<Vec<_>>();
        let test_spans = test_refs.iter().map(|cr| read_span(cr)).collect::<Vec<_>>();

        let triviality_findings = {
            let mut findings = Vec::new();
            for cr in &impl_refs {
                findings.extend(triviality::analyse_code_ref(&cr.file, cr.line, cr.line_end, req_id)?);
            }
            findings
        };

        let criteria_report = self.audit_criteria(req_id)?;
        let acceptance_criteria: Vec<AuditCriterion> = criteria_report
            .criteria
            .iter()
            .map(|cs| {
                let test_locations = cs.test_file.as_ref().map(|f| {
                    vec![SourceSpan {
                        req_id: req_id.to_string(),
                        file: f.clone(),
                        line: cs.test_line.unwrap_or(0),
                        line_end: None,
                        source_text: String::new(),
                        read_warning: None,
                    }]
                }).unwrap_or_default();
                AuditCriterion {
                    index: cs.index,
                    text: cs.text.clone(),
                    linked: cs.linked,
                    test_locations,
                }
            })
            .collect();

        let criterion_coverage = criteria_report.criteria;

        let mutation_score = if let Some(path) = mutation_report {
            let code_refs = self.build_code_refs_by_req()?;
            let report = mutation::parse_mutants_report(path, &code_refs)?;
            report.scores.into_iter().find(|s| s.req_id == req_id)
        } else {
            None
        };

        let line_coverage_score = if let Some(path) = coverage_report {
            let code_refs = self.build_code_refs_by_req()?;
            let scores = coverage_map::parse_llvm_cov_report(path, &code_refs)?;
            scores.into_iter().find(|s| s.req_id == req_id)
        } else {
            None
        };

        let mut warnings = criteria_report.warnings;
        for span in implementation_spans.iter().chain(test_spans.iter()) {
            if let Some(w) = &span.read_warning {
                warnings.push(w.clone());
            }
        }

        Ok(AuditBundle {
            schema_version: AUDIT_SCHEMA_VERSION.to_string(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            llr,
            hlr,
            acceptance_criteria,
            implementation_spans,
            test_spans,
            triviality_findings,
            mutation_score,
            line_coverage_score,
            criterion_coverage,
            prompt_hint: AUDIT_PROMPT_HINT.to_string(),
            warnings,
        })
    }

    /// Check author independence between implementation and test spans for one LLR (or all).
    // REQ: LLR-0031
    pub fn audit_independence(&self, req_id: Option<&str>) -> Result<IndependenceResult> {
        let (impl_refs, test_refs) = if let Some(id) = req_id {
            (self.cache.get_code_refs_for_requirement(id)?, self.cache.get_verifies_refs(id)?)
        } else {
            (self.cache.get_all_code_refs()?, self.cache.get_all_verifies_refs()?)
        };
        independence::check_independence(&self.base, &impl_refs, &test_refs)
    }

    /// Build a `HashMap<req_id, Vec<CodeRef>>` from all cached code refs.
    fn build_code_refs_by_req(&self) -> Result<HashMap<String, Vec<CodeRef>>> {
        let all = self.cache.get_all_code_refs()?;
        let mut map: HashMap<String, Vec<CodeRef>> = HashMap::new();
        for cr in all {
            map.entry(cr.req_id.clone()).or_default().push(cr);
        }
        // Deduplicate: multiple scan runs produce rows with the same (file, line)
        // but different line_end values (e.g. None from an old scan vs Some(x)
        // from a newer scan that added function-body extension).  For each
        // (file, line) key, keep the entry with the widest line_end (Some wins
        // over None; larger end wins over smaller).
        for refs in map.values_mut() {
            // Sort so that entries with Some(line_end) come before None.
            refs.sort_by(|a, b| {
                let key_a = (a.file.to_string_lossy().into_owned(), a.line, a.line_end.unwrap_or(0));
                let key_b = (b.file.to_string_lossy().into_owned(), b.line, b.line_end.unwrap_or(0));
                key_b.cmp(&key_a) // descending: largest line_end first
            });
            let mut seen: std::collections::HashSet<(String, usize)> = std::collections::HashSet::new();
            refs.retain(|cr| seen.insert((cr.file.to_string_lossy().into_owned(), cr.line)));
        }
        Ok(map)
    }

    // ── Export / Import ───────────────────────────────────────────────────────

    /// Export requirements as JSON or Markdown. `"ai-context"` is also accepted (used by MCP).
    // REQ: LLR-0034
    pub fn export(&self, format: &str, id: Option<&str>) -> Result<String> {
        let graph = self.graph()?;

        match format {
            "json" => {
                let mut requirements: Vec<Requirement> = if let Some(req_id) = id {
                    vec![graph
                        .get_requirement(req_id)
                        .ok_or_else(|| Error::RequirementNotFound(req_id.to_string()))?
                        .clone()]
                } else {
                    graph.requirements.values().cloned().collect()
                };
                requirements.sort_by(|a, b| a.id.cmp(&b.id));
                Ok(serde_json::to_string_pretty(&requirements)?)
            }
            "ai-context" | "ai" => {
                let coverage = graph.calculate_coverage();
                let mut requirements: Vec<Requirement> =
                    graph.requirements.values().cloned().collect();
                requirements.sort_by(|a, b| a.id.cmp(&b.id));
                let mut code_refs: Vec<CodeRef> =
                    graph.code_refs.values().flatten().cloned().collect();
                code_refs.sort_by(|a, b| {
                    a.req_id
                        .cmp(&b.req_id)
                        .then(a.file.to_string_lossy().cmp(&b.file.to_string_lossy()))
                        .then(a.line.cmp(&b.line))
                });
                let code_refs: Vec<CodeRef> = code_refs
                    .into_iter()
                    .map(|mut cr| {
                        cr.file = normalize_path(&cr.file, &self.base);
                        cr
                    })
                    .collect();
                let export = AiExport::new(requirements, graph.links, code_refs, coverage);
                Ok(serde_json::to_string_pretty(&export)?)
            }
            "markdown" | "md" => {
                let mut requirements: Vec<Requirement> = if let Some(req_id) = id {
                    vec![graph
                        .get_requirement(req_id)
                        .ok_or_else(|| Error::RequirementNotFound(req_id.to_string()))?
                        .clone()]
                } else {
                    graph.requirements.values().cloned().collect()
                };
                requirements.sort_by(|a, b| a.id.cmp(&b.id));
                let adapter = MarkdownAdapter::new();
                let parts: Vec<String> = requirements
                    .iter()
                    .map(|r| adapter.format_to_string(r))
                    .collect::<Result<Vec<_>>>()?;
                Ok(parts.join("\n\n---\n\n"))
            }
            _ => Err(Error::Config(format!("Unknown export format: {}", format))),
        }
    }

    /// Import requirements from a file.
    // REQ: LLR-0037
    pub fn import(
        &self,
        input: &Path,
        format: &str,
        provenance: Option<&str>,
    ) -> Result<Vec<Requirement>> {
        let requirements = match format {
            "json" => JsonAdapter::new().read(input)?,
            "markdown" | "md" => MarkdownAdapter::new().read(input)?,
            _ => return Err(Error::Config(format!("Unknown import format: {}", format))),
        };

        for req in &requirements {
            let mut req = req.clone();
            if let Some(p) = provenance {
                req.attributes
                    .insert("provenance".to_string(), p.to_string());
            }
            self.cache.upsert_requirement(&req)?;
        }

        let md = MarkdownAdapter::new();
        md.write(&requirements, &self.base)?;

        // Record import source so staleness can be detected on future scans
        let abs_input = if input.is_absolute() {
            input.to_path_buf()
        } else {
            self.base.join(input)
        };
        if let Ok(hash) = sha256_file(&abs_input) {
            for req in &requirements {
                self.cache.upsert_import_source(&req.id, &abs_input, &hash)?;
            }
        }

        Ok(requirements)
    }

    /// Import AI suggestions.
    // REQ: LLR-0037
    pub fn import_ai(
        &self,
        suggestions: AiSuggestions,
        options: ImportOptions,
    ) -> Result<ImportResult> {
        let source_path = options.source_path.clone();
        let result = crate::ai_import::import_suggestions(suggestions, &self.cache, &options)?;

        if !options.dry_run && !result.imported.is_empty() {
            let md = MarkdownAdapter::new();
            md.write(&result.imported, &self.base)?;

            // Record import source for staleness detection
            if let Some(ref path) = source_path {
                let abs = if path.is_absolute() {
                    path.clone()
                } else {
                    self.base.join(path)
                };
                if let Ok(hash) = sha256_file(&abs) {
                    for req in &result.imported {
                        self.cache.upsert_import_source(&req.id, &abs, &hash)?;
                    }
                }
            }
        }

        Ok(result)
    }

    // ── Migrate ────────────────────────────────────────────────────────────────

    /// Re-write all requirement files to the current schema format.
    ///
    /// If `dry_run` is true the files are parsed and validated but not written.
    /// Returns a [`MigrateResult`] describing what was (or would be) changed.
    pub fn migrate(&self, dry_run: bool) -> Result<MigrateResult> {
        let req_dir = self.base.join("requirements");
        if !req_dir.exists() {
            return Err(Error::Config(
                "No requirements/ directory found. Is this a req project?".to_string(),
            ));
        }

        let adapter = MarkdownAdapter::new();
        let requirements = adapter.read(&req_dir)?;

        let mut migrated = Vec::new();
        let mut errors = Vec::new();

        for req in &requirements {
            let path = MarkdownAdapter::get_requirement_path(&self.base, req);

            // Read the on-disk content and the re-serialised form.
            let original = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(format!("{}: {e}", path.display()));
                    continue;
                }
            };
            let rewritten = match adapter.format_to_string(req) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(format!("{}: {e}", path.display()));
                    continue;
                }
            };

            if original != rewritten {
                migrated.push(path.clone());
                if !dry_run {
                    if let Err(e) = std::fs::write(&path, &rewritten) {
                        errors.push(format!("{}: {e}", path.display()));
                    }
                }
            }
        }

        Ok(MigrateResult {
            total: requirements.len(),
            migrated,
            errors,
            dry_run,
        })
    }
}

/// Read source lines for a `CodeRef` into a `SourceSpan`.
fn read_span(cr: &CodeRef) -> SourceSpan {
    let end = cr.line_end.unwrap_or(cr.line);
    match std::fs::read_to_string(&cr.file) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let start_idx = cr.line.saturating_sub(1);
            let end_idx = end.min(lines.len());
            let source_text = lines
                .get(start_idx..end_idx)
                .map(|s| s.join("\n"))
                .unwrap_or_default();
            SourceSpan {
                req_id: cr.req_id.clone(),
                file: cr.file.clone(),
                line: cr.line,
                line_end: cr.line_end,
                source_text,
                read_warning: None,
            }
        }
        Err(e) => SourceSpan {
            req_id: cr.req_id.clone(),
            file: cr.file.clone(),
            line: cr.line,
            line_end: cr.line_end,
            source_text: String::new(),
            read_warning: Some(format!("could not read {}: {e}", cr.file.display())),
        },
    }
}

/// Result of a migrate operation
#[derive(Debug, serde::Serialize)]
pub struct MigrateResult {
    pub total: usize,
    pub migrated: Vec<PathBuf>,
    pub errors: Vec<String>,
    pub dry_run: bool,
}

/// Compute the SHA-256 hex digest of a file's contents.
// REQ: LLR-0037
fn sha256_file(path: &Path) -> Result<String> {
    CodeScanner::hash_file(path).map_err(|_| Error::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("failed to hash {}", path.display()),
    )))
}

/// Return `path` as a forward-slash string relative to `base`.
///
/// Used when serialising file paths to JSON so that output is
/// platform-independent (no Windows backslashes, no absolute prefixes).
// REQ: LLR-0034
fn normalize_path(path: &Path, base: &Path) -> PathBuf {
    let rel = path.strip_prefix(base).unwrap_or(path);
    PathBuf::from(rel.to_string_lossy().replace('\\', "/"))
}

/// Result of a `remove_requirement` operation
#[derive(Debug, serde::Serialize)]
pub struct RemoveResult {
    /// The ID that was removed
    pub id: String,
    /// Whether the on-disk `.md` file was deleted
    pub file_deleted: bool,
    /// Dependents (child IDs or referencing files) that existed at removal time.
    /// Non-empty only when `force = false` and dependents were found — the
    /// caller should surface these as warnings.
    pub dependents_warned: Vec<String>,
}

/// Result of a scan operation
#[derive(Debug, Default, serde::Serialize)]
pub struct ScanResult {
    pub files_scanned: usize,
    pub refs_found: usize,
    pub undefined_refs: Vec<String>,
    pub missing_dirs: Vec<PathBuf>,
}


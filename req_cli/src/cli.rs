//! CLI commands for the req tool.
//!
//! This module is a thin dispatch layer: parse args → call ReqEngine → format output.
//! No business logic lives here.

use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;

use req_engine::ai_import::ImportOptions;
use req_engine::config;
use req_engine::{Error, ReqEngine, RequirementStatus, RequirementType};

use crate::output;

/// Git-native requirement traceability tool
#[derive(Parser, Debug)]
#[command(name = "req", version, about, long_about = None)]
pub struct Cli {
    /// Base directory (defaults to current directory)
    #[arg(short, long, global = true)]
    pub base: Option<PathBuf>,

    /// Output format (text, json)
    #[arg(short, long, global = true, default_value = "text")]
    pub format: String,

    /// Strict mode (exit with error on warnings)
    #[arg(long, global = true)]
    pub strict: bool,

    /// Seconds to wait when cache.db is locked before failing (default: fail immediately)
    #[arg(long, global = true, default_value = "0")]
    pub wait: u64,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize a new requirements project
    Init {
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Create a new requirement
    New {
        /// Requirement type (hlr, llr, tst)
        req_type: String,
        /// Requirement title
        title: String,
        /// Parent requirement ID (for LLR)
        #[arg(short, long)]
        parent: Option<String>,
        /// Status (draft, approved)
        #[arg(short, long, default_value = "draft")]
        status: String,
    },

    /// Scan source code for requirement tags
    Scan {
        #[arg(short, long)]
        source: Option<PathBuf>,
        /// Clear existing code references before scan
        #[arg(long)]
        clear: bool,
    },

    /// Show traceability tree for a requirement
    Trace {
        id: String,
    },

    /// Show coverage statistics
    Coverage,

    /// Show gaps in traceability
    Gaps,

    /// Impact analysis for a requirement
    Impact { id: String },

    /// Validate all traceability links
    Check,

    /// Export requirements
    Export {
        /// Export format (json, markdown)
        #[arg(short, long, default_value = "json")]
        format: String,
        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Export specific requirement by ID
        #[arg(short, long)]
        id: Option<String>,
    },

    /// Import requirements
    Import {
        input: PathBuf,
        /// Import format (json, markdown)
        #[arg(short, long, default_value = "markdown")]
        format: String,
        /// Origin identifier for audit trail (e.g. "DOORS-project-X")
        #[arg(long)]
        provenance: Option<String>,
    },

    /// Import from ReqIF file (requires reqif feature)
    ImportReqif {
        input: PathBuf,
        #[arg(short, long)]
        mapping: Option<PathBuf>,
        /// Origin identifier for audit trail
        #[arg(long)]
        provenance: Option<String>,
    },

    /// Export to ReqIF file (requires reqif feature)
    ExportReqif {
        output: PathBuf,
        #[arg(short, long)]
        mapping: Option<PathBuf>,
    },

    /// List all requirements
    List {
        #[arg(short, long)]
        r#type: Option<String>,
    },

    /// Install or manage git hooks
    Hooks {
        #[command(subcommand)]
        command: HooksCommands,
    },

    /// Generate CI workflow files
    Ci {
        /// CI type (github, gitlab)
        ci_type: String,
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
    },

    /// Import AI-generated requirement suggestions
    ImportAi {
        input: PathBuf,
        /// Preview without writing (dry run)
        #[arg(long)]
        dry_run: bool,
        /// Interactive mode — prompt for each suggestion
        #[arg(long)]
        interactive: bool,
        /// Origin identifier stored as provenance attribute
        #[arg(long)]
        provenance: Option<String>,
    },

    /// Check provenance of all requirements
    CheckProvenance,

    /// Upgrade old requirement files to current schema
    Migrate {
        /// Preview changes without writing files
        #[arg(long)]
        dry_run: bool,
    },

    /// AI agent output integrity auditing
    Audit {
        #[command(subcommand)]
        command: AuditCommands,
    },

    /// Remove a requirement file from disk and purge its cache entries
    Remove {
        /// Requirement ID to remove (e.g. LLR-0042)
        id: String,
        /// Purge cache entries only — do not delete the .md file
        #[arg(long)]
        cache_only: bool,
        /// Skip dependency check (remove even if children or code refs exist)
        #[arg(long)]
        force: bool,
        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum AuditCommands {
    /// Detect hollow/trivial implementations (static analysis, no external tools)
    Triviality {
        /// Restrict to a single LLR ID (default: all LLRs)
        #[arg(short, long)]
        id: Option<String>,
    },

    /// Check acceptance criterion linkage for an LLR
    Criteria {
        /// The LLR to inspect
        id: String,
    },

    /// Import and correlate a `cargo mutants --json` report
    Mutation {
        /// Path to the cargo-mutants JSON output
        #[arg(long)]
        report: PathBuf,
    },

    /// Import and correlate a `cargo llvm-cov --json` coverage report
    Coverage {
        /// Path to the llvm-cov JSON output
        #[arg(long)]
        report: PathBuf,
    },

    /// Export a full audit bundle for one LLR (for LLM review)
    ExportContext {
        /// LLR to bundle
        id: String,
        /// Path to a cargo-mutants JSON report (optional)
        #[arg(long)]
        mutation: Option<PathBuf>,
        /// Path to a llvm-cov JSON report (optional)
        #[arg(long)]
        coverage: Option<PathBuf>,
        /// Output file (stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Check author independence between implementation and test spans
    Independence {
        /// Restrict to a single LLR ID (default: all)
        #[arg(short, long)]
        id: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum HooksCommands {
    /// Install pre-commit hook
    Install {
        /// Use strict mode (block commits with issues)
        #[arg(long)]
        strict: bool,
    },
    /// Remove pre-commit hook
    Uninstall,
}

impl Cli {
    pub fn base_dir(&self) -> PathBuf {
        self.base
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap())
    }

    // REQ: LLR-0006
    pub fn run(&self) -> anyhow::Result<()> {
        let base = self.base_dir();

        let result = match &self.command {
            Commands::Init { name } => self.cmd_init(&base, name.as_deref()),
            Commands::New { req_type, title, parent, status } => {
                self.cmd_new(&base, req_type, title, parent.as_deref(), status)
            }
            Commands::Scan { source, clear } => {
                self.cmd_scan(&base, source.as_deref(), *clear)
            }
            Commands::Trace { id } => self.cmd_trace(&base, id),
            Commands::Coverage => self.cmd_coverage(&base),
            Commands::Gaps => self.cmd_gaps(&base),
            Commands::Impact { id } => self.cmd_impact(&base, id),
            Commands::Check => self.cmd_check(&base),
            Commands::Export { format, output, id } => {
                self.cmd_export(&base, format, output.as_deref(), id.as_deref())
            }
            Commands::Import { input, format, provenance } => {
                self.cmd_import(&base, input, format, provenance.as_deref())
            }
            Commands::ImportReqif { input, mapping, provenance } => {
                self.cmd_import_reqif(&base, input, mapping.as_deref(), provenance.as_deref())
            }
            Commands::ExportReqif { output, mapping } => {
                self.cmd_export_reqif(&base, output, mapping.as_deref())
            }
            Commands::List { r#type } => self.cmd_list(&base, r#type.as_deref()),
            Commands::Hooks { command } => self.cmd_hooks(command),
            Commands::Ci { ci_type, output } => self.cmd_ci(ci_type, output),
            Commands::ImportAi { input, dry_run, interactive, provenance } => {
                self.cmd_import_ai(&base, input, *dry_run, *interactive, provenance.as_deref())
            }
            Commands::CheckProvenance => self.cmd_check_provenance(&base),
            Commands::Migrate { dry_run } => self.cmd_migrate(&base, *dry_run),
            Commands::Audit { command } => self.cmd_audit(&base, command),
            Commands::Remove { id, cache_only, force, yes } => {
                self.cmd_remove(&base, id, *cache_only, *force, *yes)
            }
        };

        match result {
            Ok(()) => Ok(()),
            Err(e) => {
                if self.format == "text" {
                    eprintln!("{}: {}", "Error".red(), e);
                }
                if self.strict {
                    std::process::exit(1);
                }
                Err(e)
            }
        }
    }

    fn cmd_init(&self, base: &std::path::Path, name: Option<&str>) -> anyhow::Result<()> {
        if config::is_initialized(base) {
            println!("Project already initialized at {}", base.display());
            return Ok(());
        }

        ReqEngine::init(base, name)?;

        println!("{}", "✓ Project initialized".green());
        println!("  Created: .req/");
        println!("  Created: requirements/hlr/");
        println!("  Created: requirements/llr/");
        println!("  Created: requirements/tst/");
        println!();
        println!("Next steps:");
        println!("  req new hlr \"First requirement\"");
        Ok(())
    }

    fn cmd_new(
        &self,
        base: &std::path::Path,
        req_type_str: &str,
        title: &str,
        parent: Option<&str>,
        status: &str,
    ) -> anyhow::Result<()> {
        let engine = ReqEngine::open_with_wait(base, self.wait)?;

        let req_type = RequirementType::from_str(req_type_str)
            .ok_or_else(|| Error::InvalidRequirementType(req_type_str.to_string()))?;

        let status = RequirementStatus::from_str(status)
            .ok_or_else(|| Error::Config(format!("Invalid status: {}", status)))?;

        let req = engine.create_requirement(req_type, title, parent, status)?;

        println!("{} {} created", "✓".green(), req.id);

        if req_type == RequirementType::Llr && parent.is_none() {
            println!("{} Consider adding --parent <HLR-xxxx>", "Note:".yellow());
        }

        Ok(())
    }

    // REQ: LLR-0035
    fn cmd_remove(
        &self,
        base: &std::path::Path,
        id: &str,
        cache_only: bool,
        force: bool,
        yes: bool,
    ) -> anyhow::Result<()> {
        if !yes {
            let target = if cache_only {
                format!("Purge '{}' from cache (file kept on disk)?", id)
            } else {
                format!("Remove '{}' from disk and cache?", id)
            };
            eprint!("{} [y/N] ", target);
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Aborted.");
                return Ok(());
            }
        }

        let engine = ReqEngine::open_with_wait(base, self.wait)?;
        let result = engine.remove_requirement(id, cache_only, force)?;

        if !result.dependents_warned.is_empty() {
            for dep in &result.dependents_warned {
                eprintln!("{} '{}' has dependent: {}", "Warning:".yellow(), id, dep);
            }
            if self.strict {
                anyhow::bail!("Removal aborted: dependents exist (use --force to override)");
            }
        }

        if result.file_deleted {
            println!("{} {} removed from disk and cache", "✓".green(), id);
        } else if cache_only {
            println!("{} {} purged from cache (file kept)", "✓".green(), id);
        } else {
            println!("{} {} purged from cache (file not found on disk)", "✓".green(), id);
        }

        Ok(())
    }

    fn cmd_scan(
        &self,
        base: &std::path::Path,
        source: Option<&std::path::Path>,
        clear: bool,
    ) -> anyhow::Result<()> {
        let engine = ReqEngine::open_with_wait(base, self.wait)?;

        if let Some(dir) = source {
            println!("Scanning {}...", dir.display());
        } else {
            println!("Scanning source directories...");
        }

        let result = engine.scan(source, clear)?;
        output::print_scan_result(&result);
        Ok(())
    }

    fn cmd_trace(&self, base: &std::path::Path, id: &str) -> anyhow::Result<()> {
        let engine = ReqEngine::open_with_wait(base, self.wait)?;
        let tree = engine.trace_tree(id)?;
        println!("{}", tree);
        Ok(())
    }

    // REQ: LLR-0018
    fn cmd_coverage(&self, base: &std::path::Path) -> anyhow::Result<()> {
        let engine = ReqEngine::open_with_wait(base, self.wait)?;
        let coverage = engine.coverage()?;

        if self.format == "json" {
            println!("{}", serde_json::to_string_pretty(&coverage)?);
            return Ok(());
        }

        println!("Coverage Report");
        println!("═══════════════");
        println!();
        println!("High-Level Requirements:");
        println!("  Total:      {}", coverage.hlr_total);
        println!(
            "  With LLR:   {} ({:.1}%)",
            coverage.hlr_with_llr,
            coverage.hlr_coverage_percent()
        );
        println!();
        println!("Low-Level Requirements:");
        println!("  Total:       {}", coverage.llr_total);
        println!(
            "  Implemented: {} ({:.1}%)",
            coverage.llr_implemented,
            coverage.llr_implementation_percent()
        );
        println!(
            "  Tested:      {} ({:.1}%)",
            coverage.llr_tested,
            coverage.llr_test_percent()
        );

        Ok(())
    }

    fn cmd_gaps(&self, base: &std::path::Path) -> anyhow::Result<()> {
        let engine = ReqEngine::open_with_wait(base, self.wait)?;
        let gaps = engine.gaps_full()?;
        let llrs_without_tests = engine.cache().get_llrs_without_tests()?;

        println!("Traceability Gaps");
        println!("═════════════════");
        println!();

        if !gaps.hlr_without_llr.is_empty() {
            println!("HLR without LLR children:");
            for id in &gaps.hlr_without_llr {
                println!("  - {}", id);
            }
            println!();
        }

        if !gaps.llr_without_parent.is_empty() {
            println!("LLR without parent HLR:");
            for id in &gaps.llr_without_parent {
                println!("  - {}", id);
            }
            println!();
        }

        if !gaps.llr_missing_parent.is_empty() {
            println!("LLR with missing parent:");
            for id in &gaps.llr_missing_parent {
                println!("  - {}", id);
            }
            println!();
        }

        if !gaps.llr_without_code.is_empty() {
            println!("LLR without code implementation:");
            for id in &gaps.llr_without_code {
                println!("  - {}", id);
            }
            println!();
        }

        if !gaps.undefined_ids.is_empty() {
            println!("Undefined requirement IDs:");
            for id in &gaps.undefined_ids {
                println!("  - {}", id);
            }
            println!();
        }

        if !llrs_without_tests.is_empty() {
            println!("LLR without test coverage ({}):", llrs_without_tests.len());
            for id in &llrs_without_tests {
                println!("  - {}", id);
            }
        }

        // REQ: LLR-0037
        if !gaps.import_stale.is_empty() {
            println!("Imported requirements with changed source:");
            for id in &gaps.import_stale {
                println!("  - {} (import_stale)", id);
            }
            println!();
        }

        // REQ: LLR-0037
        if !gaps.import_orphaned.is_empty() {
            println!("Imported requirements with deleted source:");
            for id in &gaps.import_orphaned {
                println!("  - {} (import_orphaned)", id);
            }
            println!();
        }

        if gaps.is_empty() && llrs_without_tests.is_empty() {
            println!("{}", "✓ No gaps found".green());
        }

        Ok(())
    }

    // REQ: LLR-0020
    fn cmd_impact(&self, base: &std::path::Path, id: &str) -> anyhow::Result<()> {
        let engine = ReqEngine::open_with_wait(base, self.wait)?;
        let impact = engine.impact(id)?;

        println!("Impact Analysis for {}", id);
        println!("═════════════════════════");
        println!();
        println!("Affected requirements ({}):", impact.affected_requirements.len());
        for r in &impact.affected_requirements {
            println!("  - {}", r);
        }
        println!();
        println!("Affected files ({}):", impact.affected_files.len());
        for f in &impact.affected_files {
            println!("  - {}", f.display());
        }
        println!();
        println!("Affected tests ({}):", impact.affected_tests.len());
        for t in &impact.affected_tests {
            println!("  - {}", t);
        }

        Ok(())
    }

    fn cmd_check(&self, base: &std::path::Path) -> anyhow::Result<()> {
        let engine = ReqEngine::open_with_wait(base, self.wait)?;
        let issues = engine.validate()?;

        if issues.is_empty() {
            println!("{}", "✓ All checks passed".green());
            return Ok(());
        }

        use req_engine::trace::Severity;

        let errors = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count();
        let warnings = issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .count();

        println!("Validation Report");
        println!("═════════════════");
        println!();

        for issue in &issues {
            let prefix = match issue.severity {
                Severity::Error => "ERROR".red(),
                Severity::Warning => "WARNING".yellow(),
                Severity::Info => "INFO".cyan(),
            };
            println!("[{}] {}", prefix, issue.message);
        }

        println!();
        println!("Summary: {} errors, {} warnings", errors, warnings);

        if errors > 0 || (self.strict && warnings > 0) {
            std::process::exit(1);
        }

        Ok(())
    }

    // REQ: LLR-0017
    fn cmd_export(
        &self,
        base: &std::path::Path,
        format: &str,
        output: Option<&std::path::Path>,
        id: Option<&str>,
    ) -> anyhow::Result<()> {
        let engine = ReqEngine::open_with_wait(base, self.wait)?;
        let content = engine.export(format, id)?;

        if let Some(path) = output {
            std::fs::write(path, &content)?;
            println!("Exported to {}", path.display());
        } else {
            println!("{}", content);
        }

        Ok(())
    }

    fn cmd_import(
        &self,
        base: &std::path::Path,
        input: &std::path::Path,
        format: &str,
        provenance: Option<&str>,
    ) -> anyhow::Result<()> {
        let engine = ReqEngine::open_with_wait(base, self.wait)?;
        let requirements = engine.import(input, format, provenance)?;
        println!("{} Imported {} requirements", "✓".green(), requirements.len());
        Ok(())
    }

    #[cfg(feature = "reqif")]
    fn cmd_import_reqif(
        &self,
        base: &std::path::Path,
        input: &std::path::Path,
        mapping: Option<&std::path::Path>,
        provenance: Option<&str>,
    ) -> anyhow::Result<()> {
        use req_engine::adapter::reqif::{ReqIfAdapter, ReqIfMapping};
        use req_engine::adapter::RequirementAdapter;

        let engine = ReqEngine::open_with_wait(base, self.wait)?;

        let reqif_mapping = if let Some(mapping_path) = mapping {
            let content = std::fs::read_to_string(mapping_path)?;
            serde_yaml::from_str(&content)
                .map_err(|e| req_engine::Error::Parse(format!("Mapping parse error: {}", e)))?
        } else {
            ReqIfMapping::default()
        };

        let adapter = ReqIfAdapter::with_mapping(reqif_mapping);
        let mut requirements = adapter.read(input)?;

        if let Some(p) = provenance {
            for req in &mut requirements {
                req.attributes
                    .insert("provenance".to_string(), p.to_string());
            }
        }

        for req in &requirements {
            engine.cache().upsert_requirement(req)?;
        }

        use req_engine::adapter::MarkdownAdapter;
        MarkdownAdapter::new().write(&requirements, base)?;

        println!(
            "{} Imported {} requirements from ReqIF",
            "✓".green(),
            requirements.len()
        );
        Ok(())
    }

    #[cfg(not(feature = "reqif"))]
    fn cmd_import_reqif(
        &self,
        _base: &std::path::Path,
        _input: &std::path::Path,
        _mapping: Option<&std::path::Path>,
        _provenance: Option<&str>,
    ) -> anyhow::Result<()> {
        Err(req_engine::Error::Config(
            "ReqIF support not enabled. Recompile with --features reqif".to_string(),
        )
        .into())
    }

    #[cfg(feature = "reqif")]
    fn cmd_export_reqif(
        &self,
        base: &std::path::Path,
        output: &std::path::Path,
        mapping: Option<&std::path::Path>,
    ) -> anyhow::Result<()> {
        use req_engine::adapter::reqif::{ReqIfAdapter, ReqIfMapping};
        use req_engine::adapter::RequirementAdapter;

        let engine = ReqEngine::open_with_wait(base, self.wait)?;

        let reqif_mapping = if let Some(mapping_path) = mapping {
            let content = std::fs::read_to_string(mapping_path)?;
            serde_yaml::from_str(&content)
                .map_err(|e| req_engine::Error::Parse(format!("Mapping parse error: {}", e)))?
        } else {
            ReqIfMapping::default()
        };

        let requirements = engine.cache().get_all_requirements()?;
        let adapter = ReqIfAdapter::with_mapping(reqif_mapping);
        adapter.write(&requirements, output)?;

        println!(
            "{} Exported {} requirements to {}",
            "✓".green(),
            requirements.len(),
            output.display()
        );
        Ok(())
    }

    #[cfg(not(feature = "reqif"))]
    fn cmd_export_reqif(
        &self,
        _base: &std::path::Path,
        _output: &std::path::Path,
        _mapping: Option<&std::path::Path>,
    ) -> anyhow::Result<()> {
        Err(req_engine::Error::Config(
            "ReqIF support not enabled. Recompile with --features reqif".to_string(),
        )
        .into())
    }

    fn cmd_list(&self, base: &std::path::Path, filter_type: Option<&str>) -> anyhow::Result<()> {
        let engine = ReqEngine::open_with_wait(base, self.wait)?;

        let filter = filter_type
            .map(|t| {
                RequirementType::from_str(t)
                    .ok_or_else(|| Error::InvalidRequirementType(t.to_string()))
            })
            .transpose()?;

        let requirements = engine.list_requirements(filter)?;

        if requirements.is_empty() {
            println!("No requirements found.");
            return Ok(());
        }

        // REQ: LLR-0037
        let flagged: std::collections::HashMap<String, String> = engine
            .cache()
            .get_flagged_imports()
            .unwrap_or_default()
            .into_iter()
            .collect();

        println!("Requirements ({})", requirements.len());
        println!("════════════════════════");
        println!();

        for req in &requirements {
            let import_tag = if let Some(status) = flagged.get(&req.id) {
                format!(" [{}]", status)
            } else {
                String::new()
            };
            println!(
                "{} [{}] {}{}",
                req.id,
                output::colored_status(&req.status),
                req.title,
                import_tag,
            );
        }

        Ok(())
    }

    fn cmd_hooks(&self, command: &HooksCommands) -> anyhow::Result<()> {
        match command {
            HooksCommands::Install { strict } => crate::hooks::install_hook(*strict)?,
            HooksCommands::Uninstall => crate::hooks::uninstall_hook()?,
        }
        Ok(())
    }

    fn cmd_ci(&self, ci_type: &str, output: &std::path::Path) -> anyhow::Result<()> {
        crate::hooks::generate_ci_workflow(ci_type, output)?;
        Ok(())
    }

    fn cmd_import_ai(
        &self,
        base: &std::path::Path,
        input: &std::path::Path,
        dry_run: bool,
        interactive: bool,
        provenance: Option<&str>,
    ) -> anyhow::Result<()> {
        let engine = ReqEngine::open_with_wait(base, self.wait)?;

        let mut suggestions = req_engine::ai_import::load_suggestions(input)?;

        // Interactive mode: filter suggestions before import
        if interactive {
            suggestions.suggestions = self.interactive_filter(suggestions.suggestions)?;
        }

        let options = ImportOptions {
            dry_run,
            provenance: provenance.map(str::to_string),
            source_path: Some(input.to_path_buf()),
        };

        let result = engine.import_ai(suggestions, options)?;
        output::print_ai_import_summary(&result, dry_run);

        if self.strict && !result.errors.is_empty() {
            std::process::exit(1);
        }

        Ok(())
    }

    /// Prompt user to accept/reject each suggestion interactively
    fn interactive_filter(
        &self,
        suggestions: Vec<req_engine::ai_import::AiSuggestion>,
    ) -> anyhow::Result<Vec<req_engine::ai_import::AiSuggestion>> {
        use std::io::Write;

        let mut accepted = Vec::new();

        for suggestion in suggestions {
            println!("\n{}", "─".repeat(60));
            println!("Type:  {}", suggestion.req_type);
            println!("Title: {}", suggestion.title);
            if let Some(ref p) = suggestion.parent {
                println!("Parent: {}", p);
            }
            if !suggestion.text.is_empty() {
                println!(
                    "Text:  {}",
                    suggestion.text.lines().next().unwrap_or("")
                );
            }
            println!("{}", "─".repeat(60));

            print!("Import? [y/n/q(uit)]: ");
            std::io::stdout().flush()?;

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;

            match input.trim().to_lowercase().as_str() {
                "y" | "yes" => accepted.push(suggestion),
                "q" | "quit" => break,
                _ => {} // skip
            }
        }

        Ok(accepted)
    }

    // REQ: LLR-0032
    fn cmd_audit(&self, base: &std::path::Path, command: &AuditCommands) -> anyhow::Result<()> {
        let engine = ReqEngine::open_with_wait(base, self.wait)?;

        match command {
            AuditCommands::Triviality { id } => {
                let reports = engine.audit_triviality(id.as_deref())?;
                if self.format == "json" {
                    println!("{}", serde_json::to_string_pretty(&reports)?);
                    return Ok(());
                }
                output::print_triviality_reports(&reports);
            }

            AuditCommands::Criteria { id } => {
                let report = engine.audit_criteria(id)?;
                if self.format == "json" {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                    return Ok(());
                }
                output::print_criteria_report(&report);
            }

            AuditCommands::Mutation { report } => {
                let result = engine.audit_mutation(report)?;
                if self.format == "json" {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                    return Ok(());
                }
                output::print_mutation_report(&result);
            }

            AuditCommands::Coverage { report } => {
                let scores = engine.audit_coverage(report)?;
                if self.format == "json" {
                    println!("{}", serde_json::to_string_pretty(&scores)?);
                    return Ok(());
                }
                output::print_coverage_scores(&scores);
            }

            AuditCommands::ExportContext { id, mutation, coverage, output } => {
                let bundle = engine.audit_export(
                    id,
                    mutation.as_deref(),
                    coverage.as_deref(),
                )?;
                let json = serde_json::to_string_pretty(&bundle)?;
                if let Some(path) = output {
                    std::fs::write(path, &json)?;
                    println!("{} Audit bundle written to {}", "✓".green(), path.display());
                } else {
                    println!("{}", json);
                }
            }

            AuditCommands::Independence { id } => {
                let result = engine.audit_independence(id.as_deref())?;
                if self.format == "json" {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                    return Ok(());
                }
                output::print_independence_result(&result);
            }
        }

        Ok(())
    }

    fn cmd_check_provenance(&self, base: &std::path::Path) -> anyhow::Result<()> {
        let engine = ReqEngine::open_with_wait(base, self.wait)?;
        let violations =
            req_engine::provenance::check_all_provenance(base, engine.cache())?;

        if violations.is_empty() {
            println!("{}", "✓ All requirements have valid provenance".green());
            return Ok(());
        }

        println!("Provenance Violations");
        println!("═════════════════════");
        println!();

        for v in &violations {
            println!("{} {}: {}", "✗".red(), v.req_id, v.reason);
        }

        println!();
        println!("Total: {} violations", violations.len());

        if self.strict {
            std::process::exit(1);
        }

        Ok(())
    }

    // REQ: LLR-0016
    fn cmd_migrate(&self, base: &std::path::Path, dry_run: bool) -> anyhow::Result<()> {
        let engine = ReqEngine::open_with_wait(base, self.wait)?;
        let result = engine.migrate(dry_run)?;

        if self.format == "json" {
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }

        let label = if dry_run { "Dry run: " } else { "" };

        if result.migrated.is_empty() && result.errors.is_empty() {
            println!(
                "{} {}All {} requirement files are up to date",
                "✓".green(),
                label,
                result.total
            );
            return Ok(());
        }

        for path in &result.migrated {
            let action = if dry_run { "would migrate" } else { "migrated" };
            println!("{} {}: {}", "→".cyan(), action, path.display());
        }

        for err in &result.errors {
            println!("{} Error: {}", "✗".red(), err);
        }

        println!();
        println!(
            "{}Total: {}/{} files {}",
            label,
            result.migrated.len(),
            result.total,
            if dry_run { "would be updated" } else { "updated" }
        );

        if !result.errors.is_empty() && self.strict {
            std::process::exit(1);
        }

        Ok(())
    }
}

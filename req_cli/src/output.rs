//! Terminal output helpers for the req CLI.

use colored::Colorize;
use req_engine::ai_import::ImportResult;
use req_engine::{
    CriteriaReport, IndependenceResult, LineCoverageScore, MutationReport,
    RequirementStatus, ScanResult, TrivialityReport,
};
use req_engine::FindingSeverity;

/// Print the result of a scan operation
pub fn print_scan_result(result: &ScanResult) {
    println!();
    println!("{}", "✓ Scan complete".green());
    println!("  Files scanned:         {}", result.files_scanned);
    println!("  Code references found: {}", result.refs_found);

    if !result.missing_dirs.is_empty() {
        println!();
        for dir in &result.missing_dirs {
            println!(
                "{} Source directory does not exist: {}",
                "Warning:".yellow(),
                dir.display()
            );
        }
    }

    if !result.undefined_refs.is_empty() {
        println!();
        println!(
            "{} {} undefined requirements referenced:",
            "Warning:".yellow(),
            result.undefined_refs.len()
        );
        for id in result.undefined_refs.iter().take(10) {
            println!("  - {}", id);
        }
        if result.undefined_refs.len() > 10 {
            println!("  ... and {} more", result.undefined_refs.len() - 10);
        }
    }
}

/// Print a summary of an AI import operation
pub fn print_ai_import_summary(result: &ImportResult, dry_run: bool) {
    println!();
    if dry_run {
        println!("{}", "DRY RUN — No changes written".yellow());
    }

    println!("AI Import Summary");
    println!("═════════════════");
    println!();

    if !result.imported.is_empty() {
        println!("{} Imported (as draft):", "✓".green());
        for req in &result.imported {
            println!("  - {} [draft] {}", req.id, req.title);
        }
        println!();
    }

    if !result.skipped.is_empty() {
        println!("{} Skipped:", "!".yellow());
        for (id, reason) in &result.skipped {
            println!("  - {}: {}", id, reason);
        }
        println!();
    }

    if !result.errors.is_empty() {
        println!("{} Errors:", "✗".red());
        for err in &result.errors {
            println!("  - {}", err);
        }
        println!();
    }

    println!(
        "Total: {} imported, {} skipped, {} errors",
        result.imported.len(),
        result.skipped.len(),
        result.errors.len()
    );

    if !result.imported.is_empty() && !dry_run {
        println!();
        println!(
            "{}",
            "⚠ All imported requirements are DRAFT — review and approve manually".yellow()
        );
    }
}

/// Print triviality analysis reports.
pub fn print_triviality_reports(reports: &[TrivialityReport]) {
    let total_findings: usize = reports.iter().map(|r| r.findings.len()).sum();
    if total_findings == 0 {
        println!("{}", "✓ No triviality findings".green());
        return;
    }

    println!("Triviality Analysis");
    println!("═══════════════════");
    println!();

    for report in reports {
        if report.findings.is_empty() {
            continue;
        }
        println!("{}:", report.req_id);
        for f in &report.findings {
            let sev = match f.severity {
                FindingSeverity::Error => "ERROR".red(),
                FindingSeverity::Warning => "WARNING".yellow(),
                FindingSeverity::Info => "INFO".cyan(),
            };
            println!(
                "  [{}] {}:{} — {} — \"{}\"",
                sev,
                f.file.display(),
                f.line,
                f.pattern.description(),
                f.matched_text.trim()
            );
        }
        println!();
    }

    println!("Total findings: {}", total_findings);
}

/// Print criterion linkage report.
pub fn print_criteria_report(report: &CriteriaReport) {
    println!("Criterion Linkage — {}", report.req_id);
    println!("═══════════════════════════════");
    println!();

    if report.criteria.is_empty() {
        println!("  (no acceptance criteria found in requirement text)");
    }

    for cs in &report.criteria {
        let linked_flag = if cs.linked {
            "✓".green().to_string()
        } else {
            "✗".red().to_string()
        };
        let loc = cs
            .test_file
            .as_ref()
            .map(|f| format!(" → {}:{}", f.display(), cs.test_line.unwrap_or(0)))
            .unwrap_or_default();
        println!("  #{} [{}] {}{}", cs.index, linked_flag, cs.text, loc);
    }

    if !report.warnings.is_empty() {
        println!();
        for w in &report.warnings {
            println!("{} {}", "Warning:".yellow(), w);
        }
    }
}

/// Print mutation testing report.
pub fn print_mutation_report(report: &MutationReport) {
    println!("Mutation Testing Report");
    println!("═══════════════════════");
    println!();

    if report.scores.is_empty() {
        println!("  No LLR-tagged mutants found.");
    }

    for score in &report.scores {
        let pct = score
            .score_percent
            .map(|p| format!("{:.1}%", p))
            .unwrap_or_else(|| "N/A".to_string());
        println!(
            "  {} — {}/{} caught ({})",
            score.req_id, score.caught, score.mutants_total, pct
        );
    }

    if report.untagged_mutants > 0 {
        println!();
        println!(
            "{} {} mutants outside any tagged span",
            "Note:".yellow(),
            report.untagged_mutants
        );
    }
}

/// Print line coverage scores.
pub fn print_coverage_scores(scores: &[LineCoverageScore]) {
    if scores.is_empty() {
        println!("Line Coverage");
        println!("═════════════");
        println!();
        println!("  No coverage data matched tagged spans.");
        return;
    }

    // Deduplicate by (req_id, file) — prefer entries with more lines_total (wider
    // measurement window beats old 1-line entries from stale DB rows).
    let mut best: std::collections::HashMap<(String, std::path::PathBuf), &LineCoverageScore> =
        std::collections::HashMap::new();
    for s in scores {
        let key = (s.req_id.clone(), s.file.clone());
        let keep = best.get(&key).map(|prev| s.lines_total > prev.lines_total).unwrap_or(true);
        if keep {
            best.insert(key, s);
        }
    }
    let mut unique: Vec<&LineCoverageScore> = best.into_values().collect();

    // Sort: primary = req_id prefix (LLR/TST/…), secondary = numeric suffix.
    let num_suffix = |id: &str| -> u32 {
        id.splitn(2, '-').nth(1).and_then(|n| n.parse().ok()).unwrap_or(0)
    };
    unique.sort_by(|a, b| {
        let pa = &a.req_id[..a.req_id.find('-').unwrap_or(a.req_id.len())];
        let pb = &b.req_id[..b.req_id.find('-').unwrap_or(b.req_id.len())];
        pa.cmp(pb).then(num_suffix(&a.req_id).cmp(&num_suffix(&b.req_id)))
    });

    let fmt_score = |s: &LineCoverageScore| -> String {
        let pct = s
            .hit_percent
            .map(|p| format!("{:.1}%", p))
            .unwrap_or_else(|| "N/A".to_string());
        format!(
            "  {} — {}/{} lines hit ({}) — {}",
            s.req_id,
            s.lines_hit,
            s.lines_total,
            pct,
            s.file.display()
        )
    };

    let llr: Vec<_> = unique.iter().filter(|s| s.req_id.starts_with("LLR-")).collect();
    let tst: Vec<_> = unique.iter().filter(|s| s.req_id.starts_with("TST-")).collect();
    let other: Vec<_> = unique
        .iter()
        .filter(|s| !s.req_id.starts_with("LLR-") && !s.req_id.starts_with("TST-"))
        .collect();

    if !llr.is_empty() {
        println!("Line Coverage by LLR");
        println!("════════════════════");
        println!();
        for s in &llr {
            println!("{}", fmt_score(s));
        }
        println!();
    }
    if !tst.is_empty() {
        println!("Line Coverage by TST");
        println!("════════════════════");
        println!();
        for s in &tst {
            println!("{}", fmt_score(s));
        }
        println!();
    }
    if !other.is_empty() {
        println!("Line Coverage (other)");
        println!("═════════════════════");
        println!();
        for s in &other {
            println!("{}", fmt_score(s));
        }
        println!();
    }
}

/// Print author independence result.
pub fn print_independence_result(result: &IndependenceResult) {
    if result.violations.is_empty() && result.warnings.is_empty() {
        println!("{}", "✓ No independence violations".green());
        return;
    }

    println!("Independence Check");
    println!("══════════════════");
    println!();

    for v in &result.violations {
        println!("{} {} — shared identities: {}", "VIOLATION".red(), v.req_id, v.shared.join(", "));
        println!("  impl authors: {}", v.impl_identities.join(", "));
        println!("  test authors: {}", v.test_identities.join(", "));
        println!();
    }

    for w in &result.warnings {
        println!("{} {} — {}", "Warning:".yellow(), w.req_id, w.reason);
    }
}

/// Format a requirement status with color
pub fn colored_status(status: &RequirementStatus) -> colored::ColoredString {
    match status {
        RequirementStatus::Approved => "approved".green(),
        RequirementStatus::Draft => "draft".yellow(),
        RequirementStatus::Deprecated => "deprecated".dimmed(),
        RequirementStatus::Rejected => "rejected".red(),
    }
}

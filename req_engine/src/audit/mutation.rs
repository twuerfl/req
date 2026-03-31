// REQ: LLR-0027
//! Mutation testing adapter — parses `cargo mutants --json` output.
//!
//! `req` never invokes `cargo mutants` itself; the user runs it separately
//! and passes the JSON report file to `req audit mutation --report PATH`.

use crate::{Error, Result};
use req_lib::{CodeRef, MutationReport, MutationScore};
use std::collections::HashMap;
use std::path::Path;

/// Parse a `cargo mutants --json` report and correlate scores to LLR tags.
///
/// Returns a `MutationReport` containing per-LLR scores and the count of
/// mutants that did not overlap any tagged `CodeRef` range.
pub fn parse_mutants_report(
    json_path: &Path,
    code_refs: &HashMap<String, Vec<CodeRef>>,
) -> Result<MutationReport> {
    let content = std::fs::read_to_string(json_path).map_err(Error::Io)?;
    let records: Vec<serde_json::Value> = serde_json::from_str(&content)
        .map_err(|e| Error::Parse(format!("cargo mutants JSON parse error: {e}")))?;

    // Accumulate per-req_id counters
    let mut totals: HashMap<String, (usize, usize, usize)> = HashMap::new(); // (total, caught, missed)
    let mut untagged: usize = 0;

    for record in &records {
        let file_raw = record
            .get("file")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let line = record
            .get("line")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let outcome = record
            .get("outcome")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");

        let norm_file = normalise_path(file_raw);

        let mut matched = false;
        for (req_id, refs) in code_refs {
            for cr in refs {
                let cr_file = normalise_path(&cr.file.to_string_lossy());
                let end = cr.line_end.unwrap_or(cr.line + 1);
                if cr_file == norm_file && line >= cr.line && line < end {
                    let entry = totals.entry(req_id.clone()).or_default();
                    entry.0 += 1; // total
                    match outcome {
                        "Caught" => entry.1 += 1,
                        "Missed" => entry.2 += 1,
                        _ => {} // Unviable / Timeout — counted in total only
                    }
                    matched = true;
                }
            }
        }
        if !matched {
            untagged += 1;
        }
    }

    let scores = totals
        .into_iter()
        .map(|(req_id, (total, caught, missed))| {
            let score_percent = if caught + missed > 0 {
                Some(caught as f64 / (caught + missed) as f64 * 100.0)
            } else {
                None
            };
            MutationScore {
                req_id,
                mutants_total: total,
                caught,
                missed,
                score_percent,
            }
        })
        .collect();

    Ok(MutationReport {
        scores,
        untagged_mutants: untagged,
    })
}

fn normalise_path(raw: &str) -> String {
    raw.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use req_lib::CodeRef;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn write_json(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    fn make_refs(req_id: &str, file: &str, line: usize, line_end: usize) -> HashMap<String, Vec<CodeRef>> {
        let mut m = HashMap::new();
        m.insert(
            req_id.to_string(),
            vec![CodeRef {
                req_id: req_id.to_string(),
                file: PathBuf::from(file),
                line,
                line_end: Some(line_end),
                hash: None,
                symbol: None,
            }],
        );
        m
    }

    #[test]
    fn parses_valid_json_and_counts() {
        let json = r#"[
            {"file":"src/foo.rs","line":10,"kind":"Replace","outcome":"Caught"},
            {"file":"src/foo.rs","line":11,"kind":"Replace","outcome":"Missed"}
        ]"#;
        let f = write_json(json);
        let refs = make_refs("LLR-0001", "src/foo.rs", 8, 15);
        let report = parse_mutants_report(f.path(), &refs).unwrap();
        let score = report.scores.iter().find(|s| s.req_id == "LLR-0001").unwrap();
        assert_eq!(score.caught, 1);
        assert_eq!(score.missed, 1);
        assert_eq!(score.mutants_total, 2);
        assert!((score.score_percent.unwrap() - 50.0).abs() < 0.01);
    }

    #[test]
    fn unmatched_mutant_goes_to_untagged() {
        let json = r#"[{"file":"src/other.rs","line":100,"outcome":"Caught"}]"#;
        let f = write_json(json);
        let refs = make_refs("LLR-0001", "src/foo.rs", 1, 10);
        let report = parse_mutants_report(f.path(), &refs).unwrap();
        assert_eq!(report.untagged_mutants, 1);
        assert!(report.scores.is_empty());
    }

    #[test]
    fn unviable_counts_in_total_not_caught_or_missed() {
        let json = r#"[{"file":"src/foo.rs","line":5,"outcome":"Unviable"}]"#;
        let f = write_json(json);
        let refs = make_refs("LLR-0001", "src/foo.rs", 1, 10);
        let report = parse_mutants_report(f.path(), &refs).unwrap();
        let score = report.scores.iter().find(|s| s.req_id == "LLR-0001").unwrap();
        assert_eq!(score.mutants_total, 1);
        assert_eq!(score.caught, 0);
        assert_eq!(score.missed, 0);
        assert!(score.score_percent.is_none());
    }

    #[test]
    fn malformed_json_returns_parse_error() {
        let f = write_json("{ not valid json }");
        let refs = HashMap::new();
        let result = parse_mutants_report(f.path(), &refs);
        assert!(result.is_err());
    }
}

// REQ: LLR-0028
//! Line coverage adapter — parses `cargo llvm-cov --json` output.
//!
//! `req` never invokes `cargo llvm-cov` itself; the user runs it separately
//! and passes the JSON report file to `req audit coverage --report PATH`.

use crate::{Error, Result};
use req_lib::{CodeRef, LineCoverageScore};
use std::collections::HashMap;
use std::path::Path;

/// Parse a `cargo llvm-cov --json` report and compute per-LLR line hit scores.
pub fn parse_llvm_cov_report(
    json_path: &Path,
    code_refs: &HashMap<String, Vec<CodeRef>>,
) -> Result<Vec<LineCoverageScore>> {
    let content = std::fs::read_to_string(json_path).map_err(Error::Io)?;
    let root: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| Error::Parse(format!("cargo llvm-cov JSON parse error: {e}")))?;

    let data = root
        .pointer("/data/0")
        .ok_or_else(|| Error::Parse("llvm-cov JSON: missing data[0]".to_string()))?;

    let files = data
        .get("files")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    // Always collect the functions array — used both in the region fallback
    // branch and in the per-CodeRef symbol fallback (Track B).
    let functions: Vec<&serde_json::Value> = data
        .get("functions")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default();

    // Two coverage stores — whichever is populated wins.
    //
    // Segment store (from data[0].files[*].segments):
    //   Cursor semantics — each entry sets exec_count for all following lines
    //   until the next entry. Lookup: last entry with line ≤ query_line.
    //
    // Region store (from data[0].functions[*].regions, fallback when files=[]):
    //   Explicit-range semantics — each CodeRegion covers [start_line, end_line].
    //   Stored as line -> max_exec_count for direct lookup.
    let mut segment_coverage: HashMap<String, Vec<(usize, u64)>> = HashMap::new();
    let mut region_coverage: HashMap<String, HashMap<usize, u64>> = HashMap::new();

    // Primary path: file-level segment data (cursor semantics).
    for file_entry in files {
        let filename = file_entry
            .get("filename")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let norm = normalise_path(filename);

        let segments = file_entry
            .get("segments")
            .and_then(|v| v.as_array())
            .map(|a| a.as_slice())
            .unwrap_or(&[]);

        // Each segment: [line, col, exec_count, has_count, is_region_entry, ...]
        // Skip segments where has_count == false — those are region delimiters with
        // no meaningful exec_count and would shadow real coverage entries.
        let mut coverage_points: Vec<(usize, u64)> = segments
            .iter()
            .filter_map(|seg| {
                let arr = seg.as_array()?;
                let line = arr.first()?.as_u64()? as usize;
                let has_count = arr.get(3).and_then(|v| v.as_bool()).unwrap_or(false);
                if !has_count {
                    return None;
                }
                let exec_count = arr.get(2)?.as_u64().unwrap_or(0);
                Some((line, exec_count))
            })
            .collect();
        coverage_points.sort_by_key(|(l, _)| *l);

        segment_coverage.entry(norm).or_default().extend(coverage_points);
    }

    // Secondary path: function-level region data (explicit-range semantics).
    // Always collected from functions[] so that files not present in the
    // files[] segment section (e.g. library crates when only the binary has
    // segment data) still get line-level coverage information.
    //
    // Region format: [start_line, start_col, end_line, end_col, exec_count,
    //                 file_id, expanded_file_id, kind]
    // kind 0 = CodeRegion (has a meaningful exec_count).
    for func in &functions {
        let filenames = func
            .get("filenames")
            .and_then(|v| v.as_array())
            .map(|a| a.as_slice())
            .unwrap_or(&[]);

        let regions = func
            .get("regions")
            .and_then(|v| v.as_array())
            .map(|a| a.as_slice())
            .unwrap_or(&[]);

        for region in regions {
            let arr = match region.as_array() {
                Some(a) => a,
                None => continue,
            };
            let kind = arr.get(7).and_then(|v| v.as_u64()).unwrap_or(1);
            if kind != 0 {
                continue; // only CodeRegion
            }
            let start_line = match arr.first().and_then(|v| v.as_u64()) {
                Some(l) => l as usize,
                None => continue,
            };
            let end_line = match arr.get(2).and_then(|v| v.as_u64()) {
                Some(l) => l as usize,
                None => continue,
            };
            let exec_count = arr.get(4).and_then(|v| v.as_u64()).unwrap_or(0);
            let file_id = arr.get(5).and_then(|v| v.as_u64()).unwrap_or(0) as usize;

            let filename = match filenames.get(file_id).and_then(|v| v.as_str()) {
                Some(f) => f,
                None => continue,
            };
            let norm = normalise_path(filename);
            // Only populate region_coverage for files not already covered by
            // segment data — the segment cursor is more precise for those files.
            if segment_coverage.contains_key(&norm) {
                continue;
            }
            let line_map = region_coverage.entry(norm).or_default();

            for line in start_line..=end_line {
                let entry = line_map.entry(line).or_insert(0);
                *entry = (*entry).max(exec_count);
            }
        }
    }

    let mut scores: Vec<LineCoverageScore> = Vec::new();

    for (req_id, refs) in code_refs {
        for cr in refs {
            let norm_file = normalise_path(&cr.file.to_string_lossy());
            let start = cr.line;
            let end = cr.line_end.unwrap_or(start + 1);

            let mut lines_total = 0usize;
            let mut lines_hit = 0usize;

            let has_line_data = if let Some(coverage) = segment_coverage.get(&norm_file) {
                for line_num in start..end {
                    // Cursor: last segment with line ≤ line_num sets exec_count.
                    let exec_count = coverage
                        .iter()
                        .filter(|(l, _)| *l <= line_num)
                        .last()
                        .map(|(_, c)| *c)
                        .unwrap_or(0);
                    lines_total += 1;
                    if exec_count > 0 {
                        lines_hit += 1;
                    }
                }
                true
            } else if let Some(line_map) = region_coverage.get(&norm_file) {
                for line_num in start..end {
                    lines_total += 1;
                    if line_map.get(&line_num).copied().unwrap_or(0) > 0 {
                        lines_hit += 1;
                    }
                }
                true
            } else {
                false
            };

            // Symbol fallback (Track B): when line-based scoring yields 0 hits
            // (or there was no line data at all), check if the named function
            // was called according to functions[].  LLVM mangled names embed
            // source identifiers verbatim, so contains() is reliable for
            // names ≥ 4 characters.
            if lines_hit == 0 {
                if let Some(sym) = &cr.symbol {
                    if sym.len() >= 4 {
                        if let Some(covered) =
                            symbol_covered_by_functions(sym, &norm_file, &functions)
                        {
                            if covered {
                                lines_hit = lines_total.max(1);
                                lines_total = lines_total.max(1);
                            }
                        }
                    }
                }
            }

            // Skip entries where we have neither line data nor a symbol match.
            if !has_line_data && lines_hit == 0 {
                continue;
            }

            if lines_total > 0 {
                let hit_percent = Some(lines_hit as f64 / lines_total as f64 * 100.0);
                scores.push(LineCoverageScore {
                    req_id: req_id.clone(),
                    file: cr.file.clone(),
                    lines_total,
                    lines_hit,
                    hit_percent,
                });
            }
        }
    }

    Ok(scores)
}

/// Check if a function named `symbol` in file `norm_file` was called according
/// to the functions array from the llvm-cov JSON.
///
/// LLVM Itanium mangling embeds source identifiers verbatim (e.g. `scan_file`
/// appears as `9scan_file` in the mangled name), so a substring check is
/// sufficient and does not require a demangler.
///
/// Returns `Some(true)` when a matching function was called, `Some(false)` when
/// matched but not called, and `None` when no match was found.
fn symbol_covered_by_functions(
    symbol: &str,
    norm_file: &str,
    functions: &[&serde_json::Value],
) -> Option<bool> {
    for func in functions {
        // Check that this function lives in the target file.
        let in_file = func
            .get("filenames")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .any(|f| f.as_str().map(normalise_path).as_deref() == Some(norm_file))
            })
            .unwrap_or(false);
        if !in_file {
            continue;
        }

        let mangled = func.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if !mangled.contains(symbol) {
            continue;
        }

        let count = func.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        return Some(count > 0);
    }
    None
}

fn normalise_path(raw: &str) -> String {
    raw.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn parses_valid_llvm_json_and_scores() {
        // Segments covering lines 1-5: lines 1-3 exec_count=1, lines 4-5 exec_count=0
        let json = r#"{
            "data": [{
                "files": [{
                    "filename": "src/foo.rs",
                    "segments": [
                        [1, 0, 1, true, true],
                        [4, 0, 0, true, true],
                        [6, 0, 0, true, false]
                    ]
                }]
            }]
        }"#;
        let f = write_json(json);
        let refs = make_refs("LLR-0001", "src/foo.rs", 1, 6);
        let scores = parse_llvm_cov_report(f.path(), &refs).unwrap();
        let score = scores.iter().find(|s| s.req_id == "LLR-0001").unwrap();
        assert_eq!(score.lines_total, 5);
        assert_eq!(score.lines_hit, 3);
        assert!((score.hit_percent.unwrap() - 60.0).abs() < 0.01);
    }

    #[test]
    fn line_with_zero_count_is_missed() {
        let json = r#"{
            "data": [{
                "files": [{
                    "filename": "src/foo.rs",
                    "segments": [[1, 0, 0, true, true]]
                }]
            }]
        }"#;
        let f = write_json(json);
        let refs = make_refs("LLR-0001", "src/foo.rs", 1, 2);
        let scores = parse_llvm_cov_report(f.path(), &refs).unwrap();
        let score = scores.iter().find(|s| s.req_id == "LLR-0001").unwrap();
        assert_eq!(score.lines_hit, 0);
    }

    #[test]
    fn lines_outside_refs_are_ignored() {
        let json = r#"{"data": [{"files": [{"filename": "src/foo.rs", "segments": [[50,0,5,true,true]]}]}]}"#;
        let f = write_json(json);
        let refs = make_refs("LLR-0001", "src/foo.rs", 1, 5);
        let scores = parse_llvm_cov_report(f.path(), &refs).unwrap();
        // Lines 1-4 have no covering segment so exec_count=0 → missed
        if let Some(score) = scores.iter().find(|s| s.req_id == "LLR-0001") {
            assert_eq!(score.lines_hit, 0);
        }
    }

    #[test]
    fn malformed_json_returns_parse_error() {
        let f = write_json("not json at all");
        let result = parse_llvm_cov_report(f.path(), &HashMap::new());
        assert!(result.is_err());
    }

    #[test]
    fn path_separators_normalised() {
        let json = r#"{
            "data": [{"files": [{"filename": "src\\foo.rs", "segments": [[1,0,5,true,true]]}]}]
        }"#;
        let f = write_json(json);
        let refs = make_refs("LLR-0001", "src/foo.rs", 1, 2);
        let scores = parse_llvm_cov_report(f.path(), &refs).unwrap();
        assert!(!scores.is_empty(), "expected match after path normalisation");
    }

    #[test]
    fn symbol_fallback_reports_covered_when_function_called() {
        // files[] is empty → region fallback. The region coverage map will have
        // no entry for line 1 (the comment line). But the CodeRef has a symbol
        // that appears in the functions[] mangled name with count > 0.
        let json = r#"{
            "data": [{
                "files": [],
                "functions": [{
                    "name": "_RNv...9scan_file...",
                    "filenames": ["src/scanner.rs"],
                    "count": 6,
                    "regions": []
                }]
            }]
        }"#;
        let f = write_json(json);
        let mut refs = make_refs("LLR-0001", "src/scanner.rs", 1, 2);
        // Attach a symbol so Track B activates.
        refs.get_mut("LLR-0001").unwrap()[0].symbol = Some("scan_file".to_string());
        let scores = parse_llvm_cov_report(f.path(), &refs).unwrap();
        let score = scores.iter().find(|s| s.req_id == "LLR-0001").expect("expected score");
        assert!(score.lines_hit > 0, "symbol fallback should report covered; got {score:?}");
    }

    #[test]
    fn symbol_fallback_respects_uncalled_function() {
        let json = r#"{
            "data": [{
                "files": [],
                "functions": [{
                    "name": "_RNv...12never_called...",
                    "filenames": ["src/foo.rs"],
                    "count": 0,
                    "regions": []
                }]
            }]
        }"#;
        let f = write_json(json);
        let mut refs = make_refs("LLR-0001", "src/foo.rs", 1, 2);
        refs.get_mut("LLR-0001").unwrap()[0].symbol = Some("never_called".to_string());
        let scores = parse_llvm_cov_report(f.path(), &refs).unwrap();
        // No region data → no score entry (file not in region_coverage), OR
        // score entry with lines_hit == 0 if fallback finds count == 0.
        if let Some(score) = scores.iter().find(|s| s.req_id == "LLR-0001") {
            assert_eq!(score.lines_hit, 0, "uncalled function should have 0 hits; got {score:?}");
        }
    }
}

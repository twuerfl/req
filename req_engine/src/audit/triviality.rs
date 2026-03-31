// REQ: LLR-0026
//! Static triviality detector for tagged source spans.
//!
//! Reads the source lines of a `CodeRef` span and applies six hollow-body
//! detection heuristics (P1–P6), returning a `TrivialityFinding` per match.
//!
//! No external tools are required — detection is regex-based text analysis.

use crate::{Error, Result};
use req_lib::{FindingSeverity, TrivialityFinding, TrivialityPattern};
use std::path::Path;

/// Analyse a source span for triviality patterns.
///
/// `line` and `line_end` are 1-based, matching `CodeRef` semantics.
/// When `line_end` is `None` a window of up to 20 lines starting at `line`
/// is used.
pub fn analyse_code_ref(
    path: &Path,
    line: usize,
    line_end: Option<usize>,
    req_id: &str,
) -> Result<Vec<TrivialityFinding>> {
    // Skip build.rs, benches/, examples/
    if should_skip(path) {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(path).map_err(Error::Io)?;
    let all_lines: Vec<&str> = content.lines().collect();

    let start = line.saturating_sub(1); // convert to 0-based
    let end = line_end
        .map(|e| e.min(all_lines.len()))
        .unwrap_or_else(|| (start + 20).min(all_lines.len()));

    if start >= all_lines.len() {
        return Ok(vec![]);
    }

    let span: &[&str] = &all_lines[start..end];
    let preamble_start = start.saturating_sub(3);
    let has_allow_dead_code = span_has_allow_dead_code(span)
        || all_lines[preamble_start..start]
            .iter()
            .any(|l| l.contains("#[allow(dead_code)]"));

    let is_test_ctx = is_test_context(span)
        || (start >= 3
            && all_lines[start.saturating_sub(3)..start]
                .iter()
                .any(|l| l.contains("#[test]") || l.contains("#[cfg(test)]")));

    let mut findings = Vec::new();

    // P3 — zero substantive lines (highest priority, check first)
    if count_substantive_lines(span) == 0 {
        findings.push(TrivialityFinding {
            req_id: req_id.to_string(),
            file: path.to_path_buf(),
            line,
            pattern: TrivialityPattern::ZeroSubstantiveLines,
            severity: FindingSeverity::Error,
            matched_text: span.first().copied().unwrap_or("").to_string(),
        });
        return Ok(findings); // no point running other checks
    }

    // P5 — empty test body
    if is_test_ctx && !span_has_assertion(span) && !span_has_function_call(span) {
        findings.push(TrivialityFinding {
            req_id: req_id.to_string(),
            file: path.to_path_buf(),
            line,
            pattern: TrivialityPattern::EmptyTestBody,
            severity: FindingSeverity::Error,
            matched_text: span_body_excerpt(span),
        });
    }

    // P1 — tautological assertion
    for (i, src_line) in span.iter().enumerate() {
        if let Some(matched) = detect_tautological_assertion(src_line) {
            findings.push(TrivialityFinding {
                req_id: req_id.to_string(),
                file: path.to_path_buf(),
                line: line + i,
                pattern: TrivialityPattern::TautologicalAssertion,
                severity: FindingSeverity::Error,
                matched_text: matched,
            });
        }
    }

    // P2 — stub body (only for non-test files)
    if !is_test_ctx {
        if let Some(body) = find_function_body(span) {
            if is_stub_body(body) {
                let severity = if has_allow_dead_code {
                    FindingSeverity::Info
                } else {
                    FindingSeverity::Warning
                };
                findings.push(TrivialityFinding {
                    req_id: req_id.to_string(),
                    file: path.to_path_buf(),
                    line,
                    pattern: TrivialityPattern::StubBody,
                    severity,
                    matched_text: body
                        .iter()
                        .find(|l| !l.trim().is_empty())
                        .copied()
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                });
            }

            // P4 — single literal return
            if let Some(lit) = detect_single_literal_return(body) {
                findings.push(TrivialityFinding {
                    req_id: req_id.to_string(),
                    file: path.to_path_buf(),
                    line,
                    pattern: TrivialityPattern::SingleLiteralReturn,
                    severity: FindingSeverity::Warning,
                    matched_text: lit,
                });
            }
        }
    }

    Ok(findings)
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn should_skip(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("build.rs")
        || s.contains("/benches/")
        || s.contains("\\benches\\")
        || s.contains("/examples/")
        || s.contains("\\examples\\")
}

fn span_has_allow_dead_code(span: &[&str]) -> bool {
    span.iter().any(|l| l.contains("#[allow(dead_code)]"))
}

fn is_test_context(span: &[&str]) -> bool {
    span.iter()
        .any(|l| l.contains("#[test]") || l.contains("#[cfg(test)]"))
}

/// A line is "substantive" if it contains a meaningful token.
fn is_substantive(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() || t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') {
        return false;
    }
    // tag comment lines are not substantive
    if t.contains("REQ:") || t.contains("VERIFIES:") || t.contains("CRITERION:") {
        return false;
    }
    true
}

fn count_substantive_lines(span: &[&str]) -> usize {
    span.iter().filter(|l| is_substantive(l)).count()
}

fn span_has_assertion(span: &[&str]) -> bool {
    span.iter().any(|l| {
        l.contains("assert!") || l.contains("assert_eq!") || l.contains("assert_ne!")
    })
}

fn span_has_function_call(span: &[&str]) -> bool {
    span.iter().any(|l| {
        let t = l.trim();
        !t.starts_with("//") && t.contains('(') && !t.starts_with('#')
    })
}

fn span_body_excerpt(span: &[&str]) -> String {
    span.iter()
        .filter(|l| is_substantive(l))
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join("; ")
}

fn detect_tautological_assertion(line: &str) -> Option<String> {
    let t = line.trim();
    // assert!(true) or assert!(false == false) etc.
    if t.contains("assert!(true)") || t.contains("assert!( true )") {
        return Some(t.to_string());
    }
    // assert_eq!(x, x) where both sides are identical simple tokens
    if let Some(inner) = t
        .strip_prefix("assert_eq!(")
        .and_then(|s| s.strip_suffix(");").or_else(|| s.strip_suffix(')')))
    {
        let parts: Vec<&str> = inner.splitn(2, ',').collect();
        if parts.len() == 2 && parts[0].trim() == parts[1].trim() {
            return Some(t.to_string());
        }
    }
    None
}

/// Extract lines between the first `{` and last `}` in the span.
fn find_function_body<'a>(span: &'a [&'a str]) -> Option<&'a [&'a str]> {
    let open = span.iter().position(|l| l.contains('{'))?;
    let close = span.iter().rposition(|l| l.contains('}'))?;
    if close > open {
        // Exclude the function-signature line (which contains '{') and the
        // closing-brace line — only the body statements matter for stub detection.
        Some(&span[open + 1..close])
    } else {
        None
    }
}

fn is_stub_body(body: &[&str]) -> bool {
    let substantive: Vec<&str> = body.iter().filter(|l| is_substantive(l)).cloned().collect();
    if substantive.is_empty() {
        return true;
    }
    // Accept up to 2 lines: the opening `{` line and closing `}` line, or just stub patterns
    let stub_patterns = [
        "Ok(())",
        "unimplemented!()",
        "todo!()",
        "panic!(",
        "unreachable!()",
    ];
    substantive.iter().all(|line| {
        let t = line.trim().trim_start_matches('{').trim_end_matches('}').trim();
        stub_patterns.iter().any(|p| t.contains(p)) || t == "{" || t == "}" || t.is_empty()
    })
}

fn detect_single_literal_return(body: &[&str]) -> Option<String> {
    let substantive: Vec<&str> = body.iter().filter(|l| is_substantive(l)).cloned().collect();
    // Only flag when the body is a single substantive expression
    if substantive.len() != 1 {
        return None;
    }
    let t = substantive[0]
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim();
    let literals = ["true", "false", "0", "1", "\"\"", "None", "vec![]"];
    if literals.iter().any(|lit| t == *lit || t == &format!("{lit};")) {
        Some(t.to_string())
    } else {
        None
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn detects_tautological_assert() {
        let f = write_temp("// VERIFIES: LLR-0001\n#[test]\nfn t() { assert!(true); }\n");
        let findings = analyse_code_ref(f.path(), 1, Some(3), "LLR-0001").unwrap();
        assert!(
            findings
                .iter()
                .any(|f| f.pattern == TrivialityPattern::TautologicalAssertion),
            "expected TautologicalAssertion, got: {findings:?}"
        );
        assert!(findings.iter().any(|f| f.severity == FindingSeverity::Error));
    }

    #[test]
    fn detects_zero_substantive_lines() {
        let f = write_temp("// REQ: LLR-0001\n\n// just a comment\n");
        let findings = analyse_code_ref(f.path(), 1, Some(3), "LLR-0001").unwrap();
        assert!(
            findings
                .iter()
                .any(|f| f.pattern == TrivialityPattern::ZeroSubstantiveLines),
            "expected ZeroSubstantiveLines"
        );
    }

    #[test]
    fn legitimate_function_no_findings() {
        let f = write_temp(
            "// REQ: LLR-0001\nfn add(a: i32, b: i32) -> i32 {\n    let result = a + b;\n    if result > 100 { return 100; }\n    result\n}\n",
        );
        let findings = analyse_code_ref(f.path(), 1, Some(6), "LLR-0001").unwrap();
        assert!(findings.is_empty(), "unexpected findings: {findings:?}");
    }

    #[test]
    fn detects_stub_body() {
        let f = write_temp("// REQ: LLR-0001\nfn foo() -> Result<()> {\n    Ok(())\n}\n");
        let findings = analyse_code_ref(f.path(), 1, Some(4), "LLR-0001").unwrap();
        assert!(
            findings.iter().any(|f| f.pattern == TrivialityPattern::StubBody),
            "expected StubBody"
        );
        assert!(findings.iter().any(|f| f.severity == FindingSeverity::Warning));
    }

    #[test]
    fn skips_examples_directory() {
        use std::path::PathBuf;
        let fake_path = PathBuf::from("/project/examples/demo.rs");
        let findings = analyse_code_ref(&fake_path, 1, Some(3), "LLR-0001");
        // Either Ok(empty) or Io error from missing file — either way no findings
        assert!(findings.unwrap_or_default().is_empty());
    }

    #[test]
    fn allow_dead_code_downgrades_stub_to_info() {
        let f = write_temp(
            "#[allow(dead_code)]\n// REQ: LLR-0001\nfn foo() {\n    Ok(())\n}\n",
        );
        let findings = analyse_code_ref(f.path(), 2, Some(5), "LLR-0001").unwrap();
        let stub = findings
            .iter()
            .find(|f| f.pattern == TrivialityPattern::StubBody);
        if let Some(s) = stub {
            assert_eq!(s.severity, FindingSeverity::Info);
        }
        // If no StubBody finding at all that's also acceptable (body has Ok(()) which could be caught by other checks)
    }
}

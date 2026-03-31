// REQ: LLR-0031
//! Author independence check — uses `git blame` to detect when implementation
//! and test of the same LLR were written by the same identity.
//!
//! Compiled unconditionally; the git-feature-gated implementation is selected
//! via `#[cfg(feature = "git")]`. Without the feature the method returns an
//! empty result with one informational warning.

use crate::Result;
use req_lib::{CodeRef, IndependenceResult, IndependenceWarning};
use std::path::Path;

/// Check author independence for the given implementation and test spans.
///
/// When compiled without `--features git` this always returns `Ok` with an
/// empty violations list and one warning indicating the check was skipped.
pub fn check_independence(
    _base: &Path,
    impl_refs: &[CodeRef],
    test_refs: &[CodeRef],
) -> Result<IndependenceResult> {
    #[cfg(feature = "git")]
    {
        check_independence_git(_base, impl_refs, test_refs)
    }
    #[cfg(not(feature = "git"))]
    {
        let _ = (impl_refs, test_refs);
        Ok(IndependenceResult {
            violations: vec![],
            warnings: vec![IndependenceWarning {
                req_id: String::new(),
                file: std::path::PathBuf::new(),
                reason: "git feature not enabled; independence check skipped. \
                         Rebuild with --features git to enable."
                    .to_string(),
            }],
        })
    }
}

#[cfg(feature = "git")]
fn check_independence_git(
    base: &Path,
    impl_refs: &[CodeRef],
    test_refs: &[CodeRef],
) -> Result<IndependenceResult> {
    use git2::Repository;
    use req_lib::IndependenceViolation;
    use std::collections::{HashMap, HashSet};

    let repo = match Repository::discover(base) {
        Ok(r) => r,
        Err(e) => {
            return Ok(IndependenceResult {
                violations: vec![],
                warnings: vec![IndependenceWarning {
                    req_id: String::new(),
                    file: base.to_path_buf(),
                    reason: format!("git repository not found: {e}"),
                }],
            });
        }
    };

    let mut impl_ids: HashMap<String, HashSet<String>> = HashMap::new();
    let mut test_ids: HashMap<String, HashSet<String>> = HashMap::new();
    let mut warnings: Vec<IndependenceWarning> = Vec::new();

    collect_identities(&repo, impl_refs, &mut impl_ids, &mut warnings);
    collect_identities(&repo, test_refs, &mut test_ids, &mut warnings);

    let mut violations: Vec<IndependenceViolation> = Vec::new();
    for (req_id, impl_set) in &impl_ids {
        if let Some(test_set) = test_ids.get(req_id) {
            let shared: Vec<String> = impl_set.intersection(test_set).cloned().collect();
            if !shared.is_empty() {
                violations.push(IndependenceViolation {
                    req_id: req_id.clone(),
                    impl_identities: impl_set.iter().cloned().collect(),
                    test_identities: test_set.iter().cloned().collect(),
                    shared,
                });
            }
        }
    }

    Ok(IndependenceResult { violations, warnings })
}

#[cfg(feature = "git")]
fn collect_identities(
    repo: &git2::Repository,
    refs: &[CodeRef],
    ids: &mut std::collections::HashMap<String, std::collections::HashSet<String>>,
    warnings: &mut Vec<IndependenceWarning>,
) {
    for cr in refs {
        // Skip uncommitted files
        let rel = match cr.file.strip_prefix(repo.workdir().unwrap_or(std::path::Path::new(""))) {
            Ok(r) => r.to_path_buf(),
            Err(_) => cr.file.clone(),
        };

        let status = repo.status_file(&rel);
        match status {
            Ok(s) if s == git2::Status::CURRENT => {}
            _ => {
                warnings.push(IndependenceWarning {
                    req_id: cr.req_id.clone(),
                    file: cr.file.clone(),
                    reason: "uncommitted changes; file excluded from independence check".to_string(),
                });
                continue;
            }
        }

        let blame = match repo.blame_file(&rel, None) {
            Ok(b) => b,
            Err(e) => {
                warnings.push(IndependenceWarning {
                    req_id: cr.req_id.clone(),
                    file: cr.file.clone(),
                    reason: format!("git blame failed: {e}"),
                });
                continue;
            }
        };

        let end = cr.line_end.unwrap_or(cr.line + 1);
        let entry = ids.entry(cr.req_id.clone()).or_default();

        for line_num in cr.line..end {
            if let Ok(hunk) = blame.get_line(line_num) {
                let commit_id = hunk.orig_commit_id();
                if let Ok(commit) = repo.find_commit(commit_id) {
                    // Check for Req-Agent-Session trailer
                    let identity = extract_agent_session(commit.message().unwrap_or(""))
                        .unwrap_or_else(|| {
                            commit
                                .author()
                                .email()
                                .unwrap_or("unknown")
                                .to_string()
                        });
                    entry.insert(identity);
                }
            }
        }
    }
}

/// Extract the `Req-Agent-Session: <token>` trailer from a commit message.
#[cfg(feature = "git")]
fn extract_agent_session(message: &str) -> Option<String> {
    for line in message.lines().rev() {
        if let Some(token) = line.strip_prefix("Req-Agent-Session:") {
            return Some(token.trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use req_lib::CodeRef;
    use std::path::PathBuf;

    #[test]
    fn without_git_feature_returns_ok_with_warning() {
        let result =
            check_independence(Path::new("."), &[], &[]).unwrap();
        assert!(result.violations.is_empty());
        #[cfg(not(feature = "git"))]
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn empty_refs_no_violations() {
        let result = check_independence(Path::new("."), &[], &[]).unwrap();
        assert!(result.violations.is_empty());
    }

    fn make_ref(req_id: &str, file: &str) -> CodeRef {
        CodeRef {
            req_id: req_id.to_string(),
            file: PathBuf::from(file),
            line: 1,
            line_end: Some(5),
            hash: None,
            symbol: None,
        }
    }

    #[test]
    fn refs_with_non_existent_repo_produce_warning_not_panic() {
        let r = make_ref("LLR-0001", "src/foo.rs");
        let result = check_independence(Path::new("/nonexistent/path"), &[r.clone()], &[r]);
        assert!(result.is_ok());
    }
}

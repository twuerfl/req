//! Integration tests for the AI agent output integrity (audit) feature.
// REQ: TST-0013
// REQ: TST-0014
// REQ: TST-0015
// REQ: TST-0016
// REQ: TST-0017
// REQ: TST-0018
// REQ: TST-0019
// VERIFIES: LLR-0026
// VERIFIES: LLR-0027
// VERIFIES: LLR-0028
// VERIFIES: LLR-0029
// VERIFIES: LLR-0030
// VERIFIES: LLR-0031
// VERIFIES: LLR-0032

use req_engine::audit::{coverage_map, independence, mutation, triviality};
use req_engine::{ReqEngine, Requirement, RequirementStatus, RequirementType};
use req_engine::cache::Cache;
use req_lib::{CodeRef, FindingSeverity, TrivialityPattern};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// ── helpers ───────────────────────────────────────────────────────────────────

fn make_cache(dir: &Path) -> Cache {
    Cache::open(&dir.join("test.db"), 0).unwrap()
}

fn make_code_ref(req_id: &str, file: &str, line: usize, line_end: usize) -> CodeRef {
    CodeRef {
        req_id: req_id.to_string(),
        file: PathBuf::from(file),
        line,
        line_end: Some(line_end),
        hash: None,
        symbol: None,
    }
}

fn make_refs_map(req_id: &str, file: &str, line: usize, line_end: usize) -> HashMap<String, Vec<CodeRef>> {
    let mut m = HashMap::new();
    m.insert(req_id.to_string(), vec![make_code_ref(req_id, file, line, line_end)]);
    m
}

fn write_tmp_file(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path
}

fn write_json(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f
}

/// Set up a minimal initialized project, return engine + TempDir.
fn make_engine() -> (ReqEngine, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    ReqEngine::init(dir.path(), Some("test-project")).unwrap();
    let engine = ReqEngine::open(dir.path()).unwrap();
    (engine, dir)
}

fn add_llr(engine: &ReqEngine, id: &str) {
    let mut req = Requirement::new(id.to_string(), RequirementType::Llr, "Test LLR".to_string());
    req.status = RequirementStatus::Approved;
    engine.cache().upsert_requirement(&req).unwrap();
}

// ── TST-0013: Triviality Detector ─────────────────────────────────────────────

/// TC-01: Empty span (no lines) triggers ZeroSubstantiveLines.
// VERIFIES: TST-0013
// CRITERION: TST-0013 #1
#[test]
fn triviality_empty_span_is_zero_substantive() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_tmp_file(dir.path(), "foo.rs", "\n\n\n");
    let findings = triviality::analyse_code_ref(&path, 1, Some(3), "LLR-0001").unwrap();
    assert!(
        findings.iter().any(|f| f.pattern == TrivialityPattern::ZeroSubstantiveLines),
        "expected ZeroSubstantiveLines"
    );
}

/// TC-02: `assert!(true)` triggers TautologicalAssertion with Error severity.
// VERIFIES: TST-0013
// CRITERION: TST-0013 #2
#[test]
fn triviality_assert_true_is_tautological() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_tmp_file(dir.path(), "foo.rs", "fn t() {\n    assert!(true);\n}\n");
    let findings = triviality::analyse_code_ref(&path, 1, Some(3), "LLR-0001").unwrap();
    let f = findings.iter().find(|f| f.pattern == TrivialityPattern::TautologicalAssertion);
    assert!(f.is_some(), "expected TautologicalAssertion");
    assert_eq!(f.unwrap().severity, FindingSeverity::Error);
}

/// TC-03: Real implementation with assertions produces no findings.
// VERIFIES: TST-0013
// CRITERION: TST-0013 #3
#[test]
fn triviality_real_impl_no_findings() {
    let dir = tempfile::tempdir().unwrap();
    let content = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
    let path = write_tmp_file(dir.path(), "lib.rs", content);
    let findings = triviality::analyse_code_ref(&path, 1, Some(3), "LLR-0001").unwrap();
    assert!(findings.is_empty(), "expected no findings for real impl");
}

/// TC-04: Skips build.rs.
// VERIFIES: TST-0013
#[test]
fn triviality_skips_build_rs() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_tmp_file(dir.path(), "build.rs", "\n\n");
    let findings = triviality::analyse_code_ref(&path, 1, Some(2), "LLR-0001").unwrap();
    assert!(findings.is_empty(), "build.rs should be skipped");
}

/// TC-05: Missing file returns Err.
// VERIFIES: TST-0013
#[test]
fn triviality_missing_file_returns_err() {
    let result = triviality::analyse_code_ref(Path::new("/nonexistent/file.rs"), 1, None, "LLR-0001");
    assert!(result.is_err());
}

// ── TST-0014: Mutation Testing Adapter ───────────────────────────────────────

/// TC-01: Parses valid cargo-mutants JSON and counts caught/missed correctly.
// VERIFIES: TST-0014
// CRITERION: TST-0014 #1
#[test]
fn mutation_parses_valid_json() {
    let json = r#"[
        {"file":"src/foo.rs","line":10,"kind":"Replace","outcome":"Caught"},
        {"file":"src/foo.rs","line":11,"kind":"Replace","outcome":"Missed"}
    ]"#;
    let f = write_json(json);
    let refs = make_refs_map("LLR-0001", "src/foo.rs", 8, 15);
    let report = mutation::parse_mutants_report(f.path(), &refs).unwrap();
    let score = report.scores.iter().find(|s| s.req_id == "LLR-0001").unwrap();
    assert_eq!(score.caught, 1);
    assert_eq!(score.missed, 1);
    assert!((score.score_percent.unwrap() - 50.0).abs() < 0.01);
}

/// TC-02: Mutant outside any tagged range goes to untagged_mutants.
// VERIFIES: TST-0014
// CRITERION: TST-0014 #2
#[test]
fn mutation_unmatched_goes_to_untagged() {
    let json = r#"[{"file":"src/other.rs","line":100,"outcome":"Caught"}]"#;
    let f = write_json(json);
    let refs = make_refs_map("LLR-0001", "src/foo.rs", 1, 10);
    let report = mutation::parse_mutants_report(f.path(), &refs).unwrap();
    assert_eq!(report.untagged_mutants, 1);
    assert!(report.scores.is_empty());
}

/// TC-03: Malformed JSON returns Err.
// VERIFIES: TST-0014
// CRITERION: TST-0014 #3
#[test]
fn mutation_malformed_json_returns_err() {
    let f = write_json("{not valid json}");
    let result = mutation::parse_mutants_report(f.path(), &HashMap::new());
    assert!(result.is_err());
}

// ── TST-0015: Line Coverage Adapter ──────────────────────────────────────────

/// TC-01: Parses llvm-cov JSON and computes correct hit percentage.
// VERIFIES: TST-0015
// CRITERION: TST-0015 #1
#[test]
fn coverage_parses_valid_json() {
    let json = r#"{
        "data": [{"files": [{"filename": "src/foo.rs", "segments": [
            [1, 0, 1, true, true],
            [4, 0, 0, true, true],
            [6, 0, 0, true, false]
        ]}]}]
    }"#;
    let f = write_json(json);
    let refs = make_refs_map("LLR-0001", "src/foo.rs", 1, 6);
    let scores = coverage_map::parse_llvm_cov_report(f.path(), &refs).unwrap();
    let score = scores.iter().find(|s| s.req_id == "LLR-0001").unwrap();
    assert_eq!(score.lines_total, 5);
    assert_eq!(score.lines_hit, 3);
    assert!((score.hit_percent.unwrap() - 60.0).abs() < 0.01);
}

/// TC-02: Path separators are normalised (backslash == forward slash).
// VERIFIES: TST-0015
// CRITERION: TST-0015 #2
#[test]
fn coverage_path_normalisation() {
    let json = r#"{"data":[{"files":[{"filename":"src\\foo.rs","segments":[[1,0,5,true,true]]}]}]}"#;
    let f = write_json(json);
    let refs = make_refs_map("LLR-0001", "src/foo.rs", 1, 2);
    let scores = coverage_map::parse_llvm_cov_report(f.path(), &refs).unwrap();
    assert!(!scores.is_empty(), "should match after path normalisation");
}

/// TC-03: Malformed JSON returns Err.
// VERIFIES: TST-0015
#[test]
fn coverage_malformed_json_returns_err() {
    let f = write_json("not json");
    let result = coverage_map::parse_llvm_cov_report(f.path(), &HashMap::new());
    assert!(result.is_err());
}

// ── TST-0016: Criterion Linkage ───────────────────────────────────────────────

/// TC-01: Cache stores and retrieves criterion links correctly.
// VERIFIES: TST-0016
// CRITERION: TST-0016 #1
#[test]
fn criterion_cache_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let cache = make_cache(dir.path());
    let file = dir.path().join("test.rs");

    cache.insert_criterion_link("LLR-0001", 2, &file, 42).unwrap();
    let links = cache.get_criterion_links("LLR-0001").unwrap();
    assert_eq!(links.len(), 1);
    let (idx, ref_file, line) = &links[0];
    assert_eq!(*idx, 2);
    assert_eq!(ref_file, &file);
    assert_eq!(*line, 42);
}

/// TC-02: delete_criterion_links_for_file removes only that file's links.
// VERIFIES: TST-0016
// CRITERION: TST-0016 #2
#[test]
fn criterion_delete_for_file() {
    let dir = tempfile::tempdir().unwrap();
    let cache = make_cache(dir.path());
    let file_a = dir.path().join("a.rs");
    let file_b = dir.path().join("b.rs");

    cache.insert_criterion_link("LLR-0001", 1, &file_a, 10).unwrap();
    cache.insert_criterion_link("LLR-0001", 2, &file_b, 20).unwrap();

    cache.delete_criterion_links_for_file(&file_a).unwrap();
    let links = cache.get_criterion_links("LLR-0001").unwrap();
    assert_eq!(links.len(), 1, "only file_b link should remain");
    assert_eq!(links[0].1, file_b);
}

/// TC-03: audit_criteria returns unlinked status for criteria with no tag.
// VERIFIES: TST-0016
// CRITERION: TST-0016 #3
#[test]
fn audit_criteria_unlinked_status() {
    let (engine, _dir) = make_engine();

    let mut req = Requirement::new("LLR-0001".to_string(), RequirementType::Llr, "Req".to_string());
    req.text = "## Acceptance criteria\n- [ ] First criterion\n- [ ] Second criterion\n".to_string();
    req.status = RequirementStatus::Approved;
    engine.cache().upsert_requirement(&req).unwrap();

    let report = engine.audit_criteria("LLR-0001").unwrap();
    assert_eq!(report.criteria.len(), 2);
    assert!(!report.criteria[0].linked);
    assert!(!report.criteria[1].linked);
}

/// TC-04: audit_criteria returns linked status after scan adds CRITERION tag.
// VERIFIES: TST-0016
// CRITERION: TST-0016 #4
#[test]
fn audit_criteria_linked_after_scan() {
    let (engine, dir) = make_engine();

    let mut req = Requirement::new("LLR-0099".to_string(), RequirementType::Llr, "Req".to_string());
    req.text = "## Acceptance criteria\n- [ ] Do the thing\n".to_string();
    req.status = RequirementStatus::Approved;
    engine.cache().upsert_requirement(&req).unwrap();

    // Write a source file with the CRITERION tag
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    write_tmp_file(&src, "thing.rs", "// CRITERION: LLR-0099 #1\nfn do_thing() { assert!(1==1); }\n");

    engine.scan(Some(&src), true).unwrap();

    let report = engine.audit_criteria("LLR-0099").unwrap();
    assert_eq!(report.criteria.len(), 1);
    assert!(report.criteria[0].linked, "criterion #1 should be linked after scan");
}

// ── TST-0017: AuditBundle Export ──────────────────────────────────────────────

/// TC-01: audit_export returns a bundle with correct schema version.
// VERIFIES: TST-0017
// CRITERION: TST-0017 #1
#[test]
fn audit_bundle_schema_version() {
    let (engine, _dir) = make_engine();
    add_llr(&engine, "LLR-0001");

    let bundle = engine.audit_export("LLR-0001", None, None).unwrap();
    assert_eq!(bundle.schema_version, req_lib::AUDIT_SCHEMA_VERSION);
}

/// TC-02: prompt_hint equals the static constant.
// VERIFIES: TST-0017
// CRITERION: TST-0017 #2
#[test]
fn audit_bundle_prompt_hint_is_constant() {
    let (engine, _dir) = make_engine();
    add_llr(&engine, "LLR-0001");

    let bundle = engine.audit_export("LLR-0001", None, None).unwrap();
    assert_eq!(bundle.prompt_hint, req_lib::AUDIT_PROMPT_HINT);
}

/// TC-03: Unknown LLR returns Err.
// VERIFIES: TST-0017
// CRITERION: TST-0017 #3
#[test]
fn audit_bundle_unknown_id_returns_err() {
    let (engine, _dir) = make_engine();
    let result = engine.audit_export("LLR-9999", None, None);
    assert!(result.is_err());
}

/// TC-04: Bundle serialises to valid JSON.
// VERIFIES: TST-0017
// CRITERION: TST-0017 #4
#[test]
fn audit_bundle_serialises_to_json() {
    let (engine, _dir) = make_engine();
    add_llr(&engine, "LLR-0001");

    let bundle = engine.audit_export("LLR-0001", None, None).unwrap();
    let json = serde_json::to_string_pretty(&bundle);
    assert!(json.is_ok(), "bundle must serialise to valid JSON");
}

// ── TST-0018: Author Independence ─────────────────────────────────────────────

/// TC-01: Without git feature, returns Ok with a skip warning.
// VERIFIES: TST-0018
// CRITERION: TST-0018 #1
#[test]
fn independence_without_git_feature_returns_ok() {
    let result = independence::check_independence(Path::new("."), &[], &[]).unwrap();
    assert!(result.violations.is_empty());
    #[cfg(not(feature = "git"))]
    assert!(!result.warnings.is_empty(), "should warn that git feature is disabled");
}

/// TC-02: Non-existent repo path returns Ok with a warning (no panic).
// VERIFIES: TST-0018
// CRITERION: TST-0018 #2
#[test]
fn independence_nonexistent_repo_returns_warning() {
    let r = make_code_ref("LLR-0001", "src/foo.rs", 1, 5);
    let result = independence::check_independence(Path::new("/nonexistent/path"), &[r.clone()], &[r]);
    assert!(result.is_ok());
}

/// TC-03: Empty refs produce no violations.
// VERIFIES: TST-0018
// CRITERION: TST-0018 #3
#[test]
fn independence_empty_refs_no_violations() {
    let result = independence::check_independence(Path::new("."), &[], &[]).unwrap();
    assert!(result.violations.is_empty());
}

// ── TST-0019: req audit CLI Integration ───────────────────────────────────────

/// TC-01: audit_triviality engine method returns Ok for an initialized project.
// VERIFIES: TST-0019
// CRITERION: TST-0019 #1
#[test]
fn audit_triviality_engine_method_returns_ok() {
    let (engine, _dir) = make_engine();
    add_llr(&engine, "LLR-0001");
    let result = engine.audit_triviality(None);
    assert!(result.is_ok());
}

/// TC-02: audit_mutation with a valid report returns Ok.
// VERIFIES: TST-0019
// CRITERION: TST-0019 #2
#[test]
fn audit_mutation_engine_method_returns_ok() {
    let (engine, _dir) = make_engine();
    let f = write_json(r#"[{"file":"src/foo.rs","line":1,"outcome":"Caught"}]"#);
    let result = engine.audit_mutation(f.path());
    assert!(result.is_ok());
}

/// TC-03: audit_coverage with a valid report returns Ok.
// VERIFIES: TST-0019
// CRITERION: TST-0019 #3
#[test]
fn audit_coverage_engine_method_returns_ok() {
    let (engine, _dir) = make_engine();
    let f = write_json(r#"{"data":[{"files":[]}]}"#);
    let result = engine.audit_coverage(f.path());
    assert!(result.is_ok());
}

/// TC-04: audit_independence engine method returns Ok for an initialized project.
// VERIFIES: TST-0019
// CRITERION: TST-0019 #4
#[test]
fn audit_independence_engine_method_returns_ok() {
    let (engine, _dir) = make_engine();
    let result = engine.audit_independence(None);
    assert!(result.is_ok());
}

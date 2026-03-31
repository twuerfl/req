//! Real-world integration test: runs `cargo llvm-cov` against the fixture crate
//! and feeds actual JSON output into `engine.audit_coverage()`.
//! Skips gracefully when cargo-llvm-cov is not installed.
// REQ: TST-0015
// VERIFIES: LLR-0028

use req_engine::{CodeRef, ReqEngine, Requirement, RequirementStatus, RequirementType};
use std::path::PathBuf;
use std::process::Command;

// ── Line number constants ─────────────────────────────────────────────────────
// These match the LOAD-BEARING comments in
// tests/fixtures/coverage_fixture/src/lib.rs.
// Update here AND in TST-0015.md if the fixture file ever changes.

/// Expression line of `add_numbers` body — covered by #[test] test_add.
const COVERED_LINE_START: usize = 9;
const COVERED_LINE_END: usize = 10; // exclusive upper bound

/// Expression line of `never_called` body — never invoked by any test.
const UNCOVERED_LINE_START: usize = 13;
const UNCOVERED_LINE_END: usize = 14;

// ── Skip guard ────────────────────────────────────────────────────────────────

/// Remove env vars that cargo-llvm-cov injects into its child processes.
/// Without this, a nested `cargo llvm-cov` invocation inherits a stale
/// RUSTC_WRAPPER / CARGO_ENCODED_RUSTFLAGS and fails to compile.
fn clean_llvm_cov_env(cmd: &mut Command) -> &mut Command {
    for var in &[
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTFLAGS",
        "LLVM_PROFILE_FILE",
        "CARGO_LLVM_COV_TARGET_DIR",
        "LLVM_COV",
        "LLVM_PROFDATA",
    ] {
        cmd.env_remove(var);
    }
    cmd
}

fn llvm_cov_available() -> bool {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut cmd = Command::new(cargo);
    cmd.args(["llvm-cov", "--version"]);
    clean_llvm_cov_env(&mut cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ── TC-006 ────────────────────────────────────────────────────────────────────

/// TC-006 — Real `cargo llvm-cov` output is parsed correctly via engine facade.
// VERIFIES: LLR-0028
#[test]
fn llvm_cov_real_output_parsed_correctly() {
    if !llvm_cov_available() {
        eprintln!(
            "SKIP: cargo-llvm-cov not available. \
             Install with: cargo install cargo-llvm-cov && rustup component add llvm-tools-preview"
        );
        return;
    }

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_manifest = workspace_root
        .join("tests/fixtures/coverage_fixture/Cargo.toml");
    let fixture_src_lib = workspace_root
        .join("tests/fixtures/coverage_fixture/src/lib.rs");

    assert!(
        fixture_manifest.exists(),
        "fixture crate not found at {}",
        fixture_manifest.display()
    );

    // Write llvm-cov JSON to a temp file. Keep the binding alive for the
    // entire test so the file is not deleted before audit_coverage reads it.
    let output_file = tempfile::NamedTempFile::new().expect("failed to create temp file");
    let output_path = output_file.path().to_path_buf();

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    let mut cov_cmd = Command::new(&cargo);
    cov_cmd.args([
        "llvm-cov",
        "--manifest-path",
        &fixture_manifest.to_string_lossy(),
        "--json",
        "--output-path",
        &output_path.to_string_lossy(),
    ]);
    clean_llvm_cov_env(&mut cov_cmd);
    let status = cov_cmd.status().expect("failed to spawn cargo llvm-cov");

    assert!(
        status.success(),
        "cargo llvm-cov exited non-zero: {status}"
    );
    assert!(
        output_path.exists()
            && output_path.metadata().map(|m| m.len() > 0).unwrap_or(false),
        "llvm-cov produced empty output at {}",
        output_path.display()
    );

    // Set up an isolated engine in a TempDir.
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    ReqEngine::init(dir.path(), Some("llvm-cov-integ-test")).unwrap();
    let engine = ReqEngine::open(dir.path()).unwrap();

    // Register parent LLR requirements so insert_code_ref doesn't silently
    // drop the refs (cache.rs guards against undefined requirement IDs).
    for id in ["LLR-COVFIX-01", "LLR-COVFIX-02"] {
        let mut llr = Requirement::new(
            id.to_string(),
            RequirementType::Llr,
            format!("Coverage fixture {id}"),
        );
        llr.status = RequirementStatus::Approved;
        engine.cache().upsert_requirement(&llr).unwrap();
    }

    // Normalise path separators: llvm-cov JSON on Windows uses backslashes,
    // and coverage_map.rs normalises them to forward slashes.
    let lib_path = fixture_src_lib.to_string_lossy().replace('\\', "/");

    let covered_ref = CodeRef {
        req_id: "LLR-COVFIX-01".to_string(),
        file: PathBuf::from(&lib_path),
        line: COVERED_LINE_START,
        line_end: Some(COVERED_LINE_END),
        hash: None,
        symbol: Some("add_numbers".to_string()),
    };
    let uncovered_ref = CodeRef {
        req_id: "LLR-COVFIX-02".to_string(),
        file: PathBuf::from(&lib_path),
        line: UNCOVERED_LINE_START,
        line_end: Some(UNCOVERED_LINE_END),
        hash: None,
        symbol: Some("never_called".to_string()),
    };

    engine.cache().insert_code_ref(&covered_ref).unwrap();
    engine.cache().insert_code_ref(&uncovered_ref).unwrap();

    // Drive through the public engine facade — never call coverage_map directly.
    let scores = engine
        .audit_coverage(&output_path)
        .expect("audit_coverage should succeed on real llvm-cov output");

    let covered = scores
        .iter()
        .find(|s| s.req_id == "LLR-COVFIX-01")
        .expect("expected LineCoverageScore for LLR-COVFIX-01 (add_numbers)");

    let uncovered = scores
        .iter()
        .find(|s| s.req_id == "LLR-COVFIX-02")
        .expect("expected LineCoverageScore for LLR-COVFIX-02 (never_called)");

    assert!(
        covered.lines_hit > 0,
        "add_numbers should have lines_hit > 0 (called by #[test]); got {covered:?}"
    );
    assert_eq!(
        uncovered.lines_hit, 0,
        "never_called should have lines_hit == 0 (never invoked); got {uncovered:?}"
    );
}

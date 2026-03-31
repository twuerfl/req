//! CLI command surface tests — verifies all subcommands, flags, and binary name.
//! Also contains CI exit code tests (TST-0028) and pre-commit hook tests (TST-0029).
// REQ: TST-0027
// REQ: TST-0028
// REQ: TST-0029
// VERIFIES: LLR-0006
// VERIFIES: LLR-0010
// VERIFIES: LLR-0011

use req_cli::hooks::{install_hook_in, uninstall_hook_in};
use req_engine::{CodeRef, ReqEngine, Requirement, RequirementStatus, RequirementType};
use std::path::PathBuf;
use std::process::Command;

// ── helper ────────────────────────────────────────────────────────────────────

fn req_bin() -> PathBuf {
    let exe = if cfg!(windows) { "req.exe" } else { "req" };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("debug")
        .join(exe)
}

fn req() -> Command {
    Command::new(req_bin())
}

fn help_output() -> String {
    let out = req().arg("--help").output().expect("failed to run req --help");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

// ── TST-0027: CLI command surface ─────────────────────────────────────────────

/// TC-001 — All 19 commands are present in `req --help`.
// VERIFIES: TST-0027
#[test]
fn help_lists_all_commands() {
    let help = help_output();

    let expected = [
        "init",
        "new",
        "scan",
        "trace",
        "coverage",
        "gaps",
        "impact",
        "check",
        "export",
        "import",
        "import-reqif",
        "export-reqif",
        "list",
        "hooks",
        "ci",
        "import-ai",
        "check-provenance",
        "migrate",
        "audit",
    ];

    for cmd in expected {
        assert!(
            help.contains(cmd),
            "`req --help` is missing command: {cmd}\nFull output:\n{help}"
        );
    }
}

/// TC-002 — Global --base and --format flags are present in `req --help`.
// VERIFIES: TST-0027
#[test]
fn help_contains_global_flags() {
    let help = help_output();
    assert!(help.contains("--base"), "`req --help` missing --base flag");
    assert!(help.contains("--format"), "`req --help` missing --format flag");
}

/// TC-003 — Global --strict flag is present in `req --help`.
// VERIFIES: TST-0027
#[test]
fn help_contains_strict_flag() {
    let help = help_output();
    assert!(help.contains("--strict"), "`req --help` missing --strict flag");
}

/// TC-004 — `req audit --help` lists all 6 audit subcommands.
// VERIFIES: TST-0027
#[test]
fn audit_help_lists_all_subcommands() {
    let out = req()
        .args(["audit", "--help"])
        .output()
        .expect("failed to run req audit --help");
    let help = String::from_utf8_lossy(&out.stdout).into_owned();

    let expected = [
        "triviality",
        "criteria",
        "mutation",
        "coverage",
        "export-context",
        "independence",
    ];

    for sub in expected {
        assert!(
            help.contains(sub),
            "`req audit --help` is missing subcommand: {sub}\nFull output:\n{help}"
        );
    }
}

/// TC-005 — The binary is named "req": usage line starts with "req".
// VERIFIES: TST-0027
#[test]
fn binary_name_is_req() {
    let help = help_output();
    // clap prints "Usage: req [OPTIONS]..." or "Usage: req.exe [OPTIONS]..."
    assert!(
        help.contains("Usage: req"),
        "binary name should be 'req', got:\n{help}"
    );
    assert!(
        !help.contains("Usage: req_cli"),
        "binary must not be named 'req_cli'"
    );
}

/// TC-006 — `req --help` exits with status 0.
// VERIFIES: TST-0027
#[test]
fn help_exits_zero() {
    let status = req()
        .arg("--help")
        .status()
        .expect("failed to run req --help");
    assert!(status.success(), "req --help should exit 0");
}

/// TC-007 — `req <unknown-command>` exits non-zero.
// VERIFIES: TST-0027
#[test]
fn unknown_command_exits_nonzero() {
    let status = req()
        .arg("does-not-exist")
        .status()
        .expect("failed to run req");
    assert!(!status.success(), "unknown command should exit non-zero");
}

// ── TST-0028: CI exit code tests ──────────────────────────────────────────────

fn make_engine() -> (ReqEngine, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    ReqEngine::init(dir.path(), Some("ci-test")).unwrap();
    let engine = ReqEngine::open(dir.path()).unwrap();
    (engine, dir)
}

/// TC-001 — validate() returns empty issues on a fully-linked project.
// VERIFIES: TST-0028
#[test]
fn validate_clean_project_returns_no_issues() {
    let (engine, _dir) = make_engine();

    // HLR → LLR chain
    let hlr = Requirement::new("HLR-CI-01".to_string(), RequirementType::Hlr, "CI HLR".to_string());
    engine.cache().upsert_requirement(&hlr).unwrap();

    let mut llr = Requirement::new("LLR-CI-01".to_string(), RequirementType::Llr, "CI LLR".to_string());
    llr.parent = Some("HLR-CI-01".to_string());
    llr.status = RequirementStatus::Approved;
    engine.cache().upsert_requirement(&llr).unwrap();

    // Code ref for the LLR (prevents "not implemented" warning)
    let cr = CodeRef {
        req_id: "LLR-CI-01".to_string(),
        file: PathBuf::from("src/lib.rs"),
        line: 1,
        line_end: None,
        hash: None,
        symbol: None,
    };
    engine.cache().insert_code_ref(&cr).unwrap();

    let issues = engine.validate().unwrap();
    assert!(
        issues.is_empty(),
        "clean project should have no validation issues, got: {:?}",
        issues
    );
}

/// TC-002 — validate() reports an Error-severity issue when a link is broken.
///
/// `cache.insert_code_ref` silently drops refs to undefined requirements, so the
/// canonical way to produce an Error-severity broken-traceability condition is an
/// LLR whose declared parent ID does not exist in the requirement store.
// VERIFIES: TST-0028
#[test]
fn validate_broken_link_returns_error_issue() {
    use req_engine::Severity;

    let (engine, _dir) = make_engine();

    // LLR references a parent that was never created → llr_missing_parent → Error
    let mut llr = Requirement::new("LLR-CI-02".to_string(), RequirementType::Llr, "Broken LLR".to_string());
    llr.parent = Some("HLR-DOES-NOT-EXIST".to_string());
    engine.cache().upsert_requirement(&llr).unwrap();

    let issues = engine.validate().unwrap();
    assert!(
        !issues.is_empty(),
        "broken parent link should produce at least one validation issue"
    );

    let has_error = issues.iter().any(|i| i.severity == Severity::Error);
    assert!(
        has_error,
        "expected at least one Error-severity issue, got: {:?}",
        issues
    );

    let mentions_id = issues.iter().any(|i| i.message.contains("LLR-CI-02"));
    assert!(
        mentions_id,
        "expected an issue mentioning 'LLR-CI-02', got: {:?}",
        issues
    );
}

/// TC-003 — Warning-level issues (HLR without LLR) are not Error severity.
// VERIFIES: TST-0028
#[test]
fn validate_warning_issue_is_not_error_severity() {
    use req_engine::Severity;

    let (engine, _dir) = make_engine();

    // An HLR with no LLR child generates a Warning, not an Error
    let hlr = Requirement::new("HLR-CI-03".to_string(), RequirementType::Hlr, "Warning HLR".to_string());
    engine.cache().upsert_requirement(&hlr).unwrap();

    let issues = engine.validate().unwrap();

    let has_warning = issues.iter().any(|i| i.severity == Severity::Warning);
    assert!(has_warning, "expected at least one Warning issue, got: {:?}", issues);

    let has_error = issues.iter().any(|i| i.severity == Severity::Error);
    assert!(!has_error, "no Error-severity issues expected for warning-only scenario, got: {:?}", issues);
}

// ── TST-0029: Pre-commit hook tests ──────────────────────────────────────────

/// TC-001 — install creates .git/hooks/pre-commit and it is non-empty.
// VERIFIES: TST-0029
#[test]
fn hook_install_creates_pre_commit_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".git").join("hooks")).unwrap();

    install_hook_in(dir.path(), false).expect("install_hook_in should succeed");

    let hook_path = dir.path().join(".git").join("hooks").join("pre-commit");
    assert!(hook_path.exists(), ".git/hooks/pre-commit should exist after install");

    let content = std::fs::read_to_string(&hook_path).unwrap();
    assert!(!content.is_empty(), "hook file should be non-empty");
}

/// TC-002 — uninstall removes the hook file.
// VERIFIES: TST-0029
#[test]
fn hook_uninstall_removes_pre_commit_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".git").join("hooks")).unwrap();

    install_hook_in(dir.path(), false).unwrap();
    uninstall_hook_in(dir.path()).expect("uninstall_hook_in should succeed");

    let hook_path = dir.path().join(".git").join("hooks").join("pre-commit");
    assert!(!hook_path.exists(), ".git/hooks/pre-commit should be gone after uninstall");
}

/// TC-003 — uninstall when no hook file exists returns Ok (no panic).
// VERIFIES: TST-0029
#[test]
fn hook_uninstall_absent_file_is_safe() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".git").join("hooks")).unwrap();

    let result = uninstall_hook_in(dir.path());
    assert!(result.is_ok(), "uninstall with no hook present should return Ok, got: {:?}", result);
}

/// TC-004 — install with strict=true writes --strict into the hook content.
// VERIFIES: TST-0029
#[test]
fn hook_install_strict_writes_strict_flag() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".git").join("hooks")).unwrap();

    install_hook_in(dir.path(), true).expect("install_hook_in strict should succeed");

    let hook_path = dir.path().join(".git").join("hooks").join("pre-commit");
    let content = std::fs::read_to_string(&hook_path).unwrap();
    assert!(
        content.contains("--strict"),
        "strict hook should contain '--strict', got:\n{content}"
    );
}

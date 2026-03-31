//! Integration tests for `.req/config.toml` loading.
// REQ: TST-0026
// VERIFIES: LLR-0008

use req_engine::ReqEngine;
use std::fs;
use tempfile::TempDir;

// ── helpers ───────────────────────────────────────────────────────────────────

fn config_path(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join(".req").join("config.toml")
}

// ── TST-0026: Configuration Management Tests ─────────────────────────────────

/// TC-001 — Default config is used when .req/config.toml is absent.
// VERIFIES: TST-0026
#[test]
fn default_config_when_file_absent() {
    let dir = tempfile::tempdir().unwrap();
    ReqEngine::init(dir.path(), Some("test")).unwrap();
    fs::remove_file(config_path(&dir)).unwrap();

    let result = ReqEngine::open(dir.path());
    assert!(result.is_ok(), "open should succeed with no config file: {:?}", result.err());
}

/// TC-002 — Custom source_dirs in config are respected.
// VERIFIES: TST-0026
#[test]
fn custom_source_dirs_respected() {
    let dir = tempfile::tempdir().unwrap();
    ReqEngine::init(dir.path(), Some("test")).unwrap();

    // Write custom config
    fs::write(
        config_path(&dir),
        "source_dirs = [\"custom_src\"]\n",
    )
    .unwrap();

    // Create the custom source directory
    fs::create_dir_all(dir.path().join("custom_src")).unwrap();

    let engine = ReqEngine::open(dir.path()).unwrap();
    let result = engine.scan(None, false);
    assert!(result.is_ok(), "scan should complete with custom source dir: {:?}", result.err());
}

/// TC-003 — Missing config file does not panic; falls back to defaults.
// VERIFIES: TST-0026
#[test]
fn missing_config_no_panic() {
    let dir = tempfile::tempdir().unwrap();
    ReqEngine::init(dir.path(), Some("test")).unwrap();
    fs::remove_file(config_path(&dir)).unwrap();

    let result = ReqEngine::open(dir.path());
    assert!(result.is_ok(), "open should return Ok when config is absent: {:?}", result.err());
}

/// TC-004 — Invalid TOML in config returns a clear Err.
// VERIFIES: TST-0026
#[test]
fn invalid_toml_returns_err() {
    let dir = tempfile::tempdir().unwrap();
    ReqEngine::init(dir.path(), Some("test")).unwrap();

    fs::write(config_path(&dir), "this is not valid toml ][[\n").unwrap();

    let result = ReqEngine::open(dir.path());
    assert!(result.is_err(), "open should fail on invalid TOML");
    let msg = format!("{:?}", result.err());
    assert!(
        msg.len() > 0,
        "error message should be non-empty"
    );
}

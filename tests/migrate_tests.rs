//! Integration tests for `engine.migrate()` and backward-compatible import.
// REQ: TST-0022
// VERIFIES: LLR-0016

use req_engine::adapter::markdown::MarkdownAdapter;
use req_engine::ReqEngine;
use std::path::Path;
use tempfile::TempDir;

// ── helpers ───────────────────────────────────────────────────────────────────

fn make_engine() -> (ReqEngine, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    ReqEngine::init(dir.path(), Some("test-project")).unwrap();
    let engine = ReqEngine::open(dir.path()).unwrap();
    (engine, dir)
}

/// Write a requirement file in non-canonical format (bare title, no JSON quotes).
fn write_noncanonical(dir: &Path, id: &str) {
    let req_dir = dir.join("requirements").join("llr");
    std::fs::create_dir_all(&req_dir).unwrap();
    // The canonical format uses serde_json quoting for title: `title: "My Title"`
    // A legacy / hand-written file may omit the quotes: `title: My Title`
    let content = format!(
        "---\nid: {id}\ntype: llr\ntitle: Legacy Title\nstatus: draft\n---\n\nBody.\n"
    );
    std::fs::write(req_dir.join(format!("{id}.md")), content).unwrap();
}

// ── TST-0022: Migrate Tests ───────────────────────────────────────────────────

/// TC-001 — dry_run reports the file as needing migration without writing it.
// VERIFIES: TST-0022
#[test]
fn migrate_dry_run_does_not_write() {
    let (engine, dir) = make_engine();
    write_noncanonical(dir.path(), "LLR-MIG-01");

    let result = engine.migrate(true).unwrap();

    assert!(result.dry_run);
    assert!(!result.migrated.is_empty(), "expected at least one file to migrate");

    // File on disk must be unchanged (still non-canonical).
    let path = dir.path().join("requirements/llr/LLR-MIG-01.md");
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(
        content.contains("title: Legacy Title"),
        "dry_run must not rewrite the file"
    );
}

/// TC-002 — actual migrate rewrites the non-canonical file.
// VERIFIES: TST-0022
#[test]
fn migrate_rewrites_noncanonical_file() {
    let (engine, dir) = make_engine();
    write_noncanonical(dir.path(), "LLR-MIG-02");

    let result = engine.migrate(false).unwrap();

    assert!(!result.migrated.is_empty());
    assert!(result.errors.is_empty());

    // Rewritten file must parse cleanly.
    let path = dir.path().join("requirements/llr/LLR-MIG-02.md");
    let content = std::fs::read_to_string(&path).unwrap();
    let adapter = MarkdownAdapter::new();
    let req = adapter.parse_content(&content, None).unwrap();
    assert_eq!(req.id, "LLR-MIG-02");
}

/// TC-003 — second migrate run reports zero files to migrate (idempotent).
// VERIFIES: TST-0022
#[test]
fn migrate_is_idempotent() {
    let (engine, dir) = make_engine();
    write_noncanonical(dir.path(), "LLR-MIG-03");

    engine.migrate(false).unwrap();
    let second = engine.migrate(false).unwrap();

    assert!(
        second.migrated.is_empty(),
        "second migrate run should report no changes"
    );
}

/// TC-004 — import accepts a Markdown file with only the required fields.
// VERIFIES: TST-0022
#[test]
fn import_accepts_minimal_frontmatter() {
    let content = "---\nid: LLR-MIN-01\ntype: llr\ntitle: \"Minimal\"\nstatus: draft\n---\n";
    let adapter = MarkdownAdapter::new();
    let req = adapter.parse_content(content, None).unwrap();
    assert_eq!(req.id, "LLR-MIN-01");
    assert!(req.parent.is_none());
    assert!(req.aliases.is_empty());
    assert!(req.attributes.is_empty());
}

/// TC-005 — unknown YAML fields are preserved in the attributes map.
// VERIFIES: TST-0022
#[test]
fn import_preserves_unknown_fields_in_attributes() {
    let content = "---\nid: LLR-ATTR-01\ntype: llr\ntitle: \"Attrs\"\nstatus: draft\nattributes:\n  custom_field: \"foo\"\n---\n";
    let adapter = MarkdownAdapter::new();
    let req = adapter.parse_content(content, None).unwrap();
    assert_eq!(
        req.attributes.get("custom_field").map(String::as_str),
        Some("foo")
    );
}

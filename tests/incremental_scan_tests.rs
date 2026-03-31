//! Integration tests for hash-based incremental scan.
// VERIFIES: LLR-0033

use req_engine::{ReqEngine, Requirement, RequirementType};
use std::fs;
use tempfile::TempDir;

fn setup(tmp: &TempDir) -> ReqEngine {
    let base = tmp.path();
    ReqEngine::init(base, Some("test")).unwrap();
    ReqEngine::open(base).unwrap()
}

fn write_src(base: &std::path::Path, rel: &str, content: &str) {
    let full = base.join(rel);
    if let Some(p) = full.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(full, content).unwrap();
}

fn seed_req(engine: &ReqEngine, id: &str) {
    engine
        .cache()
        .upsert_requirement(&Requirement::new(
            id.to_string(),
            RequirementType::Llr,
            format!("Title {id}"),
        ))
        .unwrap();
}

/// TC-0040-01: hash stored in file_hashes after first scan
// VERIFIES: LLR-0033
#[test]
fn test_hash_stored_after_first_scan() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    let base = tmp.path();
    seed_req(&engine, "LLR-0001");
    write_src(base, "src/a.rs", "// REQ: LLR-0001\nfn a() {}\n");

    let result = engine.scan(Some(&base.join("src")), false).unwrap();
    assert!(result.files_scanned >= 1);

    let hash = engine.cache().get_file_hash(&base.join("src").join("a.rs")).unwrap();
    assert!(hash.is_some(), "hash must be stored after scan");
    assert!(!hash.unwrap().is_empty());
}

/// TC-0040-02: unchanged file is skipped on second scan — hash and ref count unchanged
// VERIFIES: LLR-0033
#[test]
fn test_unchanged_file_skipped_on_second_scan() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    let base = tmp.path();
    seed_req(&engine, "LLR-0001");
    write_src(base, "src/b.rs", "// REQ: LLR-0001\nfn b() {}\n");
    let src = base.join("src");

    let r1 = engine.scan(Some(&src), false).unwrap();
    let h1 = engine.cache().get_file_hash(&base.join("src").join("b.rs")).unwrap().unwrap();

    let r2 = engine.scan(Some(&src), false).unwrap();
    let h2 = engine.cache().get_file_hash(&base.join("src").join("b.rs")).unwrap().unwrap();

    assert_eq!(r1.refs_found, r2.refs_found, "ref count must be stable on second scan");
    assert_eq!(h1, h2, "hash must be unchanged when file is unchanged");
}

/// TC-0040-03: modified file is re-parsed and hash updated
// VERIFIES: LLR-0033
#[test]
fn test_modified_file_reparsed_and_hash_updated() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    let base = tmp.path();
    seed_req(&engine, "LLR-0001");
    seed_req(&engine, "LLR-0002");
    let file = base.join("src").join("c.rs");
    let src = base.join("src");
    write_src(base, "src/c.rs", "// REQ: LLR-0001\nfn c() {}\n");

    let r1 = engine.scan(Some(&src), false).unwrap();
    let h1 = engine.cache().get_file_hash(&file).unwrap().unwrap();

    fs::write(&file, "// REQ: LLR-0001\n// REQ: LLR-0002\nfn c() {}\n").unwrap();

    let r2 = engine.scan(Some(&src), false).unwrap();
    let h2 = engine.cache().get_file_hash(&file).unwrap().unwrap();

    assert!(r2.refs_found > r1.refs_found, "modified file must yield more refs");
    assert_ne!(h1, h2, "hash must change when file changes");
}

/// TC-0040-04: scan with clear=true forces full re-scan, hashes repopulated
// VERIFIES: LLR-0033
#[test]
fn test_clear_forces_full_rescan() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    let base = tmp.path();
    seed_req(&engine, "LLR-0001");
    write_src(base, "src/d.rs", "// REQ: LLR-0001\nfn d() {}\n");
    let src = base.join("src");

    engine.scan(Some(&src), false).unwrap();

    let result = engine.scan(Some(&src), true).unwrap();
    assert!(result.files_scanned >= 1, "clear scan must parse files");

    let hash = engine.cache().get_file_hash(&base.join("src").join("d.rs")).unwrap();
    assert!(hash.is_some(), "hash repopulated after clear scan");
}

/// TC-0040-05: incremental scan produces same ref count as clear scan
// VERIFIES: LLR-0033
#[test]
fn test_incremental_matches_clear_scan() {
    let tmp = TempDir::new().unwrap();
    let engine = setup(&tmp);
    let base = tmp.path();
    seed_req(&engine, "LLR-0001");
    write_src(base, "src/e.rs", "// REQ: LLR-0001\nfn e() {}\n");
    let src = base.join("src");

    let full = engine.scan(Some(&src), true).unwrap();
    let incremental = engine.scan(Some(&src), false).unwrap();

    assert_eq!(
        full.refs_found, incremental.refs_found,
        "incremental scan must agree with full scan on ref count"
    );
}

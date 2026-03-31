//! Integration tests for database lock detection and error reporting.
// VERIFIES: LLR-0036

use req_engine::cache::Cache;
use req_engine::{Error, ReqEngine};
use tempfile::TempDir;

/// TC-0037-01 / TC-0037-03: opening a locked cache.db returns DatabaseLocked, not a generic error
// VERIFIES: LLR-0036
#[test]
fn test_cache_open_locked_db_returns_database_locked() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("locked.db");

    // Hold an exclusive transaction so the second open cannot acquire a write lock
    let holder = rusqlite::Connection::open(&db_path).unwrap();
    holder.execute_batch("BEGIN EXCLUSIVE TRANSACTION").unwrap();

    let result = Cache::open(&db_path, 0);
    assert!(
        matches!(result, Err(Error::DatabaseLocked)),
        "Cache::open on a locked db must return Error::DatabaseLocked"
    );

    holder.execute_batch("ROLLBACK").unwrap();
}

/// TC-0037-02: error message names the MCP server as likely lock holder
// VERIFIES: LLR-0036
#[test]
fn test_database_locked_error_message_mentions_mcp_server() {
    let msg = Error::DatabaseLocked.to_string();
    assert!(
        msg.contains("MCP server"),
        "DatabaseLocked message must mention 'MCP server', got: {msg}"
    );
}

/// TC-0037-04: busy_timeout=0 (--wait 0) fails immediately without sleeping
// VERIFIES: LLR-0036
#[test]
fn test_wait_zero_fails_immediately() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("busy.db");

    let holder = rusqlite::Connection::open(&db_path).unwrap();
    holder.execute_batch("BEGIN EXCLUSIVE TRANSACTION").unwrap();

    let start = std::time::Instant::now();
    let result = Cache::open(&db_path, 0);
    let elapsed = start.elapsed();

    assert!(
        matches!(result, Err(Error::DatabaseLocked)),
        "must fail with DatabaseLocked"
    );
    assert!(
        elapsed.as_millis() < 500,
        "busy_timeout=0 must fail fast, took {}ms",
        elapsed.as_millis()
    );

    holder.execute_batch("ROLLBACK").unwrap();
}

/// TC-0037-05: open_with_wait retries and succeeds after lock is released
// VERIFIES: LLR-0036
#[test]
fn test_open_with_wait_succeeds_after_lock_released() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path();
    ReqEngine::init(base, Some("test")).unwrap();

    let db_path = base.join(".req/cache.db");
    let holder = rusqlite::Connection::open(&db_path).unwrap();
    holder.execute_batch("BEGIN EXCLUSIVE TRANSACTION").unwrap();

    // Release the lock from a background thread after 200 ms
    let holder_moved = holder;
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        holder_moved.execute_batch("ROLLBACK").unwrap();
    });

    // open_with_wait(2) should retry and eventually succeed
    let result = ReqEngine::open_with_wait(base, 2);
    assert!(result.is_ok(), "open_with_wait must succeed once lock is released");
}

/// TC-0037-06: open_with_wait times out when lock is never released
// VERIFIES: LLR-0036
#[test]
fn test_open_with_wait_times_out() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path();
    ReqEngine::init(base, Some("test")).unwrap();

    let db_path = base.join(".req/cache.db");
    let holder = rusqlite::Connection::open(&db_path).unwrap();
    holder.execute_batch("BEGIN EXCLUSIVE TRANSACTION").unwrap();

    let start = std::time::Instant::now();
    let result = ReqEngine::open_with_wait(base, 1);
    let elapsed = start.elapsed();

    assert!(
        matches!(result, Err(Error::DatabaseLocked)),
        "must return DatabaseLocked after timeout"
    );
    assert!(
        elapsed.as_secs() >= 1,
        "must have waited at least 1 second, took {}ms",
        elapsed.as_millis()
    );

    holder.execute_batch("ROLLBACK").unwrap();
}

/// TC-0037-07: DatabaseLocked error is propagated, not silently swallowed
// VERIFIES: LLR-0036
#[test]
fn test_database_locked_error_is_not_swallowed() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("swallow.db");

    let holder = rusqlite::Connection::open(&db_path).unwrap();
    holder.execute_batch("BEGIN EXCLUSIVE TRANSACTION").unwrap();

    let result = Cache::open(&db_path, 0);

    // Must be Err — never Ok or a generic wrapped error that loses the type
    assert!(result.is_err(), "lock error must not be silently swallowed");
    assert!(
        matches!(result, Err(Error::DatabaseLocked)),
        "error kind must be DatabaseLocked, not Database(_) or other variant"
    );

    holder.execute_batch("ROLLBACK").unwrap();
}

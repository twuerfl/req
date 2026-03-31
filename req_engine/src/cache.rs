//! SQLite cache layer for fast traceability queries.
//!
//! The cache stores:
//! - Requirements (parsed from Markdown)
//! - Code references (from scanned source files)
//! - File hashes (for incremental scanning)

use crate::{Error, Result};
use req_lib::{CodeRef, Coverage, Link, LinkType, Requirement, RequirementType};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::{Path, PathBuf};

/// Database schema version
const SCHEMA_VERSION: i32 = 3;

/// SQLite cache for requirements and traceability data
pub struct Cache {
    conn: Connection,
    path: PathBuf,
}

impl Cache {
    // REQ: LLR-0004
    /// Open or create the cache database.
    ///
    /// `busy_timeout_ms` is passed directly to SQLite's `busy_timeout` pragma.
    /// When 0 the first lock contention immediately returns `Error::DatabaseLocked`.
    pub fn open(path: &Path, busy_timeout_ms: u32) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // REQ: LLR-0036
        let conn = Connection::open(path).map_err(map_lock_error)?;

        // Tell SQLite to sleep-and-retry for up to busy_timeout_ms before
        // surfacing SQLITE_BUSY.  When 0 it fails immediately (default).
        conn.execute_batch(&format!("PRAGMA busy_timeout = {};", busy_timeout_ms))
            .map_err(map_lock_error)?;

        let cache = Self {
            conn,
            path: path.to_path_buf(),
        };

        cache.initialize()?;
        Ok(cache)
    }

    /// Initialize the database schema
    fn initialize(&self) -> Result<()> {
        self.conn.execute_batch(&format!(
            r#"
            CREATE TABLE IF NOT EXISTS requirements (
                id TEXT PRIMARY KEY,
                type TEXT NOT NULL,
                title TEXT NOT NULL,
                text TEXT,
                status TEXT NOT NULL DEFAULT 'draft',
                parent TEXT,
                source_file TEXT,
                created TEXT NOT NULL,
                modified TEXT NOT NULL,
                import_status TEXT
            );

            CREATE TABLE IF NOT EXISTS aliases (
                req_id TEXT NOT NULL,
                alias TEXT NOT NULL,
                PRIMARY KEY (req_id, alias),
                FOREIGN KEY (req_id) REFERENCES requirements(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS attributes (
                req_id TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT,
                PRIMARY KEY (req_id, key),
                FOREIGN KEY (req_id) REFERENCES requirements(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS code_refs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                req_id TEXT NOT NULL,
                file TEXT NOT NULL,
                line INTEGER NOT NULL,
                line_end INTEGER,
                hash TEXT,
                symbol TEXT,
                FOREIGN KEY (req_id) REFERENCES requirements(id) ON DELETE CASCADE
            );

            -- Source can be a requirement ID or a file path (for VERIFIES tags)
            CREATE TABLE IF NOT EXISTS links (
                source TEXT NOT NULL,
                target TEXT NOT NULL,
                type TEXT NOT NULL,
                PRIMARY KEY (source, target, type)
            );

            CREATE TABLE IF NOT EXISTS file_hashes (
                path TEXT PRIMARY KEY,
                hash TEXT NOT NULL,
                scanned_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS criterion_links (
                req_id          TEXT    NOT NULL,
                criterion_index INTEGER NOT NULL,
                file            TEXT    NOT NULL,
                line            INTEGER NOT NULL,
                PRIMARY KEY (req_id, criterion_index, file, line)
            );

            CREATE TABLE IF NOT EXISTS import_sources (
                req_id       TEXT PRIMARY KEY,
                source_path  TEXT NOT NULL,
                sha256       TEXT NOT NULL,
                imported_at  TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER NOT NULL
            );

            INSERT OR IGNORE INTO schema_version (version) VALUES ({});
        "#,
            SCHEMA_VERSION
        )).map_err(map_lock_error)?;

        self.conn.execute_batch(
            r#"
            CREATE INDEX IF NOT EXISTS idx_requirements_type ON requirements(type);
            CREATE INDEX IF NOT EXISTS idx_requirements_parent ON requirements(parent);
            CREATE INDEX IF NOT EXISTS idx_code_refs_req_id ON code_refs(req_id);
            CREATE INDEX IF NOT EXISTS idx_code_refs_file ON code_refs(file);
            CREATE INDEX IF NOT EXISTS idx_links_source ON links(source);
            CREATE INDEX IF NOT EXISTS idx_links_target ON links(target);
            CREATE INDEX IF NOT EXISTS idx_criterion_links_req_id ON criterion_links(req_id);
        "#,
        ).map_err(map_lock_error)?;

        // Schema migration: add criterion_links table if upgrading from v1
        self.migrate()?;

        Ok(())
    }

    /// Run forward-only schema migrations.
    fn migrate(&self) -> Result<()> {
        let current: i32 = self
            .conn
            .query_row(
                "SELECT version FROM schema_version LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(1);

        if current < 2 {
            self.conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS criterion_links (
                    req_id          TEXT    NOT NULL,
                    criterion_index INTEGER NOT NULL,
                    file            TEXT    NOT NULL,
                    line            INTEGER NOT NULL,
                    PRIMARY KEY (req_id, criterion_index, file, line)
                );
                CREATE INDEX IF NOT EXISTS idx_criterion_links_req_id
                    ON criterion_links(req_id);
                UPDATE schema_version SET version = 2;
            "#,
            ).map_err(map_lock_error)?;
        }

        if current < 3 {
            self.conn.execute_batch(
                r#"
                ALTER TABLE requirements ADD COLUMN import_status TEXT;
                CREATE TABLE IF NOT EXISTS import_sources (
                    req_id       TEXT PRIMARY KEY,
                    source_path  TEXT NOT NULL,
                    sha256       TEXT NOT NULL,
                    imported_at  TEXT NOT NULL
                );
                UPDATE schema_version SET version = 3;
            "#,
            ).map_err(map_lock_error)?;
        }

        Ok(())
    }

    /// Clear all data from the cache
    pub fn clear(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            DELETE FROM code_refs;
            DELETE FROM links;
            DELETE FROM attributes;
            DELETE FROM aliases;
            DELETE FROM requirements;
            DELETE FROM file_hashes;
        "#,
        )?;
        Ok(())
    }

    // ========== Requirements ==========

    /// Insert or update a requirement
    pub fn upsert_requirement(&self, req: &Requirement) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO requirements (id, type, title, text, status, parent, source_file, created, modified)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(id) DO UPDATE SET
                type = excluded.type,
                title = excluded.title,
                text = excluded.text,
                status = excluded.status,
                parent = excluded.parent,
                source_file = excluded.source_file,
                modified = excluded.modified
        "#,
            params![
                req.id,
                req.req_type.as_str(),
                req.title,
                req.text,
                req.status.as_str(),
                req.parent,
                req.source_file
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                req.created.to_rfc3339(),
                req.modified.to_rfc3339(),
            ],
        )?;

        for alias in &req.aliases {
            self.conn.execute(
                "INSERT OR IGNORE INTO aliases (req_id, alias) VALUES (?1, ?2)",
                params![req.id, alias],
            )?;
        }

        for (key, value) in &req.attributes {
            self.conn.execute(
                "INSERT OR REPLACE INTO attributes (req_id, key, value) VALUES (?1, ?2, ?3)",
                params![req.id, key, value],
            )?;
        }

        Ok(())
    }

    /// Get a requirement by ID
    pub fn get_requirement(&self, id: &str) -> Result<Option<Requirement>> {
        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT id, type, title, text, status, parent, source_file, created, modified
            FROM requirements WHERE id = ?1
        "#,
        )?;

        let result = stmt.query_row(params![id], |row| self.row_to_requirement(row));

        match result {
            Ok(req) => Ok(Some(req)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::Database(e)),
        }
    }

    /// Get all requirements
    pub fn get_all_requirements(&self) -> Result<Vec<Requirement>> {
        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT id, type, title, text, status, parent, source_file, created, modified
            FROM requirements ORDER BY id
        "#,
        )?;

        let requirements = stmt
            .query_map([], |row| self.row_to_requirement(row))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(requirements)
    }

    /// Get requirements by type
    pub fn get_requirements_by_type(&self, req_type: RequirementType) -> Result<Vec<Requirement>> {
        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT id, type, title, text, status, parent, source_file, created, modified
            FROM requirements WHERE type = ?1 ORDER BY id
        "#,
        )?;

        let requirements = stmt
            .query_map(params![req_type.as_str()], |row| {
                self.row_to_requirement(row)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(requirements)
    }

    /// Delete a requirement (cascades to code_refs, aliases, attributes).
    pub fn delete_requirement(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM requirements WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Return child requirement IDs and code-ref file paths that depend on `id`.
    ///
    /// Used by `remove_requirement` to warn before deletion.
    // REQ: LLR-0035
    pub fn get_dependents(&self, id: &str) -> Result<Vec<String>> {
        let mut deps: Vec<String> = Vec::new();

        // Child requirements whose parent field points to id
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id FROM requirements WHERE parent = ?1")?;
        let children: Vec<String> = stmt
            .query_map(params![id], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        deps.extend(children);

        // Code references tagging this requirement
        let mut stmt = self
            .conn
            .prepare_cached("SELECT DISTINCT file FROM code_refs WHERE req_id = ?1")?;
        let files: Vec<String> = stmt
            .query_map(params![id], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        deps.extend(files);

        Ok(deps)
    }

    /// Remove all cache entries related to `id`: links, criterion_links, and
    /// the requirement row itself (which cascades to code_refs, aliases,
    /// attributes).  Also removes any import_sources row if the table exists.
    // REQ: LLR-0035
    pub fn purge_requirement(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM links WHERE source = ?1 OR target = ?1",
            params![id],
        )?;
        self.conn.execute(
            "DELETE FROM criterion_links WHERE req_id = ?1",
            params![id],
        )?;
        // import_sources table is added by LLR-0037 migration; ignore if absent
        let _ = self.conn.execute(
            "DELETE FROM import_sources WHERE req_id = ?1",
            params![id],
        );
        self.delete_requirement(id)?;
        Ok(())
    }

    /// Get all requirement IDs
    pub fn get_all_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id FROM requirements ORDER BY id")?;

        let ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(ids)
    }

    /// Check if a requirement exists
    pub fn requirement_exists(&self, id: &str) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM requirements WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    fn row_to_requirement(&self, row: &Row) -> std::result::Result<Requirement, rusqlite::Error> {
        let type_str: String = row.get(1)?;
        let status_str: String = row.get(4)?;
        let source_file_str: Option<String> = row.get(6)?;
        let created_str: String = row.get(7)?;
        let modified_str: String = row.get(8)?;

        Ok(Requirement {
            id: row.get(0)?,
            req_type: RequirementType::from_str(&type_str).unwrap_or(RequirementType::Llr),
            title: row.get(2)?,
            text: row.get(3)?,
            status: req_lib::RequirementStatus::from_str(&status_str)
                .unwrap_or(req_lib::RequirementStatus::Draft),
            parent: row.get(5)?,
            aliases: Vec::new(),
            attributes: std::collections::HashMap::new(),
            source_file: source_file_str.map(PathBuf::from),
            created: chrono::DateTime::parse_from_rfc3339(&created_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            modified: chrono::DateTime::parse_from_rfc3339(&modified_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        })
    }

    // ========== Code References ==========

    /// Insert a code reference
    pub fn insert_code_ref(&self, code_ref: &CodeRef) -> Result<()> {
        if !self.requirement_exists(&code_ref.req_id)? {
            return Ok(());
        }

        self.conn.execute(
            r#"
            INSERT INTO code_refs (req_id, file, line, line_end, hash, symbol)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
            params![
                code_ref.req_id,
                code_ref.file.to_string_lossy().to_string(),
                code_ref.line as i32,
                code_ref.line_end.map(|l| l as i32),
                code_ref.hash,
                code_ref.symbol,
            ],
        )?;
        Ok(())
    }

    /// Get all code references
    pub fn get_all_code_refs(&self) -> Result<Vec<CodeRef>> {
        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT req_id, file, line, line_end, hash, symbol
            FROM code_refs ORDER BY file, line
        "#,
        )?;

        let refs = stmt
            .query_map([], |row| {
                Ok(CodeRef {
                    req_id: row.get(0)?,
                    file: PathBuf::from(row.get::<_, String>(1)?),
                    line: row.get::<_, i32>(2)? as usize,
                    line_end: row.get::<_, Option<i32>>(3)?.map(|l| l as usize),
                    hash: row.get(4)?,
                    symbol: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(refs)
    }

    /// Get code references for a requirement
    pub fn get_code_refs_for_requirement(&self, req_id: &str) -> Result<Vec<CodeRef>> {
        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT req_id, file, line, line_end, hash, symbol
            FROM code_refs WHERE req_id = ?1 ORDER BY file, line
        "#,
        )?;

        let refs = stmt
            .query_map(params![req_id], |row| {
                Ok(CodeRef {
                    req_id: row.get(0)?,
                    file: PathBuf::from(row.get::<_, String>(1)?),
                    line: row.get::<_, i32>(2)? as usize,
                    line_end: row.get::<_, Option<i32>>(3)?.map(|l| l as usize),
                    hash: row.get(4)?,
                    symbol: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(refs)
    }

    /// Delete all code references for a file
    pub fn delete_code_refs_for_file(&self, file: &Path) -> Result<()> {
        self.conn.execute(
            "DELETE FROM code_refs WHERE file = ?1",
            params![file.to_string_lossy().to_string()],
        )?;
        Ok(())
    }

    /// Clear all code references, VERIFIES links, and criterion links (used by scan --clear).
    pub fn clear_code_refs(&self) -> Result<()> {
        self.conn.execute_batch(
            "DELETE FROM code_refs; DELETE FROM links; DELETE FROM criterion_links;",
        )?;
        Ok(())
    }

    // ========== Criterion Links ==========

    /// Insert a criterion linkage tag.
    pub fn insert_criterion_link(
        &self,
        req_id: &str,
        criterion_index: usize,
        file: &Path,
        line: usize,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT OR IGNORE INTO criterion_links (req_id, criterion_index, file, line)
            VALUES (?1, ?2, ?3, ?4)
        "#,
            params![
                req_id,
                criterion_index as i32,
                file.to_string_lossy().to_string(),
                line as i32,
            ],
        )?;
        Ok(())
    }

    /// Get all criterion links for a requirement.
    pub fn get_criterion_links(&self, req_id: &str) -> Result<Vec<(usize, PathBuf, usize)>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT criterion_index, file, line FROM criterion_links WHERE req_id = ?1 ORDER BY criterion_index",
        )?;
        let rows = stmt
            .query_map(params![req_id], |row| {
                Ok((
                    row.get::<_, i32>(0)? as usize,
                    PathBuf::from(row.get::<_, String>(1)?),
                    row.get::<_, i32>(2)? as usize,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Delete all criterion links for a file (called during re-scan).
    pub fn delete_criterion_links_for_file(&self, file: &Path) -> Result<()> {
        self.conn.execute(
            "DELETE FROM criterion_links WHERE file = ?1",
            params![file.to_string_lossy().to_string()],
        )?;
        Ok(())
    }

    // ========== Links ==========

    /// Insert a link
    pub fn insert_link(&self, link: &Link) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT OR IGNORE INTO links (source, target, type)
            VALUES (?1, ?2, ?3)
        "#,
            params![link.source, link.target, link.link_type.as_str(),],
        )?;
        Ok(())
    }

    /// Get all links
    pub fn get_all_links(&self) -> Result<Vec<Link>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT source, target, type FROM links")?;

        let links = stmt
            .query_map([], |row| {
                let type_str: String = row.get(2)?;
                Ok(Link {
                    source: row.get(0)?,
                    target: row.get(1)?,
                    link_type: LinkType::from_str(&type_str).unwrap_or(LinkType::References),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(links)
    }

    /// Get verifies links as CodeRef objects for a specific requirement.
    ///
    /// Source files that contain `VERIFIES: <req_id>` are returned as
    /// synthetic `CodeRef`s with `line = 1` (exact line not stored in links).
    pub fn get_verifies_refs(&self, req_id: &str) -> Result<Vec<CodeRef>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT source FROM links WHERE target = ?1 AND type = 'verifies'",
        )?;
        let refs = stmt
            .query_map(params![req_id], |row| {
                let source: String = row.get(0)?;
                Ok(CodeRef {
                    req_id: req_id.to_string(),
                    file: PathBuf::from(&source),
                    line: 1,
                    line_end: None,
                    hash: None,
                    symbol: None,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(refs)
    }

    /// Get all verifies links across all requirements.
    pub fn get_all_verifies_refs(&self) -> Result<Vec<CodeRef>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT source, target FROM links WHERE type = 'verifies'",
        )?;
        let refs = stmt
            .query_map([], |row| {
                let source: String = row.get(0)?;
                let target: String = row.get(1)?;
                Ok(CodeRef {
                    req_id: target,
                    file: PathBuf::from(&source),
                    line: 1,
                    line_end: None,
                    hash: None,
                    symbol: None,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(refs)
    }

    // ========== File Hashes ==========

    // REQ: LLR-0033
    /// Get file hash
    pub fn get_file_hash(&self, path: &Path) -> Result<Option<String>> {
        let hash: Option<String> = self
            .conn
            .query_row(
                "SELECT hash FROM file_hashes WHERE path = ?1",
                params![path.to_string_lossy().to_string()],
                |row| row.get(0),
            )
            .optional()?;

        Ok(hash)
    }

    /// Update file hash
    pub fn update_file_hash(&self, path: &Path, hash: &str) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT OR REPLACE INTO file_hashes (path, hash, scanned_at)
            VALUES (?1, ?2, ?3)
        "#,
            params![
                path.to_string_lossy().to_string(),
                hash,
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Check if file has changed
    pub fn file_has_changed(&self, path: &Path, current_hash: &str) -> Result<bool> {
        let cached = self.get_file_hash(path)?;
        Ok(cached.as_deref() != Some(current_hash))
    }

    // ========== Statistics ==========

    /// Get LLRs without test coverage
    pub fn get_llrs_without_tests(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT id FROM requirements
            WHERE type = 'llr'
            AND id NOT IN (SELECT DISTINCT target FROM links WHERE type = 'verifies')
            ORDER BY id
        "#,
        )?;

        let llrs: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(llrs)
    }

    /// Calculate coverage statistics
    pub fn calculate_coverage(&self) -> Result<Coverage> {
        let hlr_total: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM requirements WHERE type = 'hlr'",
            [],
            |row| row.get::<_, i32>(0).map(|n| n as usize),
        )?;

        let llr_total: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM requirements WHERE type = 'llr'",
            [],
            |row| row.get::<_, i32>(0).map(|n| n as usize),
        )?;

        let hlr_with_llr: usize = self.conn.query_row(
            r#"
            SELECT COUNT(DISTINCT r.id) FROM requirements r
            JOIN requirements child ON child.parent = r.id
            WHERE r.type = 'hlr'
        "#,
            [],
            |row| row.get::<_, i32>(0).map(|n| n as usize),
        )?;

        let llr_implemented: usize = self.conn.query_row(
            r#"
            SELECT COUNT(DISTINCT req_id) FROM code_refs
            WHERE req_id LIKE 'LLR-%'
        "#,
            [],
            |row| row.get::<_, i32>(0).map(|n| n as usize),
        )?;

        let llr_tested: usize = self
            .conn
            .query_row(
                r#"
            SELECT COUNT(DISTINCT target) FROM links
            WHERE type = 'verifies' AND target LIKE 'LLR-%'
        "#,
                [],
                |row| row.get::<_, i32>(0).map(|n| n as usize),
            )
            .unwrap_or(0);

        Ok(Coverage {
            hlr_total,
            hlr_with_llr,
            llr_total,
            llr_implemented,
            llr_tested,
            orphan_code: 0,
        })
    }

    /// Get the cache file path
    pub fn path(&self) -> &Path {
        &self.path
    }

    // ========== Import source tracking ==========

    /// Record (or update) the origin file for an imported requirement.
    // REQ: LLR-0037
    pub fn upsert_import_source(
        &self,
        req_id: &str,
        source_path: &Path,
        sha256: &str,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO import_sources (req_id, source_path, sha256, imported_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(req_id) DO UPDATE SET
                source_path = excluded.source_path,
                sha256      = excluded.sha256,
                imported_at = excluded.imported_at
            "#,
            params![
                req_id,
                source_path.to_string_lossy().as_ref(),
                sha256,
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Return every row from `import_sources`.
    // REQ: LLR-0037
    pub fn get_all_import_sources(&self) -> Result<Vec<ImportSource>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT req_id, source_path, sha256, imported_at FROM import_sources",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let imported_at_str: String = row.get(3)?;
                Ok(ImportSource {
                    req_id: row.get(0)?,
                    source_path: PathBuf::from(row.get::<_, String>(1)?),
                    sha256: row.get(2)?,
                    imported_at: chrono::DateTime::parse_from_rfc3339(&imported_at_str)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Set (or clear) the `import_status` column for a requirement.
    ///
    /// Pass `None` to clear the flag (source is clean).
    // REQ: LLR-0037
    pub fn set_import_status(&self, req_id: &str, status: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE requirements SET import_status = ?1 WHERE id = ?2",
            params![status, req_id],
        )?;
        Ok(())
    }

    /// Remove the import-source record for a requirement.
    // REQ: LLR-0037
    pub fn delete_import_source(&self, req_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM import_sources WHERE req_id = ?1",
            params![req_id],
        )?;
        Ok(())
    }

    /// Return all requirement IDs that have a non-NULL import_status.
    // REQ: LLR-0037
    pub fn get_flagged_imports(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, import_status FROM requirements WHERE import_status IS NOT NULL",
        )?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// A record in the `import_sources` table.
#[derive(Debug)]
pub struct ImportSource {
    pub req_id: String,
    pub source_path: PathBuf,
    pub sha256: String,
    pub imported_at: chrono::DateTime<chrono::Utc>,
}

/// Map a rusqlite error to `Error::DatabaseLocked` when the SQLite error code
/// indicates the database file is busy or locked; pass all other errors through.
// REQ: LLR-0036
fn map_lock_error(e: rusqlite::Error) -> crate::Error {
    if let rusqlite::Error::SqliteFailure(ref ffi_err, _) = e {
        use rusqlite::ffi::ErrorCode;
        if matches!(ffi_err.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) {
            return crate::Error::DatabaseLocked;
        }
    }
    crate::Error::Database(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_path = temp_dir.path().join("test.db");

        let cache = Cache::open(&cache_path, 0).unwrap();

        let req = Requirement::new(
            "HLR-0001".to_string(),
            RequirementType::Hlr,
            "Test".to_string(),
        );
        cache.upsert_requirement(&req).unwrap();

        let retrieved = cache.get_requirement("HLR-0001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().title, "Test");
    }

}

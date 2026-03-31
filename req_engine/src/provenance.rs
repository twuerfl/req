//! Provenance tracking for requirement origin and integrity.
// REQ: LLR-0013
//!
//! Tracks whether requirement files were created/modified by the tool
//! or edited manually outside the tool. Used for audit evidence.

use crate::cache::Cache;
use crate::{Error, Result};
use std::fs;
use std::path::Path;

/// Lock the requirements directory (make all .md files read-only)
///
/// Returns the number of files locked.
pub fn lock_requirements(base: &Path) -> Result<usize> {
    let req_dir = base.join("requirements");

    if !req_dir.exists() {
        return Err(Error::Config(
            "requirements/ directory not found".to_string(),
        ));
    }

    let mut locked_count = 0;

    for entry in walkdir::WalkDir::new(&req_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "md") {
            set_readonly(path, true)?;
            locked_count += 1;
        }
    }

    let lock_file = base.join(".req").join(".locked");
    fs::write(
        &lock_file,
        format!(
            "Locked at: {}\nTool version: {}\n",
            chrono::Utc::now().to_rfc3339(),
            env!("CARGO_PKG_VERSION")
        ),
    )?;

    Ok(locked_count)
}

/// Unlock the requirements directory (restore write access)
///
/// Returns the number of files unlocked.
pub fn unlock_requirements(base: &Path) -> Result<usize> {
    let req_dir = base.join("requirements");

    if !req_dir.exists() {
        return Err(Error::Config(
            "requirements/ directory not found".to_string(),
        ));
    }

    let mut unlocked_count = 0;

    for entry in walkdir::WalkDir::new(&req_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "md") {
            set_readonly(path, false)?;
            unlocked_count += 1;
        }
    }

    let lock_file = base.join(".req").join(".locked");
    if lock_file.exists() {
        fs::remove_file(&lock_file)?;
    }

    Ok(unlocked_count)
}

/// Check if requirements are locked
pub fn is_locked(base: &Path) -> bool {
    base.join(".req").join(".locked").exists()
}

#[cfg(unix)]
fn set_readonly(path: &Path, readonly: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path)?;
    let mut perms = metadata.permissions();
    let mode = perms.mode();
    let new_mode = if readonly {
        mode & !0o222
    } else {
        mode | 0o644
    };
    perms.set_mode(new_mode);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(windows)]
fn set_readonly(path: &Path, readonly: bool) -> Result<()> {
    let metadata = fs::metadata(path)?;
    let mut perms = metadata.permissions();
    perms.set_readonly(readonly);
    fs::set_permissions(path, perms)?;
    Ok(())
}

/// Verify the provenance of a single requirement file
pub fn verify_provenance(
    base: &Path,
    req_id: &str,
    cache_hash: Option<&str>,
) -> Result<ProvenanceStatus> {
    let req_file = find_requirement_file(base, req_id)?;

    if !req_file.exists() {
        return Ok(ProvenanceStatus {
            file_exists: false,
            is_locked: false,
            hash_matches: true,
            has_tool_version: false,
            current_hash: None,
        });
    }

    let locked = is_locked(base);
    let current_hash = hash_file(&req_file)?;

    let hash_matches = match cache_hash {
        Some(cached) => current_hash == cached,
        None => true,
    };

    let content = fs::read_to_string(&req_file)?;
    let has_tool_version = content.contains("tool_version:");

    Ok(ProvenanceStatus {
        file_exists: true,
        is_locked: locked,
        hash_matches,
        has_tool_version,
        current_hash: Some(current_hash),
    })
}

fn find_requirement_file(base: &Path, req_id: &str) -> Result<std::path::PathBuf> {
    let prefix = req_id.split('-').next().unwrap_or("");
    let subdir = match prefix {
        "HLR" => "hlr",
        "LLR" => "llr",
        "TST" => "tst",
        _ => return Err(Error::RequirementNotFound(req_id.to_string())),
    };
    Ok(base
        .join("requirements")
        .join(subdir)
        .join(format!("{}.md", req_id)))
}

fn hash_file(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; 65536];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// Status of a requirement's provenance
#[derive(Debug, Clone)]
pub struct ProvenanceStatus {
    pub file_exists: bool,
    pub is_locked: bool,
    pub hash_matches: bool,
    pub has_tool_version: bool,
    pub current_hash: Option<String>,
}

impl ProvenanceStatus {
    /// True if the requirement appears to have been created by the tool
    pub fn is_tool_created(&self) -> bool {
        self.has_tool_version && self.hash_matches
    }

    /// True if there is evidence of manual editing outside the tool
    pub fn appears_manually_edited(&self) -> bool {
        self.file_exists && (!self.hash_matches || !self.has_tool_version)
    }
}

/// A detected provenance violation
#[derive(Debug, Clone)]
pub struct ProvenanceViolation {
    pub req_id: String,
    pub reason: String,
}

/// Check all requirements in the cache for provenance violations
pub fn check_all_provenance(base: &Path, cache: &Cache) -> Result<Vec<ProvenanceViolation>> {
    let mut violations = Vec::new();

    for req in cache.get_all_requirements()? {
        let status = verify_provenance(base, &req.id, None)?;

        if status.appears_manually_edited() {
            violations.push(ProvenanceViolation {
                req_id: req.id.clone(),
                reason: if !status.has_tool_version {
                    "Missing tool_version attribute".to_string()
                } else {
                    "File may have been manually edited".to_string()
                },
            });
        }
    }

    Ok(violations)
}

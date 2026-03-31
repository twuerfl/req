//! Code scanner for inline requirement tags.
//!
//! Scans source code files for tags like:
//! - `// REQ:: LLR-0123` - single line tag
//! - `/* REQ:: LLR-0123, LLR-0456 */` - multi-ID tag
//! - `// VERIFIES: LLR-0123` - test verification tag
//! - `// BEGIN-REQ: LLR-0123` ... `// END-REQ: LLR-0123` - block tag

use crate::{Error, Result};
use regex::Regex;
use req_lib::{CodeRef, RequirementType};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Supported source file extensions
const SOURCE_EXTENSIONS: &[&str] = &[
    "c", "h", "cpp", "hpp", "cc", "cxx", "rs", "py", "java", "js", "ts", "go", "sh", "lua",
    "rb", "php", "swift", "kt",
];

/// Tag types we can detect
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagType {
    /// Requirement implementation tag
    Req,
    /// Verification tag (for tests)
    Verifies,
    /// Acceptance criterion linkage tag (`CRITERION: LLR-XXXX #N`)
    Criterion,
}

/// A found tag in source code
#[derive(Debug, Clone)]
pub struct FoundTag {
    /// The requirement ID
    pub req_id: String,
    /// Type of tag
    pub tag_type: TagType,
    /// Line number
    pub line: usize,
    /// Optional end line for block tags
    pub line_end: Option<usize>,
    /// For `Criterion` tags: the 1-based criterion index (`#N`)
    pub criterion_index: Option<usize>,
    /// Symbol name (function name) extracted by lookahead, if the tag
    /// precedes a `fn` definition
    pub symbol: Option<String>,
}

/// Code scanner for finding inline requirement tags
pub struct CodeScanner {
    req_regex: Regex,
    verifies_regex: Regex,
    criterion_regex: Regex,
    id_regex: Regex,
    begin_req_regex: Regex,
    end_req_regex: Regex,
}

impl CodeScanner {
    /// Create a new scanner with compiled regexes
    pub fn new() -> Self {
        Self {
            req_regex: Regex::new(r"REQ:\s*([A-Z]+-\d+(?:\s*,\s*[A-Z]+-\d+)*)")
                .expect("Invalid REQ regex"),
            verifies_regex: Regex::new(r"VERIFIES:\s*([A-Z]+-\d+(?:\s*,\s*[A-Z]+-\d+)*)")
                .expect("Invalid VERIFIES regex"),
            criterion_regex: Regex::new(r"CRITERION:\s*([A-Z]+-\d+)\s*#(\d+)")
                .expect("Invalid CRITERION regex"),
            id_regex: Regex::new(r"(HLR|LLR|TST)-(\d+)").expect("Invalid ID regex"),
            begin_req_regex: Regex::new(r"BEGIN-REQ:\s*([A-Z]+-\d+)")
                .expect("Invalid BEGIN-REQ regex"),
            end_req_regex: Regex::new(r"END-REQ:\s*([A-Z]+-\d+)")
                .expect("Invalid END-REQ regex"),
        }
    }

    /// Check if a file is a source file we should scan
    pub fn is_source_file(path: &Path) -> bool {
        path.extension()
            .map(|ext| SOURCE_EXTENSIONS.contains(&ext.to_string_lossy().as_ref()))
            .unwrap_or(false)
    }

    // REQ: LLR-0002
    /// Scan a single file for requirement tags
    pub fn scan_file(&self, path: &Path) -> Result<Vec<FoundTag>> {
        let content = std::fs::read_to_string(path).map_err(Error::Io)?;

        // Collect into a Vec so the lookahead pass can do random access.
        let lines: Vec<&str> = content.lines().collect();

        let mut tags = Vec::new();
        let mut open_blocks: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for (idx, &line) in lines.iter().enumerate() {
            let line_num = idx + 1;

            if let Some(cap) = self.begin_req_regex.captures(line) {
                if let Some(id_match) = cap.get(1) {
                    open_blocks.insert(id_match.as_str().to_string(), line_num);
                }
            }

            if let Some(cap) = self.end_req_regex.captures(line) {
                if let Some(id_match) = cap.get(1) {
                    let req_id = id_match.as_str().to_string();
                    if let Some(start_line) = open_blocks.remove(&req_id) {
                        tags.push(FoundTag {
                            req_id,
                            tag_type: TagType::Req,
                            line: start_line,
                            line_end: Some(line_num),
                            criterion_index: None,
                            symbol: None,
                        });
                    }
                }
            }

            if !self.begin_req_regex.is_match(line) && !self.end_req_regex.is_match(line) {
                self.extract_tags_from_line(line, line_num, TagType::Req, &mut tags);
            }

            self.extract_tags_from_line(line, line_num, TagType::Verifies, &mut tags);
            self.extract_criterion_tags(line, line_num, &mut tags);
        }

        // Unclosed blocks — treat as single-line
        for (req_id, start_line) in open_blocks {
            tags.push(FoundTag {
                req_id,
                tag_type: TagType::Req,
                line: start_line,
                line_end: None,
                criterion_index: None,
                symbol: None,
            });
        }

        // ── Lookahead pass ────────────────────────────────────────────────────
        // For every single-line Req tag whose line is a pure comment (no
        // executable content beyond the `// REQ:` marker), advance the anchor
        // to the first executable line that follows and capture the symbol name
        // when that line contains a `fn` definition.
        for tag in tags.iter_mut() {
            if tag.tag_type != TagType::Req || tag.line_end.is_some() {
                continue; // skip block tags and non-Req tags
            }

            let tag_line = lines[tag.line - 1]; // 0-based index

            // Skip the lookahead when the tag appears inline on an executable
            // line (content before the `// REQ:` marker is non-whitespace).
            if !is_comment_only_line(tag_line) {
                continue;
            }

            // Look ahead from the line immediately after the tag.
            if let Some((exec_line, sym)) = find_executable_line(&lines, tag.line) {
                tag.line = exec_line;
                // When the executable line is a fn definition, extend line_end to
                // cover the entire function body so coverage scoring measures real
                // line hits rather than just the always-executed fn signature.
                if sym.is_some() {
                    tag.line_end = find_function_end(&lines, exec_line)
                        .map(|closing| closing + 1); // store exclusive upper bound
                }
                tag.symbol = sym;
            }
        }

        Ok(tags)
    }

    fn extract_tags_from_line(
        &self,
        line: &str,
        line_num: usize,
        tag_type: TagType,
        tags: &mut Vec<FoundTag>,
    ) {
        let regex = match tag_type {
            TagType::Req => &self.req_regex,
            TagType::Verifies => &self.verifies_regex,
            TagType::Criterion => return, // handled separately
        };

        for cap in regex.captures_iter(line) {
            if let Some(ids_str) = cap.get(1) {
                for id_cap in self.id_regex.captures_iter(ids_str.as_str()) {
                    let full_id = id_cap.get(0).unwrap().as_str().to_string();
                    tags.push(FoundTag {
                        req_id: full_id,
                        tag_type,
                        line: line_num,
                        line_end: None,
                        criterion_index: None,
                        symbol: None,
                    });
                }
            }
        }
    }

    fn extract_criterion_tags(&self, line: &str, line_num: usize, tags: &mut Vec<FoundTag>) {
        for cap in self.criterion_regex.captures_iter(line) {
            let req_id = cap.get(1).map(|m| m.as_str().to_string());
            let index = cap.get(2).and_then(|m| m.as_str().parse::<usize>().ok());
            if let (Some(req_id), Some(index)) = (req_id, index) {
                if index > 0 {
                    tags.push(FoundTag {
                        req_id,
                        tag_type: TagType::Criterion,
                        line: line_num,
                        line_end: None,
                        criterion_index: Some(index),
                        symbol: None,
                    });
                }
            }
        }
    }

    /// Scan a directory for all source files with requirement tags
    pub fn scan_directory(&self, dir: &Path) -> Result<Vec<(PathBuf, Vec<FoundTag>)>> {
        let mut results = Vec::new();

        for entry in WalkDir::new(dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            if path
                .file_name()
                .map(|n| n.to_string_lossy().starts_with('.'))
                .unwrap_or(true)
            {
                continue;
            }

            if Self::is_source_file(path) {
                let tags = self.scan_file(path)?;
                if !tags.is_empty() {
                    results.push((path.to_path_buf(), tags));
                }
            }
        }

        Ok(results)
    }

    /// Convert found tags to CodeRef objects (REQ tags only)
    pub fn tags_to_code_refs(path: &Path, tags: &[FoundTag]) -> Vec<CodeRef> {
        tags.iter()
            .filter(|t| t.tag_type == TagType::Req)
            .map(|t| CodeRef {
                req_id: t.req_id.clone(),
                file: path.to_path_buf(),
                line: t.line,
                line_end: t.line_end,
                hash: None,
                symbol: t.symbol.clone(),
            })
            .collect()
    }

    /// Calculate file hash for change detection
    pub fn hash_file(path: &Path) -> Result<String> {
        let content = std::fs::read(path).map_err(Error::Io)?;
        let hash = blake3::hash(&content);
        Ok(hash.to_hex().to_string())
    }
}

impl Default for CodeScanner {
    fn default() -> Self {
        Self::new()
    }
}

// ── Lookahead helpers ─────────────────────────────────────────────────────────

/// Returns true when the line contains only whitespace and/or comment/doc-comment
/// content — i.e. no executable code is present on this line beyond a `// REQ:` tag.
fn is_comment_only_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return true;
    }
    // Pure comment lines (// ... or /* ... or * ...)
    if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
        return true;
    }
    // Attribute lines
    if trimmed.starts_with("#[") || trimmed.starts_with("#![") {
        return true;
    }
    false
}

/// Returns true when the line should be skipped during lookahead — it carries no
/// executable meaning on its own.
fn is_lookahead_skip_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return true;
    }
    if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
        return true;
    }
    if trimmed.starts_with("#[") || trimmed.starts_with("#![") {
        return true;
    }
    // Compile-time / declaration-only forms — not executable statements.
    if trimmed.starts_with("use ")
        || trimmed.starts_with("mod ")
        || trimmed.starts_with("extern crate")
        || trimmed.starts_with("type ")
        || trimmed.starts_with("struct ")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("trait ")
        || trimmed.starts_with("const ")
        || trimmed.starts_with("static ")
        || trimmed.starts_with("impl ")
    {
        return true;
    }
    // A bare visibility modifier with nothing else (e.g. `pub` or `pub(crate)` alone)
    let no_vis = trimmed
        .trim_start_matches("pub(crate)")
        .trim_start_matches("pub(super)")
        .trim_start_matches("pub")
        .trim();
    if no_vis.is_empty() {
        return true;
    }
    // Declaration-only forms with visibility prefix
    for kw in &["struct ", "enum ", "trait ", "mod ", "use ", "type ", "const ", "static ", "impl ", "extern "] {
        if no_vis.starts_with(kw) {
            return true;
        }
    }
    false
}

/// Starting at `from_line` (1-based), scan forward through `lines` to find the
/// first line that is not a comment, blank, attribute, or bare visibility modifier.
///
/// Returns `(1-based line number, Option<function name>)` or `None` if no
/// executable line is found within the lookahead window.
fn find_executable_line(lines: &[&str], from_line: usize) -> Option<(usize, Option<String>)> {
    const MAX_LOOKAHEAD: usize = 60;

    let start_idx = from_line; // from_line is 1-based → first line after tag is index from_line
    let end_idx = (start_idx + MAX_LOOKAHEAD).min(lines.len());

    for idx in start_idx..end_idx {
        let line = lines[idx];
        if is_lookahead_skip_line(line) {
            continue;
        }
        // This is the executable line.
        let symbol = extract_fn_name(line);
        return Some((idx + 1, symbol)); // convert back to 1-based
    }
    None
}

/// Find the 1-based line number of the closing brace that ends the function
/// starting at `fn_line` (1-based).  Uses a simple brace-depth counter; does
/// not handle braces inside string literals but is correct for well-formed code.
/// Returns `None` when no closing brace is found within 300 lines.
fn find_function_end(lines: &[&str], fn_line: usize) -> Option<usize> {
    const MAX_BODY: usize = 300;
    let mut depth: i32 = 0;
    let end = (fn_line + MAX_BODY).min(lines.len());
    for (offset, &line) in lines[fn_line - 1..end].iter().enumerate() {
        for ch in line.chars() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(fn_line + offset); // 1-based, inclusive
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// If `line` contains a `fn <name>(` definition, return `<name>`.
/// Strips leading visibility/async/unsafe/extern modifiers before matching.
fn extract_fn_name(line: &str) -> Option<String> {
    let mut rest = line.trim();
    // Strip known leading modifiers
    for prefix in &["pub(crate)", "pub(super)", "pub", "async", "unsafe", "extern"] {
        rest = rest.trim_start_matches(prefix).trim();
    }
    if let Some(after_fn) = rest.strip_prefix("fn ") {
        let name: String = after_fn
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────

/// Validate that a requirement ID is well-formed
pub fn validate_req_id(id: &str) -> Result<(RequirementType, u32)> {
    let parts: Vec<&str> = id.split('-').collect();
    if parts.len() != 2 {
        return Err(Error::InvalidIdFormat(id.to_string()));
    }

    let req_type = RequirementType::from_str(parts[0])
        .ok_or_else(|| Error::InvalidIdFormat(id.to_string()))?;

    let number: u32 = parts[1]
        .parse()
        .map_err(|_| Error::InvalidIdFormat(id.to_string()))?;

    Ok((req_type, number))
}

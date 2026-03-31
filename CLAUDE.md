# Claude Code Instructions — req project

## Primary tool: req MCP server

This project ships an MCP server (`req-mcp-server`) registered in `.mcp.json`.
**Always use its tools before writing any code or running any shell commands.**
The server is the authoritative interface to the requirement database.

### When to call which tool

| Situation | Tool to call first |
| --- | --- |
| Before touching any source file | `req_scan` — refresh the traceability DB |
| Understanding what a file implements | `req_trace` on the LLR tagged in that file |
| Checking whether an LLR is covered by tests | `req_audit_coverage` with `.req/coverage-report.json` |
| Finding untested or unimplemented requirements | `req_gaps` |
| Reviewing all validation errors before a commit | `req_check` |
| Assessing blast radius of a change | `req_impact` on the affected requirement |
| Choosing what to work on next | `req_list` with `type=llr` and `status=approved` |
| Exporting context for a long analysis task | `req_export` with `format=ai-context` |

### V-model gate — mandatory before writing code

Never write implementation code or tests without first verifying the gate:

1. Call `req_list` — confirm the target LLR is `status: approved`.
   If it is `draft`, update the requirement file to `approved` first.
2. Call `req_trace` on the LLR — confirm its parent HLR exists and is `approved`.
3. Only then write code. Tag every implementing function with `// REQ: LLR-XXXX`
   placed on the line immediately before the `fn` definition (not at module level).

### Tag placement rule

Inline requirement tags must sit on the line **immediately before** the `fn`
definition they cover — never at the top of a file and never inside a struct body.
The scanner's lookahead extends the coverage window to the function's closing brace
only when the tag is on a comment-only line directly above a `fn` keyword.

```rust
// REQ: LLR-0028          ← correct: one line above fn
pub fn parse_llvm_cov_report(...) -> Result<...> {
```

### Coverage workflow

```bash
# 1. Regenerate the LLVM coverage report after running tests
cargo llvm-cov --json --output-path .req/coverage-report.json

# 2. Refresh the traceability DB
req scan --clear

# 3. Query coverage via MCP (from Claude or via CLI)
req audit coverage --report .req/coverage-report.json
```

Or call `req_audit_coverage` directly in MCP with:
```json
{ "report": "c:/01_DATA/111 req/req/.req/coverage-report.json" }
```

### Do not use these when the MCP tool exists

- Do not run `sqlite3` or read `.req/cache.db` directly — use `req_list` / `req_trace`.
- Do not parse `requirements/**/*.md` manually — use `req_list` or `req_export`.
- Do not run `req.exe` CLI subcommands in Bash when an equivalent MCP tool exists.
  Exception: `req scan --clear` (no MCP equivalent that clears the DB).

## Test placement rule

All tests must be written as integration test files in the root `tests/` directory
(e.g. `tests/remove_tests.rs`). **Never add `#[cfg(test)] mod tests { … }` blocks
inside source files** under `src/` or any sub-crate `src/`.

- One test file per requirement cluster (e.g. `tests/staleness_tests.rs` for LLR-0037).
- Tag every test function with `// VERIFIES: LLR-XXXX` on the line immediately before `#[test]`.
- The existing `#[test]` in `req_engine/src/cache.rs` (`test_cache_roundtrip`) is a legacy
  exception and must not be used as a template.

## Project conventions

- All requirements live in `requirements/hlr/`, `requirements/llr/`, `requirements/tst/`.
- The `.req/` directory holds `cache.db` and `config.toml` — do not commit `cache.db`.
- Requirement IDs: `HLR-NNNN`, `LLR-NNNN`, `TST-NNNN` (four digits, zero-padded).
- Every LLR must have `parent: HLR-XXXX` in its frontmatter.
- Every TST must have `parent: LLR-XXXX` in its frontmatter.
- Status lifecycle: `draft` → `approved` → (`deprecated` | `rejected`).
  Only `approved` requirements may be implemented.

## See also

- `CONTEXT.md` — full project description, architecture, and technology stack.
- `SKILL.md` — project-specific slash commands.

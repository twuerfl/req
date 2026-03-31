# Project Context for AI Agents

## Project Identity

**Name:** req  
**Type:** CLI Tool (Requirement Traceability)  
**Domain:** Safety-critical systems engineering (ISO 26262, DO-178, IEC 61508)  
**License:** MIT OR Apache-2.0

## Technology Stack

### Primary Language
- **Language:** Rust (Edition 2024)
- **Minimum Version:** Rust 1.70+
- **Build System:** Cargo

### Target Platforms
- **Primary:** Windows (x86_64)
- **Secondary:** Linux (x86_64), macOS (x86_64, ARM64)
- **Binary Type:** Single static binary, no runtime dependencies

### Key Dependencies
| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing (derive feature) |
| `serde` | Serialization/deserialization |
| `serde_yaml` | YAML parsing for frontmatter |
| `rusqlite` | SQLite cache database |
| `regex` | Code tag scanning |
| `walkdir` | Recursive file traversal |
| `chrono` | Timestamp handling |
| `anyhow` | Error handling |
| `toml` | Configuration files |
| `colored` | Terminal output coloring |
| `blake3` | File hashing |

### Optional Dependencies
| Crate | Feature | Purpose |
|-------|---------|---------|
| `git2` | `git` | Git integration |
| `pyo3` | `reqif` | ReqIF import/export (requires Python + `pip install reqif`) |

## Build Commands

```bash
# Development build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Run with features
cargo build --features git,reqif

# Install locally
cargo install --path .
```

## Code Style

### Formatting
- Use `cargo fmt` before commits
- Max line width: 100 characters
- Indent: 4 spaces (Rust standard)

### Linting
- Use `cargo clippy` for linting
- Treat clippy warnings as errors

### Documentation
- All public items must have doc comments (`///`)
- Module-level docs with `//!`
- Include code examples in docs

## Architecture

### Workspace Structure

The repository is a Cargo workspace with four crates (HLR-0013):

```
req_lib/src/          # QUALIFIED kernel — pure types, no I/O, #![forbid(unsafe_code)]
├── lib.rs
└── models.rs         # Requirement, CodeRef, Link, Coverage, AiExport, …

req_engine/src/       # QUALIFIED business logic — all engine operations
├── lib.rs            # pub use req_lib::*; re-exports domain types
├── engine.rs         # ReqEngine facade (main entry point for callers)
├── cache.rs          # SQLite cache layer
├── scanner.rs        # Code tag scanner
├── trace.rs          # Traceability graph, gap detection, impact analysis
├── parser.rs         # Requirement file parser
├── config.rs         # Configuration management
├── error.rs          # Error types
├── ai_import.rs      # AI suggestion import
├── provenance.rs     # Provenance tracking
└── adapter/
    ├── mod.rs        # RequirementAdapter trait
    ├── markdown.rs   # Markdown adapter
    ├── json.rs       # JSON / AI-context adapter
    └── reqif.rs      # ReqIF adapter (optional feature)

req_cli/src/          # NOT qualified — presentation layer only
├── main.rs           # Binary entry point (built as `req`)
├── cli.rs            # clap CLI definitions
├── output.rs         # Terminal output formatting
├── hooks.rs          # Git hook installation
└── …

req_mcp/src/          # NOT qualified — MCP server (LLR-0025)
├── main.rs           # Binary entry point (built as `req-mcp-server`)
├── lib.rs            # Library target (enables integration tests)
├── server.rs         # ReqServer, #[tool_router] tools, ServerHandler resources
└── error.rs          # engine_err: req_engine::Error → rmcp ErrorData
```

**Qualification boundary:** `req_lib` + `req_engine` are the qualified components under ISO 26262 / DO-178C tool qualification. `req_cli` and `req_mcp` are explicitly outside this scope.

**Dependency direction:** `req_cli` → `req_engine` → `req_lib`. `req_mcp` → `req_engine` → `req_lib`. No reverse dependencies.

### Design Patterns

- **Facade:** `ReqEngine` in `req_engine::engine` is the single entry point for all business logic
- **Adapter Pattern:** `RequirementAdapter` trait for format abstraction
- **Result Type:** Custom `Result<T>` with `Error` enum in `req_engine::error`

### Data Flow
```
Markdown Files → Parser → Requirement structs → SQLite Cache
                                                    ↓
Source Code → Scanner → CodeRefs ──────────────→ TraceGraph
                                                    ↓
                                            Reports / Export
```

## Testing

### Test Framework
- Built-in Rust tests (`#[test]`)
- Integration tests in `tests/` directory

### Test Commands
```bash
cargo test                    # All tests
cargo test --test integration # Integration tests only
cargo test -- --nocapture     # Show output
```

## Configuration

### Project Config (`.req/config.toml`)
```toml
project = "my-project"
source_dirs = ["src", "tests"]
requirements_dir = "requirements"
default_adapter = "markdown"
```

### Environment
- No environment variables required
- Config stored in `.req/` directory
- Cache database: `.req/cache.db`

## Common Tasks for AI Agents

### V-Model Gate — MANDATORY SEQUENCE

**Every change, no matter how small, must follow this order. Skipping a step is a process violation.**

```text
1. Draft/update HLR  →  requirements/hlr/HLR-XXXX.md
2. Draft/update LLR  →  requirements/llr/LLR-XXXX.md   (parent: HLR-XXXX)
3. Draft TST spec    →  requirements/tst/TST-XXXX.md   (parent: LLR-XXXX)
4. Implement code    →  tag with // REQ: LLR-XXXX
5. Implement tests   →  matching the TST definition
6. Verify coverage   →  cargo run -p req_cli -- scan && cargo run -p req_cli -- coverage
```

No LLR before HLR. No TST before LLR. No code before TST. No tests before code. No coverage check before tests.

### Adding a New Feature

1. Follow the V-Model Gate above (all 6 steps)
2. Choose the correct crate for the implementation:
   - Pure data types → `req_lib`
   - Business logic → `req_engine`
   - CLI output/formatting → `req_cli`
   - MCP server tools → `req_mcp`
3. Tag all implementing functions with `// REQ: LLR-XXXX`
4. Add integration tests in the root `tests/` directory

### Adding a New CLI Command

1. Add command variant in `req_cli/src/cli.rs` (Commands enum)
2. Implement the corresponding operation in `req_engine/src/engine.rs` (`ReqEngine`)
3. Call the engine method from the CLI handler in `req_cli/src/cli.rs`
4. Add tests

### Adding a New Adapter

1. Create new file in `req_engine/src/adapter/`
2. Implement `RequirementAdapter` trait
3. Register in `req_engine/src/adapter/mod.rs`
4. Add CLI support in `req_cli/src/cli.rs`

## File Conventions

### Requirement Files
- Location: `requirements/{hlr,llr,tst}/`
- Naming: `{TYPE}-{NUMBER}.md` (e.g., `LLR-0001.md`)
- Format: YAML frontmatter + Markdown body

### Source Files
- One module = one file
- Re-exports in `mod.rs` or `lib.rs`
- Tests in same file (`#[cfg(test)]`)

## Error Handling

Use the custom `Error` enum from `error.rs`:
```rust
pub enum Error {
    Io(std::io::Error),
    Database(rusqlite::Error),
    Parse(String),
    InvalidIdFormat(String),
    RequirementNotFound(String),
    ParentNotFound(String),
    NotInitialized,
    // ...
}
```

## Git Workflow

### Branch Names
- `feature/xxx` - New features
- `fix/xxx` - Bug fixes
- `docs/xxx` - Documentation

### Commit Messages
- Follow conventional commits
- Reference requirement IDs when applicable:
  ```
  feat: add ReqIF export support
  
  Implements HLR-0012 by adding ReqIF export capability.
  ```

## Performance Considerations

- SQLite cache enables fast queries
- File hashing for incremental scans
- Parallel scanning possible (not yet implemented)
- Target: 10k requirements, 100k code refs

## Security Considerations

- No network access required
- No external API calls
- All data stored locally
- Suitable for air-gapped environments
# req — Git-Native Requirement Traceability Tool

**`req`** is a lightweight, CLI-first requirement management tool for safety-critical and
embedded systems engineering. It bridges the gap between heavy requirement tools (DOORS,
Polarion) and modern developer workflows (Git, CI, AI coding assistants).

Requirements are treated as code: versioned, diffable, and mergeable.

---

## Key Features

- **Git-Native Storage** — Requirements live as Markdown files in your repository. No
  database server, no vendor lock-in. The SQLite cache is a rebuildable performance layer.
- **Inline Code Traceability** — Tag source code with `// REQ: LLR-XXXX`. The scanner
  stores file, line range, and symbol for every tag.
- **Full V-Model Traceability** — HLR → LLR → Code → TST chain with gap detection,
  impact analysis, and coverage reporting.
- **AI-Ready via MCP** — Ships an MCP server (`req-mcp-server`) that exposes all engine
  operations to AI coding assistants (Claude Code, GitHub Copilot, etc.) over stdio.
- **AI Output Integrity** — Static triviality detection, LLVM coverage mapping, mutation
  score correlation, acceptance-criterion linkage, and author independence checks catch
  AI-generated implementations that satisfy metrics without real functionality.
- **Safety-Standard Ready** — Built for ISO 26262, DO-178C, and IEC 61508 workflows.
  Strict mode (`--strict`) exits non-zero on any warning for CI enforcement.
- **ReqIF Interoperability** — Import/export for DOORS and Polarion via the ReqIF adapter.
- **Modular Qualified Architecture** — `req_lib` (qualified kernel) / `req_engine`
  (qualified business logic) / `req_cli` / `req_mcp` separation minimises certification scope.
- **Single Binary** — Written in Rust. Fast, offline-capable, no runtime dependencies.

---

## Installation

```bash
# Build from source (requires Rust)
git clone https://github.com/yourname/req.git
cd req
cargo build --release
# Binaries: target/release/req  and  target/release/req-mcp-server
```

---

## Quick Start

```bash
# 1. Initialize the requirement structure
req init

# 2. Create requirements
req new hlr "Motor Current Measurement"           # → requirements/hlr/HLR-0001.md
req new llr "ADC Sampling Rate" --parent HLR-0001 # → requirements/llr/LLR-0001.md

# 3. Tag your source code
# src/adc.c
// REQ: LLR-0001
void init_adc_sampling() { ... }

# 4. Scan and validate
req scan
req coverage
req check --strict

# 5. Trace a requirement
req trace LLR-0001

# 6. Find gaps
req gaps
```

---

## Command Reference

| Command | Description |
| --- | --- |
| `init` | Initialise a new requirements project |
| `new <type> <title>` | Create a new HLR, LLR, or TST requirement |
| `scan [--clear]` | Scan source code for `REQ:` / `VERIFIES:` tags |
| `import <dir>` | (Re-)load requirement Markdown files into the cache |
| `import-ai <file>` | Import AI-generated suggestions (forced `draft` status) |
| `import-reqif <file>` | Import from ReqIF (requires `reqif` feature) |
| `export --format <fmt>` | Export as `json` or `markdown`; `--id` for single requirement |
| `export-reqif` | Export to ReqIF format |
| `list [--type] [--status]` | List requirements, filterable by type and status |
| `trace <ID>` | Show full traceability tree for a requirement |
| `coverage` | HLR/LLR implementation and test coverage statistics |
| `gaps` | All traceability gaps (HLR without LLR, LLR without code, etc.) |
| `check [--strict]` | Validate all links; exits 1 on warnings in strict mode |
| `impact <ID>` | Impact analysis — what else is affected by this change |
| `remove <ID>` | Remove a requirement file and purge its cache entries |
| `migrate` | Upgrade requirement files to the current schema version |
| `check-provenance` | Validate that all requirements have `tool_version` provenance |
| `hooks install\|uninstall` | Manage the Git pre-commit traceability hook |
| `ci <type>` | Generate CI workflow files (GitHub Actions, GitLab CI, …) |
| `audit <subcommand>` | AI output integrity auditing (see below) |

### Audit Subcommands

```text
req audit triviality [--id LLR-XXXX]    # Static hollow-body detection
req audit criteria <LLR-XXXX>           # Acceptance-criterion linkage report
req audit mutation <report.json>         # Correlate cargo mutants --json output
req audit coverage <report.json>         # Correlate cargo llvm-cov --json output
req audit export-context <LLR-XXXX>     # Export full LLM-reviewable audit bundle
req audit independence [--id LLR-XXXX]  # Check impl/test author independence via git blame
```

### Global Options

| Option | Description |
| --- | --- |
| `--base <DIR>` | Override project root (default: current directory) |
| `--format text\|json` | Output format for all commands |
| `--strict` | Exit 1 on warnings |
| `--wait <seconds>` | Wait for a locked `cache.db` before failing (useful when MCP server is running) |

---

## AI Integration via MCP Server

`req-mcp-server` exposes all engine operations to AI coding assistants over the Model
Context Protocol (stdio transport). Configure it in `.mcp.json`:

```json
{
  "mcpServers": {
    "req": {
      "command": "/path/to/req-mcp-server",
      "env": { "REQ_BASE": "/path/to/your/project" }
    }
  }
}
```

Available MCP tools: `req_scan`, `req_list`, `req_trace`, `req_coverage`, `req_gaps`,
`req_check`, `req_impact`, `req_export`, `req_audit_coverage`.

---

## Project Structure

```text
my-project/
├── .req/
│   ├── config.toml        # Language patterns, excludes, adapter profiles
│   └── cache.db           # Rebuildable SQLite index (gitignored)
├── requirements/
│   ├── hlr/               # High-Level Requirements
│   │   └── HLR-0001.md
│   ├── llr/               # Low-Level Requirements
│   │   └── LLR-0001.md
│   └── tst/               # Test Requirements
│       └── TST-0001.md
├── src/
│   └── motor.c            # Source with inline REQ: tags
└── tests/                 # Test files with VERIFIES: tags
```

---

## Requirement Format

```yaml
---
id: LLR-0001
type: llr
parent: HLR-0001
status: approved          # draft → approved → deprecated | rejected
---

## Description

The ADC shall sample the motor current at 10 kHz with a tolerance of ±1%.

## Acceptance Criteria

- [ ] Sampling rate is 10 kHz ± 1%
- [ ] Jitter does not exceed 5 µs
```

Tag the implementing function:

```c
// REQ: LLR-0001
void init_adc_sampling() { ... }
```

Tag the verifying test:

```rust
// VERIFIES: TST-0001
#[test]
fn test_adc_sampling_rate() { ... }
```

---

## Adapter Support

| Adapter | Status | Description |
| --- | --- | --- |
| **Markdown** | Implemented | Native format — human-readable, Git-diffable |
| **JSON** | Implemented | CI pipelines and AI export |
| **ReqIF** | Implemented | Import/Export for DOORS/Polarion (build with `--features reqif`) |
| **Jira** | Future | Issue sync |

---

## License

Licensed under MIT OR Apache-2.0.

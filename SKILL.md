# req Tool - AI Agent Skill Guide

## Overview

`req` is a git-native requirement traceability tool for safety-critical systems engineering. It provides bidirectional traceability between High-Level Requirements (HLR), Low-Level Requirements (LLR), and source code via inline tags.

## Core Concepts

### Requirement Types
- **HLR** (High-Level Requirement): Customer/system-level requirements
- **LLR** (Low-Level Requirement): Detailed technical requirements derived from HLRs
- **TST** (Test Requirement): Test specifications

### Traceability Chain
```
HLR → LLR → Code → Test
```

### Key Files
- `requirements/hlr/*.md` - HLR documents
- `requirements/llr/*.md` - LLR documents
- `.req/cache.db` - SQLite cache (regenerable)
- `.req/config.toml` - Project configuration

## Requirement Format

Each requirement is a single Markdown file with YAML frontmatter:

```markdown
---
id: LLR-0001
type: llr
title: "Requirement Title"
status: approved
parent: HLR-0001
---

## Description
Detailed requirement text here.

## Acceptance Criteria
- [ ] Criterion 1
- [ ] Criterion 2
```

## Code Tagging

### Inline Tags
```c
// REQ: LLR-0001
void my_function() { }

// Multiple requirements
// REQ: LLR-0001, LLR-0002

// Test verification
// VERIFIES: LLR-0001
```

### Tag Format
- `REQ: <ID>` - Implementation reference
- `VERIFIES: <ID>` - Test verification reference
- IDs: `HLR-XXXX`, `LLR-XXXX`, `TST-XXXX`

## CLI Commands

### Project Setup
```bash
req init                    # Initialize project structure
req import <file> -f markdown  # Import requirements from file
```

### Requirement Management
```bash
req new hlr "Title"         # Create new HLR
req new llr "Title" --parent HLR-0001  # Create LLR with parent
req list                    # List all requirements
req list --type llr         # List only LLRs
```

### Code Scanning
```bash
req scan                    # Scan default source dirs
req scan --source src/      # Scan specific directory
req scan --clear            # Clear existing refs first
```

### Traceability Analysis
```bash
req coverage                # Show coverage statistics
req gaps                    # Show traceability gaps
req trace LLR-0001          # Show trace tree for requirement
req impact HLR-0001         # Impact analysis
req check                   # Validate all links
req check --strict          # Exit with error on warnings
```

### Export/Import
```bash
req export --format json    # Export as JSON
req export --format ai-context  # AI-ready export
req export --id LLR-0001    # Export specific requirement
req import <file> -f json   # Import from JSON
```

## MCP Server (`req-mcp-server`)

The `req-mcp-server` binary implements the [Model Context Protocol](https://modelcontextprotocol.io), letting AI assistants query the requirements database directly without CLI calls or file exports.

### Starting the server

```bash
# Run from the project root (requires an initialized .req/ directory)
req-mcp-server
```

### MCP Tools

| Tool | Description |
|------|-------------|
| `req_scan` | Scan source code for REQ tags; accepts `{ "clear": bool }` |
| `req_coverage` | Return HLR→LLR, LLR implementation and test coverage percentages |
| `req_gaps` | Return all traceability gaps |
| `req_check` | Validate all links; return errors and warnings |
| `req_trace` | Trace tree for one requirement; accepts `{ "id": "LLR-XXXX" }` |
| `req_impact` | Impact analysis; accepts `{ "id": "LLR-XXXX" }` |
| `req_list` | List requirements; accepts `{ "type": "hlr\|llr\|tst", "status": "..." }` |
| `req_export` | Export all requirements; accepts `{ "format": "json\|ai-context" }` |

### MCP Resources

| URI | Content |
|-----|---------|
| `req://coverage` | Coverage statistics (JSON) |
| `req://gaps` | Traceability gaps (JSON) |
| `req://requirements/{type}` | All requirements of a type (JSON array) |
| `req://requirements/{type}/{id}` | Single requirement (JSON object) |

### Integration (Claude Desktop example)

```json
{
  "mcpServers": {
    "req": {
      "command": "req-mcp-server",
      "cwd": "/path/to/your/project"
    }
  }
}
```

---

## AI Integration Patterns

### Forward Engineering (Requirements → Code)

1. Export requirement context:
```bash
req export --format ai-context --id LLR-0001
```

2. The output contains structured data for LLM:
```json
{
  "id": "LLR-0001",
  "title": "ADC Sampling",
  "text": "The system shall sample at 10kHz...",
  "parent": "HLR-0001",
  "code_refs": [],
  "implemented": false
}
```

3. Generate code with proper tags:
```c
// REQ: LLR-0001
void adc_sample(void) {
    // Implementation
}
```

### Reverse Engineering (Code → Requirements)

1. Scan existing code:
```bash
req scan --source src/
```

2. Check for orphan code (code without requirements):
```bash
req gaps
```

3. Create LLRs for untraced code:
```bash
req new llr "Function description" --parent HLR-XXXX
```

4. Add REQ tags to code

### Gap Analysis Workflow

```bash
# Full analysis
req scan && req gaps && req coverage

# Check for issues
req check --strict
```

## Common Workflows

### Starting a New Feature
```bash
# 1. Create HLR if needed
req new hlr "Feature description"

# 2. Create LLR derived from HLR
req new llr "Technical detail" --parent HLR-XXXX

# 3. Implement with tags
# Add // REQ: LLR-XXXX to code

# 4. Verify traceability
req scan && req coverage
```

### Pre-Commit Validation
```bash
req check --strict
# Exit code 0 = pass, 1 = fail
```

### Audit Report Generation
```bash
req export --format ai-context --output audit.json
req coverage
req gaps
```

## Best Practices

### Requirement IDs
- Never reuse IDs
- Never change IDs
- Use sequential numbering (0001, 0002, ...)
- Delete by setting status: deprecated

### Parent Links
- Every LLR must have exactly one parent HLR
- Use `links` table for additional relationships
- Validate with `req check`

### Code Tags
- Place tag immediately before implementation
- One tag per logical unit
- Include context in comment if helpful

### Status Values
- `draft` - Work in progress
- `approved` - Reviewed and accepted
- `deprecated` - No longer valid (don't delete)
- `rejected` - Reviewed and rejected

## Error Handling

| Error | Cause | Solution |
|-------|-------|----------|
| `NotInitialized` | No .req directory | Run `req init` |
| `RequirementNotFound` | ID doesn't exist | Import or create requirement |
| `ParentNotFound` | Parent ID invalid | Check parent exists |
| `InvalidIdFormat` | Bad ID format | Use HLR-XXXX or LLR-XXXX |

## Exit Codes

- `0` - Success
- `1` - Error or validation failure (with --strict)

## File Structure Reference

```
project/
├── .req/
│   ├── cache.db           # SQLite cache (gitignored)
│   └── config.toml        # Configuration
├── requirements/
│   ├── hlr/
│   │   └── HLR-0001.md
│   ├── llr/
│   │   └── LLR-0001.md
│   └── tst/
│       └── TST-0001.md
└── src/
    └── code.c             # With // REQ: tags
```

## Quick Reference Card

| Task | Command |
|------|---------|
| Initialize | `req init` |
| Create HLR | `req new hlr "title"` |
| Create LLR | `req new llr "title" -p HLR-XXXX` |
| Scan code | `req scan` |
| Check coverage | `req coverage` |
| Find gaps | `req gaps` |
| Validate | `req check --strict` |
| Export for AI | `req export -f ai-context` |
| Trace requirement | `req trace LLR-XXXX` |
| Impact analysis | `req impact HLR-XXXX` |
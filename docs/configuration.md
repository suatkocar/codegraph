# Configuration Guide

CodeGraph supports flexible configuration through YAML files, environment variables, and CLI flags. Multiple configuration sources are merged with a well-defined priority order.

## Quick Start

### Using a preset

The fastest way to get started:

```bash
# Set preset via environment variable
export CODEGRAPH_PRESET=balanced
codegraph serve
```

### Creating a config file

For persistent configuration, create `~/.config/codegraph/config.yaml`:

```yaml
version: "1.0"
preset: balanced

tools:
  categories:
    Security:
      enabled: true
    Git:
      enabled: true
```

## Priority Order

Configuration sources are loaded and merged with the following priority (highest wins):

1. **CLI flags** — Highest priority, overrides everything
2. **Environment variables** (`CODEGRAPH_*`) — Temporary overrides
3. **Project config** (`.codegraph.yaml` in project root) — Per-project settings
4. **User config** (`~/.config/codegraph/config.yaml`) — Personal preferences
5. **Built-in defaults** — Full preset, all tools enabled

Higher-priority sources override lower-priority ones. Categories and tool overrides merge by name.

### Config file locations

| Platform | User Config Path |
|----------|-----------------|
| Linux | `~/.config/codegraph/config.yaml` |
| macOS | `~/.config/codegraph/config.yaml` |
| Windows | `%APPDATA%\codegraph\config.yaml` |

Project-level config is `.codegraph.yaml` in the project root directory.

## Configuration File Format

```yaml
version: "1.0"

# Preset: minimal, balanced, full, or security-focused
preset: balanced

# Tool configuration
tools:
  # Category-level toggles
  categories:
    Repository:
      enabled: true
    Search:
      enabled: true
    CallGraph:
      enabled: true
    Analysis:
      enabled: true
    Security:
      enabled: false
    Git:
      enabled: false
    Context:
      enabled: true

  # Individual tool overrides
  overrides:
    codegraph_dead_code:
      enabled: false
      reason: "Too slow for interactive use on this repo"
    codegraph_scan_security:
      enabled: true
      reason: "Required for compliance"

# Performance tuning
performance:
  max_tool_count: 30
  exclude_tests: false
```

## Presets

### minimal

Essential tools only. Best for lightweight editors (Zed, Vim) or when context window budget is tight.

| Property | Value |
|----------|-------|
| **Categories** | Repository, Search |
| **Tool count** | ~15 |
| **Token cost** | ~3,000 tokens |

```yaml
preset: minimal
```

### balanced

Good defaults for daily development. Adds call graphs and context assembly on top of minimal.

| Property | Value |
|----------|-------|
| **Categories** | Repository, Search, CallGraph, Context |
| **Tool count** | ~30 |
| **Token cost** | ~6,000 tokens |

```yaml
preset: balanced
```

### full

All tools enabled. Best for Claude Desktop and Claude Code where context windows are large.

| Property | Value |
|----------|-------|
| **Categories** | All 7 categories |
| **Tool count** | ~50 |
| **Token cost** | ~10,000 tokens |

```yaml
preset: full
```

### security-focused

Prioritizes security scanning and analysis. Drops Git and Context categories to keep focus.

| Property | Value |
|----------|-------|
| **Categories** | Repository, Search, Analysis, Security |
| **Tool count** | ~25 |
| **Token cost** | ~5,000 tokens |

```yaml
preset: security-focused
```

## Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `CODEGRAPH_PRESET` | Override active preset | `minimal`, `balanced`, `full`, `security-focused` |
| `CODEGRAPH_DISABLED_TOOLS` | Comma-separated tool names to disable | `codegraph_dead_code,codegraph_diagram` |
| `CODEGRAPH_DISABLED_CATEGORIES` | Comma-separated categories to disable | `Security,Git` |

Environment variables take priority over config files but are overridden by CLI flags.

```bash
# Disable security tools for a session
CODEGRAPH_DISABLED_CATEGORIES=Security codegraph serve

# Use minimal preset with a specific tool re-enabled
CODEGRAPH_PRESET=minimal codegraph serve
```

## Tool Categories

CodeGraph organizes its MCP tools into 7 categories:

| Category | Tools | Description |
|----------|-------|-------------|
| **Repository** | structure, languages, frameworks | Project overview and metadata |
| **Search** | query, node | Hybrid search and symbol lookup |
| **CallGraph** | callers, callees, dependencies, impact | Graph traversal and blast radius |
| **Analysis** | dead_code, diagram | Code quality analysis |
| **Security** | scan_security, check_owasp, check_cwe, find_secrets, etc. | Vulnerability scanning |
| **Git** | blame, file_history, diff, contributors, hotspots, etc. | Git integration |
| **Context** | context, tests | LLM context assembly |

### Disabling a category

```yaml
tools:
  categories:
    Security:
      enabled: false
    Git:
      enabled: false
```

### Overriding individual tools

Even when a category is enabled, individual tools can be disabled:

```yaml
tools:
  categories:
    CallGraph:
      enabled: true
  overrides:
    codegraph_diagram:
      enabled: false
      reason: "Mermaid diagrams not useful for this project"
```

## Performance Tuning

```yaml
performance:
  # Maximum tools to expose (drops lowest-priority first)
  max_tool_count: 30

  # Exclude test files from indexing
  exclude_tests: true
```

### max_tool_count

Caps the number of tools exposed to the MCP client. When the limit is reached, tools are dropped in reverse priority order (security and analysis tools are dropped before core search and structure tools).

### exclude_tests

When `true`, test files are skipped during indexing. This reduces index size and speeds up indexing for large projects. Test files are identified by path patterns (`tests/`, `__tests__/`, `_test.go`, `.test.ts`, `.spec.js`, etc.).

## Editor Auto-Detection

When no preset is specified, CodeGraph can auto-detect your editor and select an appropriate preset. Detection is based on the MCP client connecting to the server:

| Editor | Default Preset |
|--------|---------------|
| Claude Code | full |
| Claude Desktop | full |
| Cursor | balanced |
| VS Code | balanced |
| Other | full |

## Example Configurations

### Minimal for Vim/Zed

```yaml
version: "1.0"
preset: minimal
performance:
  max_tool_count: 15
```

### Full-featured for Claude Code

```yaml
version: "1.0"
preset: full
tools:
  categories:
    Security:
      enabled: true
    Git:
      enabled: true
```

### Security audit

```yaml
version: "1.0"
preset: security-focused
tools:
  overrides:
    codegraph_scan_security:
      enabled: true
    codegraph_check_owasp:
      enabled: true
    codegraph_find_secrets:
      enabled: true
performance:
  exclude_tests: true
```

### Large monorepo

```yaml
version: "1.0"
preset: balanced
performance:
  max_tool_count: 25
  exclude_tests: true
tools:
  overrides:
    codegraph_diagram:
      enabled: false
      reason: "Too many nodes for useful diagrams"
```

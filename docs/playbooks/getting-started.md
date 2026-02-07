# Getting Started

Get CodeGraph running in under 2 minutes.

## 1. Install

Choose one:

```bash
# macOS / Linux (recommended)
curl -fsSL https://raw.githubusercontent.com/nickolay-grechkin/codegraph/main/install.sh | bash

# macOS (Homebrew)
brew install nickolay-grechkin/codegraph/codegraph

# Windows (PowerShell)
irm https://raw.githubusercontent.com/nickolay-grechkin/codegraph/main/install.ps1 | iex

# From source (all platforms)
cargo install --git https://github.com/nickolay-grechkin/codegraph
```

Verify:
```bash
codegraph --version
# codegraph 0.2.0
```

## 2. Initialize Your Project

```bash
cd /path/to/your/project
codegraph init .
```

This single command:
1. **Indexes your codebase** -- Parses every source file across 32 languages
2. **Registers the MCP server** -- Writes `.mcp.json` for auto-discovery
3. **Installs hooks** -- Keeps the graph in sync as you work
4. **Generates CLAUDE.md** -- Teaches your AI agent to prefer CodeGraph tools
5. **Sets up git hooks** -- Re-indexes on every commit

For CI or scripting, use non-interactive mode:
```bash
codegraph init . --yes
```

## 3. Open Your AI Editor

Open Claude Code (or any MCP-compatible editor) in your project. CodeGraph is already registered. The session start hook triggers an incremental re-index automatically.

## 4. Try Your First Queries

Ask your AI assistant natural questions about your code:

```
"What is the structure of this project?"
```
The agent calls `codegraph_structure` and gets a PageRank-ranked overview of your most important symbols, files, and frameworks.

```
"Find all functions related to authentication"
```
The agent calls `codegraph_query` with hybrid search (keyword + semantic) and returns ranked results.

```
"What calls the handleLogin function?"
```
The agent calls `codegraph_callers` and traces the reverse call graph.

```
"What breaks if I change src/auth/middleware.ts?"
```
The agent calls `codegraph_impact` and shows direct dependents, transitive dependents, affected files, and risk level.

```
"Show me potentially unused code"
```
The agent calls `codegraph_dead_code` and lists symbols with zero incoming edges.

## 5. Understand What Happens Automatically

### On every prompt you send
The UserPromptSubmit hook searches the graph for context relevant to your message and injects it. The agent sees the right code before it starts thinking.

### Before every tool call
The PreToolUse hook injects codebase context before Edit, Write, Read, Grep, Glob, and Bash calls. The agent knows which symbols are in a file before opening it.

### On every file edit
The PostToolUse hook re-indexes the modified file instantly. The graph stays in sync.

### When subagents spawn
The SubagentStart hook gives every subagent a compact project overview from turn one.

## 6. CLI Commands

You can also use CodeGraph directly from the terminal:

```bash
# Search for code
codegraph query "database connection"

# Impact analysis
codegraph impact src/auth/middleware.ts

# Show project statistics
codegraph stats

# Find unused symbols
codegraph dead-code

# Detect frameworks
codegraph frameworks .

# Language breakdown
codegraph languages
```

## 7. Configuration (Optional)

CodeGraph works out of the box with the `full` preset. To customize:

```bash
# Use a different preset
export CODEGRAPH_PRESET=balanced

# Or create a config file
mkdir -p ~/.config/codegraph
```

Create `~/.config/codegraph/config.yaml`:
```yaml
version: "1.0"
preset: balanced
tools:
  categories:
    Security:
      enabled: true
```

See [configuration.md](../configuration.md) for the full reference.

## Next Steps

- [Understand a Codebase](understand-codebase.md) -- Explore an unfamiliar project
- [Fix a Bug](fix-a-bug.md) -- Debug with call graphs and impact analysis
- [Security Audit](security-audit.md) -- Scan for vulnerabilities

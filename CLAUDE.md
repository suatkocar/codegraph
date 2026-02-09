# CodeGraph — Codebase Intelligence MCP Server (Rust) v0.3.0

## What This Is
High-performance Rust codebase intelligence engine. Builds a semantic code graph from source code using native tree-sitter (32 languages), stores symbols and relationships in SQLite with FTS5 and sqlite-vec, generates 768-dim code-specific embeddings via fastembed (Jina v2 Base Code), and exposes 46 MCP tools. Features cross-file import resolution, qualified names (`Class.method`), framework-specific route resolution, git integration, security scanning (OWASP/CWE), data flow analysis, query expansion, progressive disclosure, adaptive context budgets, multi-agent config generation, and a built-in evaluation framework.

## Architecture
- **src/main.rs** — CLI entry point (clap derive, 16 commands, interactive installer)
- **src/mcp/server.rs** — MCP server with 46 tools (rmcp stdio transport)
- **src/mcp/tools_*.rs** — Tool handler modules (core, git, security, analysis, dataflow)
- **src/db/schema.rs** — SQLite schema (nodes, edges, file_hashes, embedding_cache, unresolved_refs, FTS5, vec)
- **src/indexer/** — Native tree-sitter parsing (32 langs), parallel extraction (rayon), fastembed embeddings, qualified name population
- **src/graph/** — Graph store, traversal (recursive CTEs + BFS), ranking (PageRank), hybrid search (FTS5 + vector + RRF + query expansion), complexity analysis, data flow
- **src/context/** — Token-budgeted context assembly for LLM prompts (4-tier adaptive: 40/25/20/15 with redistribution)
- **src/hooks/** — Claude Code hooks, git hooks, CLAUDE.md template, Codex config generation
- **src/resolution/** — Cross-file import resolution, path alias support, framework-specific route resolvers, framework detection (18+), dead code analysis
- **src/git/** — Git integration (blame, history, hotspots, contributors) via `std::process::Command`
- **src/security/** — YAML-based security rules engine, OWASP/CWE scanning, taint analysis (source→sink)
- **src/config/** — YAML configuration, 4 presets (minimal/balanced/full/security), auto editor detection
- **src/observability/** — Structured logging (tracing), path validation, secret redaction, metrics
- **src/eval/** — Evaluation harness (precision/recall/F1), token reduction benchmarks
- **src/cli/** — Interactive installer with ASCII banner, progress bars, confirmations

## Key Commands
- `cargo build --release` — Build optimized binary
- `cargo test` — Run all tests (~2330+)
- `./target/release/codegraph init <dir>` — Interactive setup (index + hooks + MCP + git hooks + CLAUDE.md + auto-allow + Codex config)
- `./target/release/codegraph init <dir> --yes` — Non-interactive setup (CI/scripting)
- `./target/release/codegraph index <dir>` — Index a codebase
- `./target/release/codegraph serve` — Start MCP server (stdio)
- `./target/release/codegraph serve --http 0.0.0.0:8080` — Start MCP server (HTTP)
- `./target/release/codegraph query <text>` — CLI search
- `./target/release/codegraph stats` — Show index statistics (includes unresolved refs)
- `./target/release/codegraph impact <symbol>` — Blast radius analysis
- `./target/release/codegraph dead-code` — Find unused symbols
- `./target/release/codegraph frameworks <dir>` — Detect frameworks
- `./target/release/codegraph languages` — Language breakdown
- `./target/release/codegraph git-hooks install|uninstall` — Git hook management

## Supported Languages (32)
TypeScript, TSX, JavaScript, JSX, Python, Go, Rust, Java, C, C++, C#, PHP, Ruby, Swift, Kotlin, Bash, Scala, Dart, Zig, Lua, Verilog/SystemVerilog, Haskell, Elixir, Groovy, PowerShell, Clojure, Julia, R, Erlang, Elm, Fortran, Nix

## Performance
- 68% average token reduction vs reading all files (measured via evaluation framework)
- 20x faster indexing than TypeScript version (230ms vs ~5s for 54 files, without embeddings)
- Incremental no-op: 13ms
- 606%+ CPU utilization via rayon parallel parsing
- Caller detection: 100% precision, 100% recall
- Dead code detection: 75% precision, 100% recall

### Ground-Truth Evaluation (v0.3.0, self-indexed: 83 files, 7383 nodes)
- **Search** (15 queries): Precision 0.233, Recall 0.463, F1 0.294, MRR 0.631
- **Callers** (8 callees): Precision 0.392, Recall 0.688, F1 0.407
- **Overall applicable**: Precision 0.313, Recall 0.576, F1 0.351
- Ground-truth data: `eval/ground-truth/codegraph.json`
- Results: `eval/results/codegraph-v030.json`

## MCP Tools (46)

### Core (14)
1. `codegraph_query` — Hybrid keyword + semantic search with query expansion
2. `codegraph_search` — Fast FTS5-only keyword search (<10ms)
3. `codegraph_dependencies` — Forward dependency traversal
4. `codegraph_callers` — Reverse call graph traversal (with detail_level)
5. `codegraph_callees` — Forward call graph (with detail_level)
6. `codegraph_impact` — Blast radius analysis
7. `codegraph_structure` — Project overview with PageRank
8. `codegraph_tests` — Test coverage discovery
9. `codegraph_context` — LLM context assembly (68% fewer tokens, adaptive budget)
10. `codegraph_node` — Direct symbol lookup with relationships (with detail_level)
11. `codegraph_diagram` — Mermaid diagram generation
12. `codegraph_dead_code` — Find unused symbols
13. `codegraph_frameworks` — Detect project frameworks
14. `codegraph_languages` — Language breakdown statistics

### Git Integration (9)
15. `codegraph_blame` — Line-by-line blame
16. `codegraph_file_history` — File commit history
17. `codegraph_recent_changes` — Recent repository commits
18. `codegraph_commit_diff` — Commit diff details
19. `codegraph_symbol_history` — Symbol modification history
20. `codegraph_branch_info` — Branch status and tracking
21. `codegraph_modified_files` — Working tree changes
22. `codegraph_hotspots` — Churn-based hotspot detection
23. `codegraph_contributors` — Contributor statistics

### Security (9)
24. `codegraph_scan_security` — YAML rule-based vulnerability scan
25. `codegraph_check_owasp` — OWASP Top 10 2021 scan
26. `codegraph_check_cwe` — CWE Top 25 scan
27. `codegraph_explain_vulnerability` — CWE explanation + remediation
28. `codegraph_suggest_fix` — Fix suggestion for findings
29. `codegraph_find_injections` — SQL/XSS/command injection via taint analysis
30. `codegraph_taint_sources` — Identify taint sources
31. `codegraph_security_summary` — Comprehensive risk assessment
32. `codegraph_trace_taint` — Data flow tracing from source

### Repository & Analysis (7)
33. `codegraph_stats` — Index statistics
34. `codegraph_circular_imports` — Cycle detection (Tarjan SCC)
35. `codegraph_project_tree` — Directory tree with symbol counts
36. `codegraph_find_references` — Cross-reference search
37. `codegraph_export_map` — Module export listing
38. `codegraph_import_graph` — Import graph visualization
39. `codegraph_file` — File symbol listing

### Call Graph & Data Flow (6)
40. `codegraph_find_path` — Shortest call path (BFS)
41. `codegraph_complexity` — Cyclomatic + cognitive complexity
42. `codegraph_data_flow` — Variable def-use chains
43. `codegraph_dead_stores` — Assignments never read
44. `codegraph_find_uninitialized` — Variables used before init
45. `codegraph_reaching_defs` — Reaching definition analysis

### Deep Search (1)
46. `codegraph_deep_query` — Cross-encoder re-ranked search (highest precision)

## Claude Code Hooks (10)
- **SessionStart** — Incremental re-index on session open
- **UserPromptSubmit** — Inject graph-aware context into prompts
- **PreToolUse** — Inject codebase context before tool execution (Edit/Write/Read/Grep/Glob/Bash)
- **PostToolUse** — Re-index modified file after Write/Edit
- **PostToolUseFailure** — Provide corrective context when tools fail
- **SubagentStart** — Inject project overview + tool guidance into subagents
- **PreCompact** — Save PageRank summary before compaction
- **Stop** — Quality check before agent stops (unresolved ref ratio)
- **TaskCompleted** — Quality gate: dead code + unresolved ref report
- **SessionEnd** — Final re-index and session diagnostics

## Security Rules
- 4 bundled YAML rule files: `rules/owasp-top10.yaml`, `rules/cwe-top25.yaml`, `rules/crypto.yaml`, `rules/secrets.yaml`
- Custom rules via YAML with regex patterns, severity, CWE/OWASP mappings
- Taint analysis: source→sink tracking for injection vulnerabilities

## Configuration
- YAML config: `~/.config/codegraph/config.yaml` or `.codegraph.yaml`
- 4 presets: minimal (15 tools), balanced (30 tools), full (all), security-focused
- Auto editor detection: Claude Code → full, VS Code → balanced, Zed → minimal
- Environment overrides: `CODEGRAPH_PRESET`, `CODEGRAPH_DISABLED_TOOLS`

## Multi-Agent Support
- Claude Code: `.mcp.json` + auto-allow permissions + global `~/.claude/CLAUDE.md`
- OpenAI Codex CLI: `~/.codex/config.toml` + `AGENTS.md`
- MCP Resources: `codegraph://status`, `codegraph://overview`

## v0.3.0 New Features
- **Query Expansion**: CamelCase/snake_case splitting, 57 abbreviation mappings, 21 synonym groups
- **CamelCase FTS5 Tokenization**: `processUserInput` now searchable as "process", "user", "input"
- **Progressive Disclosure**: `detail_level` parameter (summary/standard/full) on key tools
- **Adaptive Token Budget**: Dynamic redistribution of unused budget across 4 tiers (default 32K)
- **Fast Keyword Search**: `codegraph_search` — FTS5-only, <10ms, for exact name lookups
- **Auto-Allow Permissions**: 46 MCP tool permissions auto-registered in `~/.claude/settings.json`
- **Global Discovery**: Marker-based idempotent section in `~/.claude/CLAUDE.md`
- **SubagentStart Guidance**: Tool tiers and anti-patterns injected into subagent context
- **RRF Top-Rank Bonus**: +0.05 for rank-1, +0.02 for rank 2-3 in each result list
- **BM25 Column Weights**: name(10), qualified_name(8), signature(5), doc_comment(3), file_path(1)
- **Codex Config**: Auto-generates `config.toml` and `AGENTS.md` for Codex CLI
- **MCP Resources**: `list_resources`/`read_resource` for codegraph://status and codegraph://overview
- **Project Context Metadata**: Directory-level annotations in `.codegraph.yaml`

## Conventions
- Sync core, async only at MCP boundary (rmcp + tokio)
- `prepare_cached` for all SQL queries
- Feature-gated embeddings (`fastembed` behind `embedding` feature)
- Embedding model: jina-embeddings-v2-base-code (768-dim, code-specific)
- tree-sitter 0.25 with 32 statically linked grammars
- Structured logging via `tracing` crate (RUST_LOG support)
- Cross-file import resolution for relative imports (./ ../) and path aliases (@/ ~/)
- Framework-specific route resolution (React, Express, Django, Rails, Laravel, Spring Boot)
- Qualified names: `ClassName.methodName` for methods/properties via line-range containment
- Path traversal protection on all MCP tool inputs
- Secret redaction in tool outputs
- All hooks use `panic::catch_unwind()` — never block Claude Code
- **Prefer CodeGraph MCP tools over grep/glob** for finding code. They understand your project's structure, dependencies, and call graphs — not just text matches.

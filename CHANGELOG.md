# Changelog

All notable changes to CodeGraph are documented here.

## [0.2.0] - 2026-02-07

### Language Support
- Added 17 new languages: Bash, Scala, Haskell, Julia, Lua, Elixir, Clojure, Dart, Fortran, Groovy, Zig, R, Verilog, Erlang, Elm, PowerShell, Nix
- Total language support now at **32 languages** (15 original + 17 new)
- All grammars statically linked via native tree-sitter 0.25

### Git Integration
- `codegraph_blame` — Git blame for any file/line range
- `codegraph_file_history` — Commit history for a file
- `codegraph_diff` — Diff between commits or working tree
- `codegraph_contributors` — Contributor statistics
- `codegraph_hotspots` — Files with high churn and complexity
- `codegraph_recent_changes` — Recently modified files
- `codegraph_commit_graph` — Commit relationship visualization
- `codegraph_stale_branches` — Branches that haven't been updated
- `codegraph_symbol_history` — Git history for a specific symbol

### Security Analysis
- Pattern-based security scanner with YAML rule engine
- 4 bundled rulesets: OWASP Top 10 2021, CWE Top 25, crypto weaknesses, secret detection
- 65+ security rules covering injection, XSS, path traversal, deserialization, weak crypto, hardcoded secrets
- `codegraph_scan_security` — Full security scan with severity filtering
- `codegraph_check_owasp` — OWASP Top 10 compliance check
- `codegraph_check_cwe` — CWE Top 25 compliance check
- `codegraph_find_secrets` — Detect hardcoded API keys, passwords, tokens
- `codegraph_taint_sources` — Identify user input entry points
- `codegraph_crypto_audit` — Audit cryptographic usage
- `codegraph_security_summary` — Aggregate security posture overview
- Custom rule support: load additional YAML rule files from disk

### Configuration System
- YAML-based configuration with multi-source merging
- 4 presets: minimal (~15 tools), balanced (~30 tools), full (~50 tools), security-focused (~25 tools)
- Priority order: CLI flags > environment variables > project config > user config > defaults
- Per-tool and per-category enable/disable with reason annotations
- Environment variables: `CODEGRAPH_PRESET`, `CODEGRAPH_DISABLED_TOOLS`, `CODEGRAPH_DISABLED_CATEGORIES`
- Editor auto-detection for optimal preset selection
- Performance tuning: `max_tool_count`, `exclude_tests`

### Call Graph Improvements
- Enhanced forward and reverse call graph traversal
- Transitive caller/callee support with configurable depth
- Complexity metrics (cyclomatic, cognitive)
- Data flow tracking between functions

### New Hooks
- `PostToolUseFailure` — Corrective context when tools fail
- `SubagentStart` — Project overview injection for subagents
- `PreCompact` — PageRank summary preservation before compaction
- `Stop` — Quality gate before agent stops
- `TaskCompleted` — Dead code and unresolved ref report
- `SessionEnd` — Final re-index and session diagnostics

### Distribution
- Homebrew formula for macOS and Linux
- PowerShell installer for Windows
- MCP configuration templates for Claude Code, Claude Desktop, Cursor, VS Code
- Comprehensive documentation and playbooks

### Internal
- Structured logging via `tracing` with env-filter support
- File watcher for real-time re-indexing (`notify` + `notify-debouncer-full`)
- `serde_yaml` for config and security rule parsing
- `regex` crate for security pattern matching
- `directories` crate for platform-specific config paths
- `chrono` for timestamp handling

## [0.1.3] - 2026-02-05

### Fixed
- Minor bug fixes and stability improvements

## [0.1.0] - 2026-01-28

### Initial Release
- 15 language support (TypeScript, TSX, JavaScript, JSX, Python, Go, Rust, Java, C, C++, C#, PHP, Ruby, Swift, Kotlin)
- 13 MCP tools (query, dependencies, callers, callees, impact, structure, tests, context, node, diagram, dead_code, frameworks, languages)
- Hybrid search: FTS5 BM25 + sqlite-vec cosine similarity + Reciprocal Rank Fusion
- Jina v2 Base Code embeddings (768-dim, ONNX)
- 4-tier token-budgeted context assembly (Core 40%, Near 25%, Extended 20%, Background 15%)
- Cross-file import resolution with path alias support (@/, ~/)
- Framework-specific resolvers (React, Express, Django, Rails, Laravel, Spring Boot)
- Qualified names via line-range containment (`ClassName.methodName`)
- PageRank-based symbol importance ranking
- Dead code analysis
- Framework detection (18+ frameworks)
- Claude Code hooks (SessionStart, UserPromptSubmit, PreToolUse, PostToolUse)
- Git post-commit hook for automatic re-indexing
- Interactive installer with ASCII banner, progress bars, confirmations
- Non-interactive mode with `--yes` flag
- Evaluation framework with precision/recall/F1 metrics
- 314 tests passing
- CI/CD with 4-platform binary releases (macOS ARM64, macOS x86_64, Linux x86_64, Windows x86_64)

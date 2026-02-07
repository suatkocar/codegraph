# Performance

CodeGraph is designed for sub-second indexing and minimal overhead during AI-assisted development sessions. This document covers measured benchmarks, architectural decisions that drive performance, and tuning guidance.

## Indexing Speed

| Metric | Value | Notes |
|--------|-------|-------|
| 30 files, from scratch | **127 ms** | Including parsing, extraction, FTS5 indexing |
| 54 files, from scratch | **230 ms** | Without embeddings |
| Incremental (no changes) | **12 ms** | SHA-256 hash check per file |
| CPU utilization | **600%+** | Parallel parsing via rayon |

Incremental indexing skips unchanged files entirely by comparing SHA-256 content hashes. On a typical session start where no files have changed, re-indexing completes in ~12ms.

### Parallel Architecture

CodeGraph uses rayon for CPU-bound work:
1. Files are discovered via the `ignore` crate (respects `.gitignore`)
2. Each file is hashed with SHA-256 for change detection
3. Changed files are parsed in parallel across all CPU cores
4. tree-sitter `Parser` instances are created per-task (not Send/Sync)
5. Results are written to SQLite sequentially (rusqlite `Connection` is not Sync)

This architecture achieves near-linear scaling with core count for the parsing phase.

### Embedding Generation

First-time indexing with embeddings enabled takes longer due to ONNX model loading and inference:

| Phase | Time | Notes |
|-------|------|-------|
| Model load (cold) | ~2-3s | One-time, cached for subsequent runs |
| Embed 54 files | ~5s | Jina v2 Base Code, 768-dim |
| Incremental embed | 0 ms | Unchanged files skipped |

Embeddings are feature-gated. Build with `--no-default-features` for a keyword-only binary that skips embedding entirely.

## Token Reduction

Measured on a real 11-file TypeScript project with ground-truth evaluation:

| Task Query | Baseline Tokens | CodeGraph Tokens | Reduction |
|------------|-----------------|------------------|-----------|
| Authentication login | 4,962 | 1,997 | **59.8%** |
| Database connection | 4,962 | 1,305 | **73.7%** |
| User repository | 4,962 | 2,108 | **57.5%** |
| API routes handlers | 4,962 | 935 | **81.2%** |
| Password hashing | 4,962 | 1,522 | **69.3%** |
| **Average** | | | **68.3%** |

Without CodeGraph, an agent reads all files to understand the codebase. With CodeGraph, the agent receives only the relevant symbols and their relationships, reducing context window usage by 68% on average.

### How Token Reduction Works

The context assembler uses a 4-tier budget:

| Tier | Budget % | Contents |
|------|----------|----------|
| **Core** | 40% | Full source code of top-ranked results |
| **Near** | 25% | Signatures of direct callers and callees |
| **Extended** | 20% | Related tests and sibling declarations |
| **Background** | 15% | Project file listing for structural orientation |

This means the agent gets the most important code in full, surrounding context as signatures, and a project map for navigation. The total fits within configurable token budgets.

## Quality Metrics

Measured with the built-in evaluation framework against ground-truth fixtures:

| Category | Precision | Recall | F1 Score |
|----------|-----------|--------|----------|
| Caller detection | 1.00 | 1.00 | **1.00** |
| Dead code detection | 0.75 | 1.00 | **0.86** |
| Search relevance (FTS only) | 0.27 | 0.58 | **0.37** |

### Caller Detection (Perfect)

CodeGraph achieves 100% precision and recall on caller detection. It never misses a caller and never reports a false caller. This is critical for impact analysis -- when you need to know what breaks if you change a function.

### Dead Code Detection (High Recall)

Dead code analysis catches 100% of truly unused symbols (recall = 1.0). Precision is 0.75 because some symbols that appear unused are actually invoked through dynamic dispatch, reflection, or framework conventions. The analyzer excludes exported symbols, main functions, and test functions to reduce false positives.

### Search Relevance

Keyword-only search (FTS5 BM25) scores lower on precision because symbol names don't always match natural language queries. Hybrid search with semantic embeddings (Jina v2 Base Code) scores significantly higher by understanding code semantics, but we report the FTS-only number as the baseline.

## Binary Size

| Build | Size | Features |
|-------|------|----------|
| With embeddings | ~45 MB | Full hybrid search (FTS5 + vector + RRF) |
| Without embeddings | ~29 MB | Keyword search only (FTS5 BM25) |

The binary is fully self-contained: 32 tree-sitter grammars, SQLite with FTS5 and sqlite-vec, and optionally the Jina v2 Base Code ONNX model are all statically linked. No runtime dependencies, no downloads, no initialization delay.

## Memory Usage

CodeGraph stores its index in a SQLite database on disk (`.codegraph/` directory). Runtime memory usage is proportional to the number of files being actively parsed:

- **Idle:** ~10-20 MB (MCP server waiting for requests)
- **Indexing:** ~50-200 MB peak (depends on file count and parallelism)
- **Query:** ~15-30 MB (database reads + result formatting)

The SQLite database grows roughly linearly with codebase size. A 1000-file project typically produces a 5-15 MB database.

## Configuration Presets and Performance

Presets control how many tools are exposed to the AI client. Fewer tools means less token overhead in the system prompt:

| Preset | Tools | Estimated Tokens | Best For |
|--------|-------|------------------|----------|
| minimal | ~15 | ~3,000 | Zed, Vim, quick edits |
| balanced | ~30 | ~6,000 | VS Code, Cursor, JetBrains |
| full | ~50 | ~10,000 | Claude Desktop, Claude Code |
| security-focused | ~25 | ~5,000 | Security auditing |

Use the `minimal` preset when context window budget is tight. Use `full` when you want maximum capabilities.

## Tuning Tips

### For large codebases (>1000 files)

- Use `codegraph index --force` for the first full index
- Subsequent incremental indexes will be fast
- Consider `exclude_tests: true` in your config to skip test files

### For slow machines

- Build without embeddings: `cargo build --release --no-default-features`
- Use the `minimal` preset to reduce tool count
- The index is stored on disk, so startup doesn't require re-indexing

### For CI/CD

- Use `codegraph init <dir> --yes` for non-interactive mode
- The index is deterministic given the same input files
- Git hooks can be disabled if not needed: skip `codegraph git-hooks install`

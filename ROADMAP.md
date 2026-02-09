# CodeGraph Roadmap: From v0.2.5 to v1.0

> Synthesized from competitive analysis of 25+ MCP code intelligence projects, 7 commercial tools (Cursor, Windsurf, Sourcegraph Cody, Continue.dev, GitHub Copilot, Augment Code, Greptile), and research into what every major LLM coding agent needs.

---

## Strategic Position

**Where we stand:**
- 45 MCP tools, 32 languages, 10 hooks, 2157 tests — technically the most comprehensive single-binary code intelligence MCP server
- Hooks are UNIQUE — no competitor offers Claude Code lifecycle hooks
- Our tree-sitter + SQLite + FTS5 + vector architecture is validated by Sourcegraph's move AWAY from embeddings toward BM25 + code graph
- narsil-mcp (92 stars, Hacker News featured) appears to be a derivative of our project

**Where we're losing:**
- qmd has 7,200 stars with 6 tools. claude-context has 5,300 stars with 4 tools. **Simplicity wins adoption.**
- colbymchenry/codegraph got 100 stars in 3 weeks with 7 tools and a one-liner installer
- Our A/B test showed CodeGraph made things WORSE (16 min/$6.85 vs 7 min/$2.17) until v0.2.4 fixes
- We are 100% Claude Code specific — Codex, Gemini CLI, Copilot, Cursor cannot use our hooks
- 46 tools create decision fatigue; Claude's Tool Search defers loading when descriptions exceed 10K tokens

**The gap is not capability. The gap is time-to-first-value and multi-agent support.**

---

## Competitive Landscape Summary

### Direct Competitors (MCP Code Intelligence)

| Project | Stars | Tools | Languages | Stack | Key Differentiator |
|---------|-------|-------|-----------|-------|-------------------|
| **zilliztech/claude-context** | 5,300 | 4 | N/A (chunking) | TS, Zilliz Cloud | Backed by Milvus creators |
| **sourcebot-dev/sourcebot** | 3,100 | MCP | All | TS, Docker | Self-hosted Sourcegraph alternative |
| **vitali87/code-graph-rag** | 1,700 | MCP | Multi | Python, Neo4j | Knowledge graph + Cypher queries |
| **johnhuang316/code-index-mcp** | 754 | 13 | 50+ | Python, tree-sitter | Dual strategy: specialized + universal |
| **CodeGraphContext** | 419 | CLI+MCP | 12 | Python, FalkorDB | Graph DB, live file watching |
| **JudiniLabs/mcp-code-graph** | 376 | 7 | Multi | JS | CodeGPT backing, public+private graphs |
| **wrale/mcp-server-tree-sitter** | 254 | 30+ | Multi | Python | Most comprehensive pure tree-sitter MCP |
| **postrv/narsil-mcp** | 92 | 90 | 32 | Rust | Appears derivative of our CodeGraph |
| **colbymchenry/codegraph** | 100 | 7 | 15 | Node.js | One-liner install, Explore agent targeting |
| **suatkocar/codegraph (us)** | — | 45 | 32 | Rust | Hooks, security, git, data flow |

### Commercial Tools

| Tool | Code Graph? | Embedding Approach | MCP Support |
|------|------------|-------------------|-------------|
| **Cursor** | No | Proprietary, Turbopuffer (100B+ vectors) | Client |
| **Windsurf** | Claims "relational" | Proprietary, local vector store | Client |
| **Sourcegraph Cody** | Yes (SCIP/BFG) | Moving away from embeddings → BM25 + graph | No |
| **Continue.dev** | No | Configurable (Voyage code-3 recommended) | Full host |
| **GitHub Copilot** | No | Proprietary 512-dim, Blackbird search | Client + Server |
| **Augment Code** | Claims cross-repo | Undisclosed | Server |
| **Aider** | No (PageRank repo map) | None | No native MCP |

### Key Insight

> Only Sourcegraph has a true, compiler-accurate code graph. Cursor, Windsurf, Copilot, and Continue all rely on embedding similarity — they can find "semantically similar" code but CANNOT answer "who calls this function." Our 45-tool graph-aware MCP server operates in a space where most tools have zero capability.

---

## Phase 1: Hackathon Ready (v0.3.0) — ✅ COMPLETE

**Goal:** Fix the biggest UX gaps that prevent adoption. Make CodeGraph work out of the box.

- ✅ **1.1 Auto-Allow Permissions** — 45 tool permissions auto-registered in `~/.claude/settings.json`
- ✅ **1.2 Global CLAUDE.md Discovery** — Marker-based idempotent section in `~/.claude/CLAUDE.md`
- ✅ **1.3 SubagentStart Tool Guidance** — Tool tiers + anti-patterns injected into subagent context
- ✅ **1.4 CLAUDE.md Tool Tiers** — 3-tier hierarchy (Start Here → Drill Down → Specialized)
- ✅ **1.5 RRF Top-Rank Bonus** — +0.05 for rank-1, +0.02 for rank 2-3 in each result list
- ✅ **1.6 Fix Cosmetic Indexing Count** — `ignore::WalkBuilder` with `.standard_filters(true)`

---

## Phase 2: Multi-Agent Support (v0.4.0) — 60% COMPLETE

**Goal:** Make CodeGraph work with Codex, Gemini CLI, Copilot, and Cursor — not just Claude Code.

- ✅ **2.1 Multi-Format Config Generation** — Codex `config.toml` + `AGENTS.md` auto-generated
- ✅ **2.2 Enable MCP Resources** — `codegraph://status`, `codegraph://overview`
- ❌ **2.3 Upgrade rmcp** — We're on rmcp 0.1, Codex client expects 0.13+. Wire format changed.
- ❌ **2.4 Tool Schema Audit** — Codex silently excludes tools with unsupported JSON Schema patterns.
- ✅ **2.5 AGENTS.md Generation** — Included in 2.1

---

## Phase 3: Search Quality (v0.5.0) — ✅ COMPLETE

**Goal:** Close the search quality gap with QMD's pipeline and Cursor's approach.

- ✅ **3.1 Rules-Based Query Expansion** — CamelCase/snake_case splitting, 57 abbreviations, 21 synonym groups
- ✅ **3.2 CamelCase/snake_case FTS5 Tokenization** — `name_tokens` column with pre-split identifiers
- ✅ **3.3 Tiered Search Commands** — `codegraph_search` (FTS5-only, <10ms) + `codegraph_query` (hybrid)
- ✅ **3.4 BM25 Column Weights** — name(10), qualified_name(8), signature(5), doc_comment(3), file_path(1)
- ❌ **3.5 Position-Aware Blending** — Prep for re-ranker, low priority without re-ranker

---

## Phase 4: Architecture & Quality (v0.6.0) — 50% COMPLETE

**Goal:** Clean up technical debt, improve code quality, wire up unused features.

- ✅ **4.1 Split server.rs** — 5 modules: tools_core (831L), tools_git (135L), tools_security (167L), tools_analysis (267L), tools_dataflow (125L). server.rs 3522→2258 lines.
- ❌ **4.2 Wire Config Presets to MCP Server** — `list_tools()` override needed (rmcp macro limitation)
- ✅ **4.3 Fix Mutex Poisoning** — 26 sites: `.lock().unwrap()` → `.lock().unwrap_or_else(|e| e.into_inner())`
- ❌ **4.4 Lazy Test Discovery** — Add `is_test` column for indexed test lookup
- ❌ **4.5 Data Flow Tools Accept File Paths** — Accept `file_path` parameter, read internally
- ✅ **4.6 Consolidate Overlapping Tools** — Clarified security tools, callers vs find_references, dependencies vs callees
- ❌ **4.7 Run and Publish Evaluation Metrics** — Ground-truth JSON + precision/recall/F1/MRR numbers

---

## Phase 5: Context Intelligence (v0.7.0) — 70% COMPLETE

**Goal:** Make context assembly state-of-the-art. This is where we differentiate from every competitor.

- ✅ **5.1 Adaptive Token Budget** — Dynamic redistribution of unused budget across 4 tiers
- ✅ **5.2 Increase Default Budget** — 8K → 32K default
- ✅ **5.3 Progressive Disclosure** — `detail_level` parameter (summary/standard/full) on callers/callees/node/context
- ✅ **5.4 Project Context Metadata** — `contexts` section in `.codegraph.yaml`, enriches search results
- ❌ **5.5 Better Token Estimation** — Character-class heuristic (operators, keywords, identifiers separately)
- ❌ **5.6 File-Level Search Mode** — `scope: "file"` parameter on codegraph_search

---

## Phase 6: Advanced Search (v0.8.0) — NOT STARTED

**Goal:** Bring search quality to parity with QMD's hybrid pipeline (minus the LLM latency).

### 6.1 Cross-Encoder Re-ranking (Optional Deep Search)
**Why:** Cross-encoders see query AND document simultaneously. ~30% MRR improvement on retrieval benchmarks.
**What:** New `codegraph_deep_query` tool with:
1. Normal hybrid search (top 20)
2. Cross-encode (query, symbol_body) with ms-marco-MiniLM-L-6-v2 (22M params, ~20ms for 20 docs on CPU)
3. Position-aware blending
4. Return top 10
**Model:** ONNX via fastembed, feature-gated like embeddings.
**File:** New `src/graph/reranker.rs`, `src/mcp/server.rs`
**Effort:** 1 week

### 6.2 Hybrid Embedding (Code + Documentation)
**Why:** Current embeddings combine code and documentation in one vector. Separate vectors could improve precision.
**What:** Generate two embeddings per symbol: one for the code body, one for the doc comment. Search against the most appropriate one based on query type.
**File:** `src/indexer/embedder.rs`, `src/graph/search.rs`
**Effort:** 1 week

### 6.3 Query Intent Detection
**Why:** "getUserById" is a symbol lookup (use FTS). "function that validates email" is a semantic query (use vectors).
**What:** Simple heuristic: if query matches camelCase/snake_case/PascalCase pattern, prioritize FTS. Otherwise, prioritize vector search.
**File:** `src/graph/search.rs`
**Effort:** 1 day

---

## Phase 7: Ecosystem (v0.9.0) — NOT STARTED

**Goal:** Make CodeGraph embeddable and integrable beyond MCP.

### 7.1 Streamable HTTP Transport
**Why:** Gemini CLI, Copilot, and remote deployments need HTTP. We only support stdio.
**What:** Add HTTP server mode using rmcp's HTTP transport. `codegraph serve --http :8080`.
**File:** `src/main.rs`, `src/mcp/server.rs`
**Effort:** 3 days

### 7.2 MCP Prompts
**Why:** MCP prompts are structured message templates the server provides. Underused but valuable for guided workflows.
**What:** Expose prompts like:
- "Review file for security issues" → runs scan_security + check_owasp
- "Explain this function's role" → runs node + callers + callees + context
- "Pre-refactor impact check" → runs impact + callers + dependencies
**File:** `src/mcp/server.rs`
**Effort:** 2 days

### 7.3 MCP Tasks (Async Operations)
**Why:** MCP Nov 2025 spec added Tasks for long-running operations. Perfect for full codebase indexing.
**What:** Expose `codegraph_index` as an async MCP Task that returns a task handle. Clients poll for completion.
**File:** `src/mcp/server.rs`
**Effort:** 3 days

### 7.4 C FFI Layer
**Why:** Enables Node.js bindings (napi-rs), Python bindings (PyO3), and any language with C FFI.
**What:** Expose core functions via `extern "C"`:
```c
codegraph_open(path) -> handle
codegraph_query(handle, text) -> json
codegraph_callers(handle, symbol) -> json
codegraph_close(handle)
```
**File:** New `src/ffi.rs`
**Effort:** 1 week

### 7.5 GitHub Action
**Why:** CI/CD integration. Post security findings and code quality metrics as PR comments.
**What:** Docker-based GitHub Action that runs `codegraph scan-security` and comments on PRs.
**Effort:** 2 days

---

## Phase 8: v1.0 — Polish & Scale

### 8.1 Windows Support
- Fix `PermissionsExt` usage, add Windows paths
- Add Windows target to CI release matrix
- Test install.ps1

### 8.2 Multi-Repo Support
- Cross-repo dependency tracking
- Shared index across related repos
- Monorepo per-package indexing

### 8.3 Web Visualization
- Interactive code graph viewer (Mermaid → D3.js or vis.js)
- Standalone web server mode: `codegraph viz --port 3000`

### 8.4 LLM Re-ranking with Code-Specific Model
- Train or fine-tune a cross-encoder on code retrieval tasks
- Feature-gated like embeddings

### 8.5 Node.js Bindings (via napi-rs)
- First-class npm package with TypeScript types
- Drop-in replacement for colbymchenry/codegraph's library API

---

## Priority Matrix

| Phase | Version | Effort | Impact on Adoption | Impact on Quality | Status |
|-------|---------|--------|-------------------|-------------------|--------|
| 1. Hackathon Ready | v0.3.0 | 2 days | **Very High** | Medium | ✅ COMPLETE |
| 2. Multi-Agent | v0.4.0 | 1 week | **Very High** | Low | 60% done |
| 3. Search Quality | v0.5.0 | 1 week | Medium | **Very High** | ✅ COMPLETE |
| 4. Architecture | v0.6.0 | 1 week | Low | **High** | 50% done |
| 5. Context Intel | v0.7.0 | 1 week | **High** | **High** | 70% done |
| 6. Advanced Search | v0.8.0 | 2 weeks | Low | **Very High** | Not started |
| 7. Ecosystem | v0.9.0 | 2 weeks | **High** | Medium | Not started |
| 8. v1.0 Polish | v1.0 | Ongoing | Medium | Medium | Not started |

---

## The Honest Assessment

### What We're Best At (Unmatched)
1. **Lifecycle hooks** — no competitor has this. 10 hooks that transform CodeGraph from passive query service to active coding partner.
2. **Tool breadth** — 46 tools across 6 categories. Security + git + data flow in one binary.
3. **Language coverage** — 32 languages, all statically linked. No runtime downloads.
4. **Performance** — Rust native, 230ms indexing, 13ms incremental, 600%+ CPU utilization.
5. **Testing rigor** — 2157 tests. Unmatched in this space.

### What We Must Fix (Blocking Adoption)
1. ~~Auto-allow permissions~~ ✅ Fixed in v0.3.0
2. **Multi-agent config** — rmcp upgrade needed for Codex. Gemini, Copilot configs not yet generated.
3. ~~Tool decision fatigue~~ ✅ Fixed with tiers, but presets not wired to MCP yet.
4. ~~Search quality~~ ✅ Fixed with query expansion, CamelCase tokenization, BM25 weights.
5. ~~Discovery~~ ✅ Fixed with global CLAUDE.md.

### The Strategic Bet
> Stars correlate with simplicity, not feature count. qmd has 7,200 stars with 6 tools. But simplicity without depth is a ceiling. Our bet is that depth wins in the long run — IF we fix the adoption curve. The right strategy is to present simplicity (7-tool default preset, one-liner install, auto-everything) while having 46 tools available for power users who need them.

---

## Research Sources

### Competitor Analysis
- [colbymchenry/codegraph](https://github.com/colbymchenry/codegraph) — Node.js, 7 tools, 100 stars
- [tobi/qmd](https://github.com/tobi/qmd) — Hybrid search pipeline, 7,200 stars
- [zilliztech/claude-context](https://github.com/zilliztech/claude-context) — Zilliz-backed, 5,300 stars
- [sourcebot-dev/sourcebot](https://github.com/sourcebot-dev/sourcebot) — Self-hosted code search, 3,100 stars
- [vitali87/code-graph-rag](https://github.com/vitali87/code-graph-rag) — Neo4j knowledge graph, 1,700 stars
- [postrv/narsil-mcp](https://github.com/postrv/narsil-mcp) — 90 tools, 32 langs, appears derivative
- 20+ additional projects analyzed (see landscape report)

### Commercial Tools
- [Cursor Indexing Architecture](https://turbopuffer.com/customers/cursor) — Turbopuffer, 100B+ vectors
- [Windsurf Context Awareness](https://docs.windsurf.com/context-awareness/overview) — Riptide/Cortex engine
- [Sourcegraph SCIP Protocol](https://sourcegraph.com/blog/announcing-scip) — Moving away from embeddings

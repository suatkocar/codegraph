//! MCP server implementation using rmcp over stdio transport.
//!
//! Provides 46 CodeGraph tools that Claude (or any MCP client) can invoke
//! to search, navigate, analyze, secure, and visualize a codebase.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    Annotated, CallToolRequestParams, CallToolResult, GetPromptRequestParams, GetPromptResult,
    ListPromptsResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams, Prompt,
    PromptArgument, PromptMessage, PromptMessageRole, RawResource, ReadResourceRequestParams,
    ReadResourceResult, ResourceContents, ServerCapabilities, ServerInfo,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{tool, tool_router, ErrorData as McpError, ServerHandler, ServiceExt};
use serde::{Deserialize, Serialize};

use crate::config::schema::CodeGraphConfig;
use crate::graph::ranking::GraphRanking;
use crate::graph::store::GraphStore;
use crate::graph::traversal::NodeWithDepth;
use crate::types::CodeNode;

// ---------------------------------------------------------------------------
// Server struct
// ---------------------------------------------------------------------------

/// CodeGraph MCP server.
///
/// Wraps a `GraphStore` in `Arc<Mutex<>>` to satisfy the `Clone + Send + Sync`
/// requirements of rmcp's `ServerHandler` trait while keeping all graph
/// operations synchronous internally.
#[derive(Clone)]
pub struct CodeGraphServer {
    store: Arc<Mutex<GraphStore>>,
    project_root: PathBuf,
    config: CodeGraphConfig,
    #[cfg(feature = "reranking")]
    reranker: Option<Arc<crate::graph::reranker::Reranker>>,
}

impl std::fmt::Debug for CodeGraphServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("CodeGraphServer");
        s.field("project_root", &self.project_root)
            .field("config", &self.config);
        #[cfg(feature = "reranking")]
        s.field(
            "reranker",
            &self.reranker.as_ref().map(|_| "Reranker(loaded)"),
        );
        s.finish()
    }
}

impl CodeGraphServer {
    /// Create a new MCP server backed by the given store.
    pub fn new(store: GraphStore) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            project_root: PathBuf::from("."),
            config: CodeGraphConfig::default(),
            #[cfg(feature = "reranking")]
            reranker: crate::graph::reranker::Reranker::try_new()
                .ok()
                .map(Arc::new),
        }
    }

    /// Create a new MCP server with an explicit project root.
    pub fn with_project_root(store: GraphStore, project_root: PathBuf) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            project_root,
            config: CodeGraphConfig::default(),
            #[cfg(feature = "reranking")]
            reranker: crate::graph::reranker::Reranker::try_new()
                .ok()
                .map(Arc::new),
        }
    }

    /// Create a new MCP server with an explicit project root and config.
    pub fn with_config(store: GraphStore, project_root: PathBuf, config: CodeGraphConfig) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            project_root,
            config,
            #[cfg(feature = "reranking")]
            reranker: crate::graph::reranker::Reranker::try_new()
                .ok()
                .map(Arc::new),
        }
    }
}

/// Resolve a symbol reference to a CodeNode from a store.
/// Accepts either a full node ID or a symbol name (returns the first match).
pub(crate) fn resolve_symbol(store: &Arc<Mutex<GraphStore>>, symbol_ref: &str) -> Option<CodeNode> {
    let store = store.lock().unwrap_or_else(|e| e.into_inner());
    if let Ok(Some(node)) = store.get_node(symbol_ref) {
        return Some(node);
    }
    if let Ok(nodes) = store.get_nodes_by_name(symbol_ref) {
        if !nodes.is_empty() {
            return Some(nodes.into_iter().next().unwrap());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Helper: serialize to JSON text
// ---------------------------------------------------------------------------

pub(crate) fn json_text<T: Serialize>(data: &T) -> String {
    serde_json::to_string_pretty(data).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

// ---------------------------------------------------------------------------
// Progressive disclosure: detail_level support
// ---------------------------------------------------------------------------

/// Controls how much detail MCP tool responses include.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetailLevel {
    /// Names, kinds only — minimal output for token efficiency.
    Summary,
    /// Default — names, kinds, file paths, lines.
    Standard,
    /// Everything including source bodies and documentation.
    Full,
}

/// Parse a user-provided detail_level string into a [`DetailLevel`].
/// Defaults to `Standard` for `None` or unrecognised values.
pub(crate) fn parse_detail_level(s: Option<&str>) -> DetailLevel {
    match s.map(|v| v.to_lowercase()).as_deref() {
        Some("summary") => DetailLevel::Summary,
        Some("full") => DetailLevel::Full,
        _ => DetailLevel::Standard,
    }
}

/// Format a [`NodeWithDepth`] from traversal results according to detail level.
pub(crate) fn format_traversal_node(nwd: &NodeWithDepth, level: DetailLevel) -> serde_json::Value {
    let c = &nwd.node;
    match level {
        DetailLevel::Summary => serde_json::json!({
            "name": c.name, "kind": c.kind.as_str(),
            "filePath": c.file_path, "depth": nwd.depth,
        }),
        DetailLevel::Standard => serde_json::json!({
            "id": c.id, "name": c.name, "kind": c.kind.as_str(),
            "filePath": c.file_path, "startLine": c.start_line, "depth": nwd.depth,
        }),
        DetailLevel::Full => {
            let mut v = serde_json::json!({
                "id": c.id, "name": c.name, "kind": c.kind.as_str(),
                "filePath": c.file_path, "startLine": c.start_line,
                "endLine": c.end_line, "depth": nwd.depth,
                "language": c.language.as_str(),
            });
            if let Some(ref body) = c.body {
                v["body"] = serde_json::json!(body);
            }
            if let Some(ref doc) = c.documentation {
                v["documentation"] = serde_json::json!(doc);
            }
            if let Some(ref qn) = c.qualified_name {
                v["qualifiedName"] = serde_json::json!(qn);
            }
            v
        }
    }
}

// ---------------------------------------------------------------------------
// Mermaid diagram helpers
// ---------------------------------------------------------------------------

pub(crate) fn mermaid_safe(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '[' | ']' | '(' | ')' | '{' | '}' | '|' | '<' | '>' | '#' | '&' | '"' => '_',
            _ => c,
        })
        .collect()
}

pub(crate) fn mermaid_id(node_id: &str) -> String {
    let mut hash: i32 = 0;
    for ch in node_id.chars() {
        hash = ((hash << 5).wrapping_sub(hash)).wrapping_add(ch as i32);
    }
    format!("n{:x}", hash.unsigned_abs())
}

pub(crate) fn generate_graph_diagram(
    center: &CodeNode,
    nodes: &[CodeNode],
    edges: &[crate::types::CodeEdge],
    title: &str,
) -> String {
    let mut lines = Vec::new();
    lines.push("```mermaid".to_string());
    lines.push("graph LR".to_string());
    lines.push(format!("  %% {} for {}", title, center.name));

    let mut emitted = HashSet::new();
    for node in nodes {
        let mid = mermaid_id(&node.id);
        if emitted.contains(&mid) {
            continue;
        }
        emitted.insert(mid.clone());
        let label = mermaid_safe(&format!("{}: {}", node.kind, node.name));
        if node.id == center.id {
            lines.push(format!("  {}[[\"{}\"]]", mid, label));
        } else {
            lines.push(format!("  {}[\"{}\"]", mid, label));
        }
    }

    let edge_labels: HashMap<&str, &str> = [
        ("calls", "calls"),
        ("imports", "imports"),
        ("extends", "extends"),
        ("implements", "impl"),
        ("references", "refs"),
        ("contains", "contains"),
    ]
    .into_iter()
    .collect();

    for edge in edges {
        let src_id = mermaid_id(&edge.source);
        let tgt_id = mermaid_id(&edge.target);
        if !emitted.contains(&src_id) || !emitted.contains(&tgt_id) {
            continue;
        }
        let kind_str = edge.kind.as_str();
        let label = edge_labels.get(kind_str).unwrap_or(&kind_str);
        lines.push(format!("  {} -->|{}| {}", src_id, label, tgt_id));
    }

    lines.push("```".to_string());
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Tool parameter structs (rmcp 0.14 uses Parameters<T> instead of #[tool(param)])
// ---------------------------------------------------------------------------

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct QueryParams {
    #[schemars(description = "Natural language or keyword search query")]
    pub query: String,
    #[schemars(description = "Maximum results to return (default 20)")]
    pub limit: Option<usize>,
    #[schemars(description = "Filter by language (e.g. 'typescript', 'python')")]
    pub language: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SearchParams {
    #[schemars(description = "Keyword query — symbol name, function name, or exact term")]
    pub query: String,
    #[schemars(description = "Maximum results to return (default 10)")]
    pub limit: Option<usize>,
    #[schemars(description = "Filter by node kind (e.g. 'function', 'class', 'method')")]
    pub kind: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SymbolDepthParams {
    #[schemars(description = "Symbol name or node ID")]
    pub symbol: String,
    #[schemars(description = "Maximum traversal depth (default 5, max 50)")]
    pub max_depth: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SymbolDepthDetailParams {
    #[schemars(description = "Symbol name or node ID")]
    pub symbol: String,
    #[schemars(description = "Maximum traversal depth (default 5, max 50)")]
    pub max_depth: Option<u32>,
    #[schemars(
        description = "Detail level: 'summary' (names only), 'standard' (default), or 'full' (includes signatures and source)"
    )]
    pub detail_level: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ImpactParams {
    #[schemars(description = "File path to analyze impact for (analyzes all symbols in the file)")]
    pub file_path: Option<String>,
    #[schemars(description = "Symbol name or node ID to analyze impact for")]
    pub symbol: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct StructureParams {
    #[schemars(
        description = "Scope to a specific directory or file path (default: entire project)"
    )]
    pub path: Option<String>,
    #[schemars(description = "Number of top symbols to return per category (default 10)")]
    pub depth: Option<usize>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SymbolParams {
    #[schemars(description = "Symbol name or node ID")]
    pub symbol: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ContextParams {
    #[schemars(description = "Natural language question or topic to gather context for")]
    pub query: String,
    #[schemars(description = "Token budget for the context document (default 8000, max 100000)")]
    pub budget: Option<usize>,
    #[schemars(
        description = "Detail level: 'summary' (names+signatures, ~50% budget), 'standard' (default), or 'full' (2x budget, all source)"
    )]
    pub detail_level: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct DiagramParams {
    #[schemars(description = "Symbol name or node ID to center the diagram on")]
    pub symbol: Option<String>,
    #[schemars(description = "Diagram type: 'dependency' (default), 'call', or 'module'")]
    pub diagram_type: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct NodeParams {
    #[schemars(description = "Symbol name or node ID to look up")]
    pub symbol: String,
    #[schemars(
        description = "Include relationships (callers, callees, dependencies) in the response (default false)"
    )]
    pub include_relations: Option<bool>,
    #[schemars(
        description = "Detail level: 'summary' (name+kind+file+signature only), 'standard' (default), or 'full' (includes body + all relationships)"
    )]
    pub detail_level: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct DeadCodeParams {
    #[schemars(
        description = "Filter by symbol kinds (comma-separated, e.g. 'function,class'). If omitted, all kinds are checked."
    )]
    pub kinds: Option<String>,
    #[schemars(description = "Include exported symbols in results (default false)")]
    pub include_exported: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct OptionalDirParams {
    #[schemars(description = "Directory path (defaults to project root)")]
    pub directory: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct FilePathParams {
    #[schemars(description = "File path")]
    pub file_path: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct FileHistoryParams {
    #[schemars(description = "File path to get history for")]
    pub file_path: String,
    #[schemars(description = "Maximum commits to return (default 20)")]
    pub limit: Option<usize>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct LimitParams {
    #[schemars(description = "Maximum items to return (default 20)")]
    pub limit: Option<usize>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct CommitParams {
    #[schemars(description = "Commit hash")]
    pub commit: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct OptionalFilePathParams {
    #[schemars(description = "Optional file path to scope to")]
    pub file_path: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ScanSecurityParams {
    #[schemars(description = "Directory to scan (defaults to project root)")]
    pub directory: Option<String>,
    #[schemars(description = "Exclude test files from scan (default true)")]
    pub exclude_tests: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct CweIdParams {
    #[schemars(description = "CWE identifier (e.g. 'CWE-89')")]
    pub cwe_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SuggestFixParams {
    #[schemars(description = "Rule ID of the finding (e.g. 'sql-injection-string-format')")]
    pub rule_id: String,
    #[schemars(description = "The matched vulnerable code snippet")]
    pub matched_code: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SourceLangParams {
    #[schemars(description = "Source code to analyze")]
    pub source: String,
    #[schemars(description = "Programming language (e.g. 'python', 'javascript')")]
    pub language: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct TraceTaintParams {
    #[schemars(description = "Source code to analyze")]
    pub source: String,
    #[schemars(description = "Programming language")]
    pub language: String,
    #[schemars(description = "Line number to trace from")]
    pub from_line: usize,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct MaxDepthParams {
    #[schemars(description = "Maximum directory depth (default 3)")]
    pub max_depth: Option<usize>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct OptionalScopeParams {
    #[schemars(description = "Optional directory to scope to")]
    pub scope: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct FindPathParams {
    #[schemars(description = "Source symbol name or node ID")]
    pub from: String,
    #[schemars(description = "Target symbol name or node ID")]
    pub to: String,
    #[schemars(description = "Maximum path depth (default 10)")]
    pub max_depth: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ComplexityParams {
    #[schemars(description = "Minimum cyclomatic complexity to include in results (default 5)")]
    pub min_complexity: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct DataFlowParams {
    #[schemars(description = "Path to source file (reads file and auto-detects language)")]
    pub file_path: Option<String>,
    #[schemars(description = "Source code to analyze (used when file_path is not provided)")]
    pub source: Option<String>,
    #[schemars(description = "Programming language (used when file_path is not provided)")]
    pub language: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ReachingDefsParams {
    #[schemars(description = "Path to source file (reads file and auto-detects language)")]
    pub file_path: Option<String>,
    #[schemars(description = "Source code to analyze (used when file_path is not provided)")]
    pub source: Option<String>,
    #[schemars(description = "Programming language (used when file_path is not provided)")]
    pub language: Option<String>,
    #[schemars(description = "Target line number")]
    pub target_line: u32,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct FrameworksParams {
    #[schemars(
        description = "Project directory to scan for manifests (defaults to the indexed project root)"
    )]
    pub project_dir: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct DeepQueryParams {
    #[schemars(
        description = "Natural language or keyword search query — re-ranked by a cross-encoder for maximum relevance"
    )]
    pub query: String,
    #[schemars(description = "Maximum results to return after re-ranking (default 10)")]
    pub limit: Option<usize>,
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

#[tool_router]
impl CodeGraphServer {
    // 1. codegraph_query — Hybrid keyword + semantic search
    #[tool(
        name = "codegraph_query",
        description = "Hybrid semantic + keyword search with query expansion. Best for conceptual queries and natural language. For exact symbol name lookups, use codegraph_search instead (10x faster). Use instead of Grep/Glob when searching for code symbols or concepts."
    )]
    async fn codegraph_query(&self, Parameters(p): Parameters<QueryParams>) -> String {
        super::tools_core::handle_query(&self.store, &p.query, p.limit, p.language, &self.config)
    }

    // 1b. codegraph_search — Fast keyword-only search (FTS5, no embeddings)
    #[tool(
        name = "codegraph_search",
        description = "Fast keyword search for exact symbol name lookups (<10ms). FTS5-only, no embeddings, no RRF fusion. Use this when you know the symbol name. For semantic/conceptual search, use codegraph_query instead."
    )]
    async fn codegraph_search(&self, Parameters(p): Parameters<SearchParams>) -> String {
        super::tools_core::handle_search(&self.store, &p.query, p.limit, p.kind, &self.config)
    }

    // 2. codegraph_dependencies — Forward dependency traversal
    #[tool(
        name = "codegraph_dependencies",
        description = "Find all dependencies of a file or module (imports, type references, etc.). Returns a dependency tree with depth levels. Use instead of Explore agents to trace imports and dependencies. For call-only relationships from a specific function, use codegraph_callees."
    )]
    async fn codegraph_dependencies(&self, Parameters(p): Parameters<SymbolDepthParams>) -> String {
        super::tools_core::handle_dependencies(&self.store, &p.symbol, p.max_depth)
    }

    // 3. codegraph_callers — Reverse call graph traversal
    #[tool(
        name = "codegraph_callers",
        description = "Find what CALLS this function/method. Returns a caller tree with depth levels. Use instead of Grep for caller analysis — 100% precise, no false positives. For all reference types (not just calls), use codegraph_find_references."
    )]
    async fn codegraph_callers(
        &self,
        Parameters(p): Parameters<SymbolDepthDetailParams>,
    ) -> String {
        super::tools_core::handle_callers(&self.store, &p.symbol, p.max_depth, p.detail_level)
    }

    // 4. codegraph_callees — Forward call graph traversal
    #[tool(
        name = "codegraph_callees",
        description = "Find all functions/methods that a symbol calls (forward call graph). Returns a callee tree with depth levels. Use instead of manual file reading to understand what a function calls."
    )]
    async fn codegraph_callees(
        &self,
        Parameters(p): Parameters<SymbolDepthDetailParams>,
    ) -> String {
        super::tools_core::handle_callees(&self.store, &p.symbol, p.max_depth, p.detail_level)
    }

    // 5. codegraph_impact — Blast radius analysis
    #[tool(
        name = "codegraph_impact",
        description = "Analyze the blast radius of changing a file or symbol. Returns affected files and functions grouped by risk level. Use before refactoring to understand what might break."
    )]
    async fn codegraph_impact(&self, Parameters(p): Parameters<ImpactParams>) -> String {
        super::tools_core::handle_impact(&self.store, p.file_path, p.symbol)
    }

    // 5. codegraph_structure — Project overview with PageRank
    #[tool(
        name = "codegraph_structure",
        description = "Get a project overview: modules, key classes/functions, and dependency summary. Uses PageRank to identify the most important symbols. Use instead of Explore agents for project overview."
    )]
    async fn codegraph_structure(&self, Parameters(p): Parameters<StructureParams>) -> String {
        super::tools_core::handle_structure(&self.store, p.path, p.depth)
    }

    // 6. codegraph_tests — Test coverage discovery
    #[tool(
        name = "codegraph_tests",
        description = "Find test files and functions that cover a given symbol. Returns test locations grouped by file."
    )]
    async fn codegraph_tests(&self, Parameters(p): Parameters<SymbolParams>) -> String {
        super::tools_core::handle_tests(&self.store, &p.symbol)
    }

    // 7. codegraph_context — 4-tier token-budgeted LLM context assembly
    #[tool(
        name = "codegraph_context",
        description = "Assemble optimal context for Claude from the code graph. Uses a tiered approach (core -> near -> extended -> background) to pack the most relevant code within a token budget. Use instead of reading multiple files — provides pre-ranked, token-budgeted context."
    )]
    async fn codegraph_context(&self, Parameters(p): Parameters<ContextParams>) -> String {
        super::tools_core::handle_context(&self.store, &p.query, p.budget, p.detail_level)
    }

    // 8. codegraph_diagram — Mermaid diagram generation
    #[tool(
        name = "codegraph_diagram",
        description = "Generate a Mermaid diagram from the code graph. Supports dependency graphs, call graphs, and module-level diagrams."
    )]
    async fn codegraph_diagram(&self, Parameters(p): Parameters<DiagramParams>) -> String {
        super::tools_core::handle_diagram(&self.store, p.symbol, p.diagram_type)
    }

    // 9. codegraph_node — Direct node lookup with full details
    #[tool(
        name = "codegraph_node",
        description = "Look up a specific code symbol by name or ID and return its full details including source code, documentation, file location, and relationships. Use instead of Grep for exact symbol lookup."
    )]
    async fn codegraph_node(&self, Parameters(p): Parameters<NodeParams>) -> String {
        super::tools_core::handle_node(&self.store, &p.symbol, p.include_relations, p.detail_level)
    }

    // 10. codegraph_dead_code — Find potentially unused symbols
    #[tool(
        name = "codegraph_dead_code",
        description = "Find potentially unused/dead code symbols that have no incoming references"
    )]
    async fn codegraph_dead_code(&self, Parameters(p): Parameters<DeadCodeParams>) -> String {
        super::tools_core::handle_dead_code(&self.store, p.kinds, p.include_exported)
    }

    // 10. codegraph_frameworks — Detect frameworks and libraries
    #[tool(
        name = "codegraph_frameworks",
        description = "Detect frameworks and libraries used in the project"
    )]
    async fn codegraph_frameworks(&self, Parameters(p): Parameters<FrameworksParams>) -> String {
        super::tools_core::handle_frameworks(&self.store, p.project_dir)
    }

    // 11. codegraph_languages — Language breakdown statistics
    #[tool(
        name = "codegraph_languages",
        description = "Show language breakdown statistics for the indexed codebase"
    )]
    async fn codegraph_languages(&self) -> String {
        super::tools_core::handle_languages(&self.store)
    }

    // 46. codegraph_deep_query — Cross-encoder re-ranked search
    #[tool(
        name = "codegraph_deep_query",
        description = "Deep search with cross-encoder re-ranking for maximum relevance. Runs hybrid search to gather candidates, then re-ranks them through a BAAI/bge-reranker-base cross-encoder that scores each (query, document) pair jointly. Much more accurate than codegraph_query for ambiguous or conceptual queries. Falls back to codegraph_query if the reranker model is unavailable."
    )]
    async fn codegraph_deep_query(&self, Parameters(p): Parameters<DeepQueryParams>) -> String {
        let top_k = p.limit.unwrap_or(10);
        // Gather candidates via hybrid search (fetch more than needed for re-ranking)
        let candidates = {
            let store = self.store.lock().unwrap_or_else(|e| e.into_inner());
            let search = crate::graph::search::HybridSearch::new(&store.conn);
            let opts = crate::graph::search::SearchOptions {
                limit: Some(30),
                ..Default::default()
            };
            match search.search(&p.query, &opts) {
                Ok(results) => results,
                Err(e) => return json_text(&serde_json::json!({"error": e.to_string()})),
            }
        };

        #[cfg(feature = "reranking")]
        {
            if let Some(ref reranker) = self.reranker {
                match crate::graph::reranker::deep_search(&p.query, reranker, candidates, top_k) {
                    Ok(reranked) => return json_text(&reranked),
                    Err(e) => return json_text(&serde_json::json!({"error": e})),
                }
            }
        }

        // Fallback: no reranker available, return truncated hybrid results
        let truncated: Vec<_> = candidates.into_iter().take(top_k).collect();
        json_text(&truncated)
    }

    // =========================================================================
    // Git Integration Tools (9)
    // =========================================================================

    // 14. codegraph_blame
    #[tool(
        name = "codegraph_blame",
        description = "Show git blame for a file — line-by-line author, date, and commit hash. Use instead of running git blame via Bash."
    )]
    async fn codegraph_blame(&self, Parameters(p): Parameters<FilePathParams>) -> String {
        super::tools_git::handle_blame(&self.project_root, &p.file_path)
    }

    // 15. codegraph_file_history
    #[tool(
        name = "codegraph_file_history",
        description = "Show commit history for a specific file."
    )]
    async fn codegraph_file_history(&self, Parameters(p): Parameters<FileHistoryParams>) -> String {
        super::tools_git::handle_file_history(&self.project_root, &p.file_path, p.limit)
    }

    // 16. codegraph_recent_changes
    #[tool(
        name = "codegraph_recent_changes",
        description = "Show recent commits across the repository."
    )]
    async fn codegraph_recent_changes(&self, Parameters(p): Parameters<LimitParams>) -> String {
        super::tools_git::handle_recent_changes(&self.project_root, p.limit)
    }

    // 17. codegraph_commit_diff
    #[tool(
        name = "codegraph_commit_diff",
        description = "Show the diff of a specific commit."
    )]
    async fn codegraph_commit_diff(&self, Parameters(p): Parameters<CommitParams>) -> String {
        super::tools_git::handle_commit_diff(&self.project_root, &p.commit)
    }

    // 18. codegraph_symbol_history
    #[tool(
        name = "codegraph_symbol_history",
        description = "Find commits that modified a specific symbol (uses git log -S)."
    )]
    async fn codegraph_symbol_history(&self, Parameters(p): Parameters<SymbolParams>) -> String {
        super::tools_git::handle_symbol_history(&self.project_root, &p.symbol)
    }

    // 19. codegraph_branch_info
    #[tool(
        name = "codegraph_branch_info",
        description = "Show current branch, tracking status, and ahead/behind counts."
    )]
    async fn codegraph_branch_info(&self) -> String {
        super::tools_git::handle_branch_info(&self.project_root)
    }

    // 20. codegraph_modified_files
    #[tool(
        name = "codegraph_modified_files",
        description = "Show working tree changes — staged, unstaged, and untracked files."
    )]
    async fn codegraph_modified_files(&self) -> String {
        super::tools_git::handle_modified_files(&self.project_root)
    }

    // 21. codegraph_hotspots
    #[tool(
        name = "codegraph_hotspots",
        description = "Find code hotspots — files with the most churn (commit count × recency)."
    )]
    async fn codegraph_hotspots(&self, Parameters(p): Parameters<LimitParams>) -> String {
        super::tools_git::handle_hotspots(&self.project_root, p.limit)
    }

    // 22. codegraph_contributors
    #[tool(
        name = "codegraph_contributors",
        description = "List contributors with commit counts and line statistics."
    )]
    async fn codegraph_contributors(
        &self,
        Parameters(p): Parameters<OptionalFilePathParams>,
    ) -> String {
        super::tools_git::handle_contributors(&self.project_root, p.file_path.as_deref())
    }

    // =========================================================================
    // Security Tools (9)
    // =========================================================================

    // 23. codegraph_scan_security
    #[tool(
        name = "codegraph_scan_security",
        description = "Scan a directory for security vulnerabilities using YAML-based pattern matching rules. Use instead of grep-based pattern matching for vulnerability detection. This is the primary security scanning tool. Supports filtering by standard (OWASP, CWE, or all)."
    )]
    async fn codegraph_scan_security(
        &self,
        Parameters(p): Parameters<ScanSecurityParams>,
    ) -> String {
        super::tools_security::handle_scan_security(
            &self.project_root,
            p.directory,
            p.exclude_tests,
        )
    }

    // 24. codegraph_check_owasp
    #[tool(
        name = "codegraph_check_owasp",
        description = "Scan for OWASP Top 10 2021 vulnerabilities. Shortcut for codegraph_scan_security with OWASP Top 10 focus. For comprehensive scanning, use codegraph_scan_security instead."
    )]
    async fn codegraph_check_owasp(&self, Parameters(p): Parameters<OptionalDirParams>) -> String {
        super::tools_security::handle_check_owasp(&self.project_root, p.directory)
    }

    // 25. codegraph_check_cwe
    #[tool(
        name = "codegraph_check_cwe",
        description = "Scan for CWE Top 25 most dangerous software weaknesses. Shortcut for codegraph_scan_security with CWE Top 25 focus. For comprehensive scanning, use codegraph_scan_security instead."
    )]
    async fn codegraph_check_cwe(&self, Parameters(p): Parameters<OptionalDirParams>) -> String {
        super::tools_security::handle_check_cwe(&self.project_root, p.directory)
    }

    // 26. codegraph_explain_vulnerability
    #[tool(
        name = "codegraph_explain_vulnerability",
        description = "Get a detailed explanation of a CWE vulnerability including severity, description, and references."
    )]
    async fn codegraph_explain_vulnerability(
        &self,
        Parameters(p): Parameters<CweIdParams>,
    ) -> String {
        super::tools_security::handle_explain_vulnerability(&p.cwe_id)
    }

    // 27. codegraph_suggest_fix
    #[tool(
        name = "codegraph_suggest_fix",
        description = "Suggest a fix for a specific security finding."
    )]
    async fn codegraph_suggest_fix(&self, Parameters(p): Parameters<SuggestFixParams>) -> String {
        super::tools_security::handle_suggest_fix(&p.rule_id, &p.matched_code)
    }

    // 28. codegraph_find_injections
    #[tool(
        name = "codegraph_find_injections",
        description = "Find injection vulnerabilities (SQL, XSS, command, path traversal) via taint analysis."
    )]
    async fn codegraph_find_injections(
        &self,
        Parameters(p): Parameters<SourceLangParams>,
    ) -> String {
        super::tools_security::handle_find_injections(&p.source, &p.language)
    }

    // 29. codegraph_taint_sources
    #[tool(
        name = "codegraph_taint_sources",
        description = "Find all taint sources (user input, file reads, network requests) in source code."
    )]
    async fn codegraph_taint_sources(&self, Parameters(p): Parameters<SourceLangParams>) -> String {
        super::tools_security::handle_taint_sources(&p.source, &p.language)
    }

    // 30. codegraph_security_summary
    #[tool(
        name = "codegraph_security_summary",
        description = "Comprehensive security risk assessment combining rule scanning and taint analysis."
    )]
    async fn codegraph_security_summary(
        &self,
        Parameters(p): Parameters<OptionalDirParams>,
    ) -> String {
        super::tools_security::handle_security_summary(&self.project_root, p.directory)
    }

    // 31. codegraph_trace_taint
    #[tool(
        name = "codegraph_trace_taint",
        description = "Trace data flow from a specific source line to find where tainted data flows."
    )]
    async fn codegraph_trace_taint(&self, Parameters(p): Parameters<TraceTaintParams>) -> String {
        super::tools_security::handle_trace_taint(&p.source, &p.language, p.from_line)
    }

    // =========================================================================
    // Existing Feature Exposure Tools (8)
    // =========================================================================

    // 32. codegraph_stats
    #[tool(
        name = "codegraph_stats",
        description = "Show index statistics — node, edge, file counts, and unresolved references."
    )]
    async fn codegraph_stats(&self) -> String {
        super::tools_analysis::handle_stats(&self.store)
    }

    // 33. codegraph_circular_imports
    #[tool(
        name = "codegraph_circular_imports",
        description = "Detect circular import dependencies using Tarjan's SCC algorithm."
    )]
    async fn codegraph_circular_imports(&self) -> String {
        super::tools_analysis::handle_circular_imports(&self.store)
    }

    // 34. codegraph_project_tree
    #[tool(
        name = "codegraph_project_tree",
        description = "Show a directory tree of the indexed project with file counts per directory."
    )]
    async fn codegraph_project_tree(&self, Parameters(p): Parameters<MaxDepthParams>) -> String {
        super::tools_analysis::handle_project_tree(&self.store, p.max_depth)
    }

    // 35. codegraph_find_references
    #[tool(
        name = "codegraph_find_references",
        description = "Find ALL references to a symbol across the codebase (calls, imports, type usage, etc.). Use instead of Grep for cross-file reference search. For call-only relationships, use codegraph_callers instead."
    )]
    async fn codegraph_find_references(&self, Parameters(p): Parameters<SymbolParams>) -> String {
        super::tools_analysis::handle_find_references(&self.store, &p.symbol)
    }

    // 36. codegraph_export_map
    #[tool(
        name = "codegraph_export_map",
        description = "List all exported symbols grouped by file."
    )]
    async fn codegraph_export_map(&self) -> String {
        super::tools_analysis::handle_export_map(&self.store)
    }

    // 37. codegraph_import_graph
    #[tool(
        name = "codegraph_import_graph",
        description = "Visualize the import graph as a Mermaid diagram."
    )]
    async fn codegraph_import_graph(
        &self,
        Parameters(p): Parameters<OptionalScopeParams>,
    ) -> String {
        super::tools_analysis::handle_import_graph(&self.store, p.scope)
    }

    // 38. codegraph_file
    #[tool(
        name = "codegraph_file",
        description = "Get all symbols defined in a specific file. Use before reading a file to understand its structure first."
    )]
    async fn codegraph_file(&self, Parameters(p): Parameters<FilePathParams>) -> String {
        super::tools_analysis::handle_file(&self.store, &p.file_path)
    }

    // =========================================================================
    // Call Graph & Analysis Tools (6)
    // =========================================================================

    // 39. codegraph_find_path
    #[tool(
        name = "codegraph_find_path",
        description = "Find the shortest call path between two functions using BFS on the call graph."
    )]
    async fn codegraph_find_path(&self, Parameters(p): Parameters<FindPathParams>) -> String {
        super::tools_dataflow::handle_find_path(&self.store, &p.from, &p.to, p.max_depth)
    }

    // 40. codegraph_complexity
    #[tool(
        name = "codegraph_complexity",
        description = "Calculate cyclomatic and cognitive complexity for all functions in the codebase."
    )]
    async fn codegraph_complexity(&self, Parameters(p): Parameters<ComplexityParams>) -> String {
        super::tools_dataflow::handle_complexity(&self.store, p.min_complexity)
    }

    // 41. codegraph_data_flow
    #[tool(
        name = "codegraph_data_flow",
        description = "Analyze variable def-use chains. Pass file_path to read a file directly (language auto-detected), or pass source + language explicitly."
    )]
    async fn codegraph_data_flow(&self, Parameters(p): Parameters<DataFlowParams>) -> String {
        super::tools_dataflow::handle_data_flow(
            p.file_path.as_deref(),
            p.source.as_deref(),
            p.language.as_deref(),
            &self.project_root,
        )
    }

    // 42. codegraph_dead_stores
    #[tool(
        name = "codegraph_dead_stores",
        description = "Find variable assignments that are never subsequently read (dead stores). Pass file_path to read a file directly (language auto-detected), or pass source + language explicitly."
    )]
    async fn codegraph_dead_stores(&self, Parameters(p): Parameters<DataFlowParams>) -> String {
        super::tools_dataflow::handle_dead_stores(
            p.file_path.as_deref(),
            p.source.as_deref(),
            p.language.as_deref(),
            &self.project_root,
        )
    }

    // 43. codegraph_find_uninitialized
    #[tool(
        name = "codegraph_find_uninitialized",
        description = "Find variables used before initialization. Pass file_path to read a file directly (language auto-detected), or pass source + language explicitly."
    )]
    async fn codegraph_find_uninitialized(
        &self,
        Parameters(p): Parameters<DataFlowParams>,
    ) -> String {
        super::tools_dataflow::handle_find_uninitialized(
            p.file_path.as_deref(),
            p.source.as_deref(),
            p.language.as_deref(),
            &self.project_root,
        )
    }

    // 44. codegraph_reaching_defs
    #[tool(
        name = "codegraph_reaching_defs",
        description = "Find which variable definitions reach a specific line. Pass file_path to read a file directly (language auto-detected), or pass source + language explicitly."
    )]
    async fn codegraph_reaching_defs(
        &self,
        Parameters(p): Parameters<ReachingDefsParams>,
    ) -> String {
        super::tools_dataflow::handle_reaching_defs(
            p.file_path.as_deref(),
            p.source.as_deref(),
            p.language.as_deref(),
            p.target_line,
            &self.project_root,
        )
    }
}

// ---------------------------------------------------------------------------
// ServerHandler impl — manual list_tools/call_tool for preset filtering
// ---------------------------------------------------------------------------

impl ServerHandler for CodeGraphServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "CodeGraph — codebase intelligence MCP server with 46 tools across 32 languages. \
                 ALWAYS prefer CodeGraph tools over Grep/Glob/Explore agents for code analysis: \
                 codegraph_query (semantic search), codegraph_dependencies (import tracing), \
                 codegraph_callers (call graph), codegraph_find_references (cross-file refs), \
                 codegraph_structure (project overview), codegraph_node (symbol lookup). \
                 These use a pre-built AST index and return results in milliseconds."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
            server_info: rmcp::model::Implementation {
                name: "codegraph".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, McpError>> + Send + '_ {
        let resources = vec![
            Annotated::new(
                RawResource {
                    uri: "codegraph://status".into(),
                    name: "Index Status".into(),
                    title: None,
                    description: Some(
                        "CodeGraph index health: node, edge, file counts, and unresolved references."
                            .into(),
                    ),
                    mime_type: Some("application/json".into()),
                    size: None,
                    icons: None,
                    meta: None,
                },
                None,
            ),
            Annotated::new(
                RawResource {
                    uri: "codegraph://overview".into(),
                    name: "Project Overview".into(),
                    title: None,
                    description: Some(
                        "Project structure summary: top symbols by PageRank, file counts, language breakdown."
                            .into(),
                    ),
                    mime_type: Some("application/json".into()),
                    size: None,
                    icons: None,
                    meta: None,
                },
                None,
            ),
        ];
        std::future::ready(Ok(ListResourcesResult {
            meta: None,
            next_cursor: None,
            resources,
        }))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResult, McpError>> + Send + '_ {
        let result = match request.uri.as_str() {
            "codegraph://status" => {
                let store = self.store.lock().unwrap_or_else(|e| e.into_inner());
                match store.get_stats() {
                    Ok(stats) => {
                        let unresolved = store.get_unresolved_ref_count().unwrap_or(0);
                        let json = serde_json::json!({
                            "version": env!("CARGO_PKG_VERSION"),
                            "projectRoot": self.project_root.to_string_lossy(),
                            "nodes": stats.nodes,
                            "edges": stats.edges,
                            "files": stats.files,
                            "unresolvedRefs": unresolved,
                            "status": "healthy",
                        });
                        Ok(ReadResourceResult {
                            contents: vec![ResourceContents::text(
                                serde_json::to_string_pretty(&json).unwrap_or_default(),
                                "codegraph://status",
                            )],
                        })
                    }
                    Err(e) => Err(McpError::internal_error(
                        format!("Failed to read index stats: {e}"),
                        None,
                    )),
                }
            }
            "codegraph://overview" => {
                let store = self.store.lock().unwrap_or_else(|e| e.into_inner());
                let stats = match store.get_stats() {
                    Ok(s) => s,
                    Err(e) => {
                        return std::future::ready(Err(McpError::internal_error(
                            format!("Failed to read stats: {e}"),
                            None,
                        )));
                    }
                };
                let all_nodes = store.get_all_nodes().unwrap_or_default();
                let file_count = all_nodes
                    .iter()
                    .map(|n| &n.file_path)
                    .collect::<HashSet<_>>()
                    .len();

                // Language breakdown
                let mut lang_counts: HashMap<String, usize> = HashMap::new();
                for node in &all_nodes {
                    *lang_counts.entry(node.language.to_string()).or_default() += 1;
                }
                let mut langs: Vec<_> = lang_counts.into_iter().collect();
                langs.sort_by(|a, b| b.1.cmp(&a.1));

                // Top symbols by PageRank
                let ranking = GraphRanking::new(&store);
                let page_rank = ranking.compute_page_rank(0.85, 100);
                let top_symbols: Vec<serde_json::Value> = page_rank
                    .iter()
                    .take(10)
                    .filter_map(|r| {
                        store.get_node(&r.node_id).ok().flatten().map(|n| {
                            serde_json::json!({
                                "name": n.name,
                                "kind": n.kind.as_str(),
                                "file": n.file_path,
                                "rank": format!("{:.4}", r.score),
                            })
                        })
                    })
                    .collect();

                let json = serde_json::json!({
                    "totalNodes": stats.nodes,
                    "totalEdges": stats.edges,
                    "totalFiles": file_count,
                    "languages": langs.iter().take(10).map(|(l, c)| serde_json::json!({"language": l, "symbols": c})).collect::<Vec<_>>(),
                    "topSymbols": top_symbols,
                });
                Ok(ReadResourceResult {
                    contents: vec![ResourceContents::text(
                        serde_json::to_string_pretty(&json).unwrap_or_default(),
                        "codegraph://overview",
                    )],
                })
            }
            uri => Err(McpError::resource_not_found(
                format!("Unknown resource: {uri}"),
                None,
            )),
        };
        std::future::ready(result)
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListPromptsResult, McpError>> + Send + '_ {
        let prompts = vec![
            Prompt::new(
                "review-security",
                Some("Run a comprehensive security review on a directory — scans for OWASP Top 10, CWE Top 25, and injection vulnerabilities."),
                Some(vec![PromptArgument {
                    name: "directory".into(),
                    title: None,
                    description: Some("Directory path to scan for security vulnerabilities".into()),
                    required: Some(true),
                }]),
            ),
            Prompt::new(
                "explain-function",
                Some("Deep-dive into a function or method — shows its source, callers, callees, and surrounding context."),
                Some(vec![PromptArgument {
                    name: "symbol".into(),
                    title: None,
                    description: Some("Function or method name (or node ID) to explain".into()),
                    required: Some(true),
                }]),
            ),
            Prompt::new(
                "pre-refactor-check",
                Some("Analyze the blast radius before refactoring a symbol — shows callers, dependencies, and affected files."),
                Some(vec![PromptArgument {
                    name: "symbol".into(),
                    title: None,
                    description: Some("Symbol name (or node ID) to check before refactoring".into()),
                    required: Some(true),
                }]),
            ),
        ];
        std::future::ready(Ok(ListPromptsResult {
            meta: None,
            next_cursor: None,
            prompts,
        }))
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<GetPromptResult, McpError>> + Send + '_ {
        let args = request.arguments.unwrap_or_default();

        let result = match request.name.as_str() {
            "review-security" => {
                let directory = args
                    .get("directory")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");
                Ok(GetPromptResult {
                    description: Some("Security review workflow".into()),
                    messages: vec![
                        PromptMessage::new_text(
                            PromptMessageRole::User,
                            format!(
                                "Run a comprehensive security review on the directory `{directory}`. \
                                 Follow these steps:\n\n\
                                 1. Call `codegraph_scan_security` with directory=\"{directory}\" to scan for all vulnerability patterns.\n\
                                 2. Call `codegraph_check_owasp` with directory=\"{directory}\" for OWASP Top 10 coverage.\n\
                                 3. Call `codegraph_check_cwe` with directory=\"{directory}\" for CWE Top 25 coverage.\n\
                                 4. For any high/critical findings, call `codegraph_suggest_fix` with the rule_id and matched_code.\n\
                                 5. Summarize all findings grouped by severity with remediation recommendations."
                            ),
                        ),
                    ],
                })
            }
            "explain-function" => {
                let symbol = args
                    .get("symbol")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                Ok(GetPromptResult {
                    description: Some("Function deep-dive workflow".into()),
                    messages: vec![
                        PromptMessage::new_text(
                            PromptMessageRole::User,
                            format!(
                                "Explain the function/method `{symbol}` in detail. Follow these steps:\n\n\
                                 1. Call `codegraph_node` with symbol=\"{symbol}\" and include_relations=true, detail_level=\"full\" to get its source, documentation, and relationships.\n\
                                 2. Call `codegraph_callers` with symbol=\"{symbol}\" and max_depth=2 to see what calls it.\n\
                                 3. Call `codegraph_callees` with symbol=\"{symbol}\" and max_depth=2 to see what it calls.\n\
                                 4. Call `codegraph_context` with query=\"{symbol}\" to get surrounding context.\n\
                                 5. Provide a clear explanation of:\n\
                                    - What the function does (purpose and behavior)\n\
                                    - Its inputs, outputs, and side effects\n\
                                    - How it fits in the call graph (who calls it, what it calls)\n\
                                    - Any noteworthy patterns or potential issues"
                            ),
                        ),
                    ],
                })
            }
            "pre-refactor-check" => {
                let symbol = args
                    .get("symbol")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                Ok(GetPromptResult {
                    description: Some("Pre-refactor impact analysis workflow".into()),
                    messages: vec![
                        PromptMessage::new_text(
                            PromptMessageRole::User,
                            format!(
                                "Analyze the impact of refactoring `{symbol}` before making changes. Follow these steps:\n\n\
                                 1. Call `codegraph_impact` with symbol=\"{symbol}\" to get the blast radius analysis.\n\
                                 2. Call `codegraph_callers` with symbol=\"{symbol}\" and max_depth=3 to find all transitive callers.\n\
                                 3. Call `codegraph_dependencies` with symbol=\"{symbol}\" to trace its dependency tree.\n\
                                 4. Call `codegraph_tests` with symbol=\"{symbol}\" to find existing test coverage.\n\
                                 5. Summarize:\n\
                                    - Number of direct and transitive callers affected\n\
                                    - Files that would need updates\n\
                                    - Existing test coverage for the symbol\n\
                                    - Risk assessment (low/medium/high) with rationale\n\
                                    - Recommended refactoring approach"
                            ),
                        ),
                    ],
                })
            }
            name => Err(McpError::invalid_params(
                format!("Unknown prompt: {name}"),
                None,
            )),
        };

        std::future::ready(result)
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        // Get the full tool list from the macro-generated ToolRouter
        let all_tools = Self::tool_router().list_all();

        // Build the set of enabled tool names from config + registry
        let enabled = super::registry::enabled_tool_names(&self.config);

        // Filter: keep only tools whose name is in the enabled set
        let filtered = all_tools
            .into_iter()
            .filter(|t| enabled.contains(t.name.as_ref()))
            .collect();

        std::future::ready(Ok(ListToolsResult {
            meta: None,
            next_cursor: None,
            tools: filtered,
        }))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        // Check if the tool is enabled before dispatching
        let enabled = super::registry::enabled_tool_names(&self.config);
        if !enabled.contains(request.name.as_ref()) {
            return Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                format!(
                    "Tool '{}' is not available in the current preset ({}). \
                     Change the preset in .codegraph.yaml or set CODEGRAPH_PRESET=full to enable all tools.",
                    request.name, self.config.preset
                ),
            )]));
        }

        // Dispatch to the macro-generated tool handler
        let tool_context =
            rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        Self::tool_router().call(tool_context).await
    }
}

// ---------------------------------------------------------------------------
// Public entry point: run the MCP server over stdio
// ---------------------------------------------------------------------------

/// Start the MCP server on stdin/stdout.
///
/// This blocks until the client disconnects or a shutdown signal is received.
pub async fn run_server(store: GraphStore) -> Result<(), Box<dyn std::error::Error>> {
    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let config = crate::config::loader::load_config(None, Some(&project_root)).unwrap_or_default();
    let server = CodeGraphServer::with_config(store, project_root, config);
    let transport = rmcp::transport::io::stdio();
    let running = server.serve(transport).await.inspect_err(|e| {
        tracing::error!("MCP server error: {}", e);
    })?;
    let _ = running.waiting().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::initialize_database;
    use crate::types::{CodeEdge, CodeNode, EdgeKind, Language, NodeKind};

    fn setup_server() -> CodeGraphServer {
        let conn = initialize_database(":memory:").expect("schema init");
        let store = GraphStore::from_connection(conn);
        CodeGraphServer::new(store)
    }

    fn make_node(
        id: &str,
        name: &str,
        file: &str,
        kind: NodeKind,
        line: u32,
        exported: Option<bool>,
    ) -> CodeNode {
        CodeNode {
            id: id.to_string(),
            name: name.to_string(),
            qualified_name: None,
            kind,
            file_path: file.to_string(),
            start_line: line,
            end_line: line + 5,
            start_column: 0,
            end_column: 1,
            language: Language::TypeScript,
            body: Some(format!("function {}() {{}}", name)),
            documentation: None,
            exported,
        }
    }

    fn make_node_with_lang(
        id: &str,
        name: &str,
        file: &str,
        kind: NodeKind,
        line: u32,
        lang: Language,
    ) -> CodeNode {
        CodeNode {
            id: id.to_string(),
            name: name.to_string(),
            qualified_name: None,
            kind,
            file_path: file.to_string(),
            start_line: line,
            end_line: line + 5,
            start_column: 0,
            end_column: 1,
            language: lang,
            body: None,
            documentation: None,
            exported: None,
        }
    }

    fn make_edge(source: &str, target: &str, kind: EdgeKind, file: &str, line: u32) -> CodeEdge {
        CodeEdge {
            source: source.to_string(),
            target: target.to_string(),
            kind,
            file_path: file.to_string(),
            line,
            metadata: None,
        }
    }

    // -- codegraph_callees ----------------------------------------------------

    #[tokio::test]
    async fn callees_returns_forward_call_graph() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node("n1", "main", "src/main.ts", NodeKind::Function, 1, None),
                    make_node("n2", "helper", "src/helper.ts", NodeKind::Function, 1, None),
                    make_node("n3", "util", "src/util.ts", NodeKind::Function, 1, None),
                ])
                .unwrap();
            store
                .upsert_edges(&[
                    make_edge("n1", "n2", EdgeKind::Calls, "src/main.ts", 5),
                    make_edge("n2", "n3", EdgeKind::Calls, "src/helper.ts", 3),
                    make_edge("n1", "n3", EdgeKind::Imports, "src/main.ts", 1),
                ])
                .unwrap();
        }

        let result = server
            .codegraph_callees(Parameters(SymbolDepthDetailParams {
                symbol: "main".to_string(),
                max_depth: None,
                detail_level: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(json["calleeCount"].as_u64().unwrap(), 2);
        let callees = json["callees"].as_array().unwrap();
        assert_eq!(callees[0]["name"].as_str().unwrap(), "helper");
        assert_eq!(callees[0]["depth"].as_u64().unwrap(), 1);
        assert_eq!(callees[1]["name"].as_str().unwrap(), "util");
        assert_eq!(callees[1]["depth"].as_u64().unwrap(), 2);
    }

    #[tokio::test]
    async fn callees_not_found() {
        let server = setup_server();
        let result = server
            .codegraph_callees(Parameters(SymbolDepthDetailParams {
                symbol: "nonexistent".to_string(),
                max_depth: None,
                detail_level: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["error"].as_str().unwrap().contains("not found"));
    }

    // -- codegraph_node -------------------------------------------------------

    #[tokio::test]
    async fn node_returns_full_details() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[make_node(
                    "n1",
                    "processData",
                    "src/processor.ts",
                    NodeKind::Function,
                    10,
                    Some(true),
                )])
                .unwrap();
        }

        let result = server
            .codegraph_node(Parameters(NodeParams {
                symbol: "processData".to_string(),
                include_relations: None,
                detail_level: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(json["name"].as_str().unwrap(), "processData");
        assert_eq!(json["kind"].as_str().unwrap(), "function");
        assert_eq!(json["filePath"].as_str().unwrap(), "src/processor.ts");
        assert_eq!(json["startLine"].as_u64().unwrap(), 10);
        assert!(json["exported"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn node_with_relations() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node("n1", "caller", "src/a.ts", NodeKind::Function, 1, None),
                    make_node("n2", "target", "src/b.ts", NodeKind::Function, 1, None),
                    make_node("n3", "callee", "src/c.ts", NodeKind::Function, 1, None),
                ])
                .unwrap();
            store
                .upsert_edges(&[
                    make_edge("n1", "n2", EdgeKind::Calls, "src/a.ts", 5),
                    make_edge("n2", "n3", EdgeKind::Calls, "src/b.ts", 3),
                ])
                .unwrap();
        }

        let result = server
            .codegraph_node(Parameters(NodeParams {
                symbol: "target".to_string(),
                include_relations: Some(true),
                detail_level: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(json["name"].as_str().unwrap(), "target");
        let callers = json["callers"].as_array().unwrap();
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0]["name"].as_str().unwrap(), "caller");
        let callees = json["callees"].as_array().unwrap();
        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0]["name"].as_str().unwrap(), "callee");
    }

    #[tokio::test]
    async fn node_not_found_with_suggestions() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[make_node(
                    "n1",
                    "processData",
                    "src/a.ts",
                    NodeKind::Function,
                    1,
                    None,
                )])
                .unwrap();
        }

        let result = server
            .codegraph_node(Parameters(NodeParams {
                symbol: "process".to_string(),
                include_relations: None,
                detail_level: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert!(json["error"].as_str().unwrap().contains("not found"));
        let suggestions = json["suggestions"].as_array().unwrap();
        assert!(suggestions
            .iter()
            .any(|s| s.as_str().unwrap() == "processData"));
    }

    // -- codegraph_dead_code --------------------------------------------------

    #[tokio::test]
    async fn dead_code_finds_unreferenced_symbols() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node("n1", "usedFunc", "src/a.ts", NodeKind::Function, 1, None),
                    make_node("n2", "unusedFunc", "src/b.ts", NodeKind::Function, 1, None),
                    make_node("n3", "caller", "src/c.ts", NodeKind::Function, 1, None),
                ])
                .unwrap();
            store
                .upsert_edge(&make_edge("n3", "n1", EdgeKind::Calls, "src/c.ts", 5))
                .unwrap();
        }

        let result = server
            .codegraph_dead_code(Parameters(DeadCodeParams {
                kinds: None,
                include_exported: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert!(json["deadCodeCount"].as_u64().unwrap() >= 2);
        let files = json["files"].as_array().unwrap();
        let all_names: Vec<&str> = files
            .iter()
            .flat_map(|f| {
                f["symbols"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s["name"].as_str().unwrap())
            })
            .collect();
        assert!(all_names.contains(&"unusedFunc"));
        assert!(all_names.contains(&"caller"));
        assert!(!all_names.contains(&"usedFunc"));
    }

    #[tokio::test]
    async fn dead_code_filters_by_kind() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node("n1", "unusedFunc", "src/a.ts", NodeKind::Function, 1, None),
                    make_node("n2", "UnusedClass", "src/b.ts", NodeKind::Class, 1, None),
                ])
                .unwrap();
        }

        let result = server
            .codegraph_dead_code(Parameters(DeadCodeParams {
                kinds: Some("function".to_string()),
                include_exported: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(json["deadCodeCount"].as_u64().unwrap(), 1);
        let files = json["files"].as_array().unwrap();
        let name = files[0]["symbols"][0]["name"].as_str().unwrap();
        assert_eq!(name, "unusedFunc");
    }

    #[tokio::test]
    async fn dead_code_empty_graph() {
        let server = setup_server();
        let result = server
            .codegraph_dead_code(Parameters(DeadCodeParams {
                kinds: None,
                include_exported: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(json["deadCodeCount"].as_u64().unwrap(), 0);
        assert!(json["message"].as_str().is_some());
    }

    // -- codegraph_frameworks -------------------------------------------------

    #[tokio::test]
    async fn frameworks_with_explicit_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies": {"react": "^18.0.0"}}"#,
        )
        .unwrap();

        let server = setup_server();
        let result = server
            .codegraph_frameworks(Parameters(FrameworksParams {
                project_dir: Some(dir.path().to_str().unwrap().to_string()),
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(json["frameworkCount"].as_u64().unwrap(), 1);
        let frameworks = json["frameworks"].as_array().unwrap();
        assert_eq!(frameworks[0]["name"].as_str().unwrap(), "React");
        assert_eq!(frameworks[0]["language"].as_str().unwrap(), "javascript");
        assert_eq!(frameworks[0]["category"].as_str().unwrap(), "web");
    }

    #[tokio::test]
    async fn frameworks_empty_project() {
        let dir = tempfile::tempdir().unwrap();
        let server = setup_server();
        let result = server
            .codegraph_frameworks(Parameters(FrameworksParams {
                project_dir: Some(dir.path().to_str().unwrap().to_string()),
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(json["frameworkCount"].as_u64().unwrap(), 0);
        assert!(json["message"].as_str().is_some());
    }

    #[tokio::test]
    async fn frameworks_no_dir_with_empty_store() {
        let server = setup_server();
        let result = server
            .codegraph_frameworks(Parameters(FrameworksParams { project_dir: None }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();

        // With no indexed files, defaults to "." which likely has no manifests
        // or finds the current project's Cargo.toml
        assert!(json["frameworkCount"].is_number());
    }

    // -- codegraph_languages --------------------------------------------------

    #[tokio::test]
    async fn languages_shows_breakdown() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node_with_lang(
                        "n1",
                        "foo",
                        "src/a.ts",
                        NodeKind::Function,
                        1,
                        Language::TypeScript,
                    ),
                    make_node_with_lang(
                        "n2",
                        "bar",
                        "src/a.ts",
                        NodeKind::Function,
                        10,
                        Language::TypeScript,
                    ),
                    make_node_with_lang(
                        "n3",
                        "baz",
                        "src/b.py",
                        NodeKind::Function,
                        1,
                        Language::Python,
                    ),
                ])
                .unwrap();
            store
                .upsert_edge(&make_edge("n1", "n2", EdgeKind::Calls, "src/a.ts", 5))
                .unwrap();
        }

        let result = server.codegraph_languages().await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(json["languageCount"].as_u64().unwrap(), 2);
        assert_eq!(json["totalFiles"].as_u64().unwrap(), 2);
        assert_eq!(json["totalSymbols"].as_u64().unwrap(), 3);
        assert_eq!(json["totalEdges"].as_u64().unwrap(), 1);

        let languages = json["languages"].as_array().unwrap();
        assert_eq!(languages.len(), 2);

        // TypeScript has more symbols so should be first
        let ts = &languages[0];
        assert_eq!(ts["language"].as_str().unwrap(), "typescript");
        assert_eq!(ts["files"].as_u64().unwrap(), 1);
        assert_eq!(ts["symbols"].as_u64().unwrap(), 2);
        assert_eq!(ts["edges"].as_u64().unwrap(), 1);
        assert_eq!(ts["percentage"].as_str().unwrap(), "66.7%");

        let py = &languages[1];
        assert_eq!(py["language"].as_str().unwrap(), "python");
        assert_eq!(py["files"].as_u64().unwrap(), 1);
        assert_eq!(py["symbols"].as_u64().unwrap(), 1);
        assert_eq!(py["percentage"].as_str().unwrap(), "33.3%");
    }

    #[tokio::test]
    async fn languages_empty_graph() {
        let server = setup_server();
        let result = server.codegraph_languages().await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(json["languageCount"].as_u64().unwrap(), 0);
        assert!(json["message"].as_str().is_some());
    }

    #[tokio::test]
    async fn languages_single_language() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node_with_lang(
                        "n1",
                        "foo",
                        "src/a.rs",
                        NodeKind::Function,
                        1,
                        Language::Rust,
                    ),
                    make_node_with_lang(
                        "n2",
                        "bar",
                        "src/b.rs",
                        NodeKind::Function,
                        1,
                        Language::Rust,
                    ),
                ])
                .unwrap();
        }

        let result = server.codegraph_languages().await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(json["languageCount"].as_u64().unwrap(), 1);
        let languages = json["languages"].as_array().unwrap();
        assert_eq!(languages[0]["language"].as_str().unwrap(), "rust");
        assert_eq!(languages[0]["percentage"].as_str().unwrap(), "100.0%");
    }

    // =====================================================================
    // NEW TESTS: Phase 18C — MCP Server comprehensive coverage
    // =====================================================================

    // -- codegraph_stats --------------------------------------------------

    #[tokio::test]
    async fn stats_empty_graph() {
        let server = setup_server();
        let result = server.codegraph_stats().await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(json["nodes"].as_u64().unwrap(), 0);
        assert_eq!(json["edges"].as_u64().unwrap(), 0);
        assert_eq!(json["files"].as_u64().unwrap(), 0);
        assert_eq!(json["unresolvedRefs"].as_u64().unwrap(), 0);
    }

    #[tokio::test]
    async fn stats_with_data() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node("n1", "a", "src/a.ts", NodeKind::Function, 1, None),
                    make_node("n2", "b", "src/b.ts", NodeKind::Function, 1, None),
                ])
                .unwrap();
            store
                .upsert_edge(&make_edge("n1", "n2", EdgeKind::Calls, "src/a.ts", 5))
                .unwrap();
            store
                .insert_unresolved_ref("n1", "./missing", "import", "src/a.ts", 1)
                .unwrap();
        }
        let result = server.codegraph_stats().await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(json["nodes"].as_u64().unwrap(), 2);
        assert_eq!(json["edges"].as_u64().unwrap(), 1);
        assert_eq!(json["files"].as_u64().unwrap(), 2);
        assert_eq!(json["unresolvedRefs"].as_u64().unwrap(), 1);
    }

    // -- codegraph_circular_imports ---------------------------------------

    #[tokio::test]
    async fn circular_imports_no_cycles() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node("n1", "a", "a.ts", NodeKind::Function, 1, None),
                    make_node("n2", "b", "b.ts", NodeKind::Function, 1, None),
                ])
                .unwrap();
            store
                .upsert_edge(&make_edge("n1", "n2", EdgeKind::Calls, "a.ts", 5))
                .unwrap();
        }
        let result = server.codegraph_circular_imports().await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(json["cycleCount"].as_u64().unwrap(), 0);
        assert!(json["message"].as_str().is_some());
    }

    #[tokio::test]
    async fn circular_imports_with_cycle() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node("n1", "a", "a.ts", NodeKind::Function, 1, None),
                    make_node("n2", "b", "b.ts", NodeKind::Function, 1, None),
                ])
                .unwrap();
            store
                .upsert_edges(&[
                    make_edge("n1", "n2", EdgeKind::Calls, "a.ts", 5),
                    make_edge("n2", "n1", EdgeKind::Calls, "b.ts", 3),
                ])
                .unwrap();
        }
        let result = server.codegraph_circular_imports().await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(json["cycleCount"].as_u64().unwrap(), 1);
        let cycles = json["cycles"].as_array().unwrap();
        assert_eq!(cycles[0]["size"].as_u64().unwrap(), 2);
    }

    // -- codegraph_project_tree -------------------------------------------

    #[tokio::test]
    async fn project_tree_empty() {
        let server = setup_server();
        let result = server
            .codegraph_project_tree(Parameters(MaxDepthParams { max_depth: None }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["tree"].is_array() || json["message"].is_string() || json.is_object());
    }

    #[tokio::test]
    async fn project_tree_with_files() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node("n1", "a", "src/lib/a.ts", NodeKind::Function, 1, None),
                    make_node("n2", "b", "src/lib/b.ts", NodeKind::Function, 1, None),
                    make_node("n3", "c", "src/utils/c.ts", NodeKind::Function, 1, None),
                ])
                .unwrap();
        }
        let result = server
            .codegraph_project_tree(Parameters(MaxDepthParams { max_depth: Some(2) }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json.is_object());
    }

    // -- codegraph_find_references ----------------------------------------

    #[tokio::test]
    async fn find_references_existing_symbol() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node("n1", "helper", "src/a.ts", NodeKind::Function, 1, None),
                    make_node("n2", "caller1", "src/b.ts", NodeKind::Function, 1, None),
                    make_node("n3", "caller2", "src/c.ts", NodeKind::Function, 1, None),
                ])
                .unwrap();
            store
                .upsert_edges(&[
                    make_edge("n2", "n1", EdgeKind::Calls, "src/b.ts", 5),
                    make_edge("n3", "n1", EdgeKind::Calls, "src/c.ts", 3),
                ])
                .unwrap();
        }
        let result = server
            .codegraph_find_references(Parameters(SymbolParams {
                symbol: "helper".to_string(),
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["referenceCount"].as_u64().unwrap() >= 2);
    }

    #[tokio::test]
    async fn find_references_nonexistent_symbol() {
        let server = setup_server();
        let result = server
            .codegraph_find_references(Parameters(SymbolParams {
                symbol: "nonexistent".to_string(),
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["error"].is_string());
    }

    // -- codegraph_export_map ---------------------------------------------

    #[tokio::test]
    async fn export_map_with_exports() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node(
                        "n1",
                        "publicFunc",
                        "src/a.ts",
                        NodeKind::Function,
                        1,
                        Some(true),
                    ),
                    make_node(
                        "n2",
                        "privateFunc",
                        "src/a.ts",
                        NodeKind::Function,
                        10,
                        Some(false),
                    ),
                    make_node(
                        "n3",
                        "anotherPublic",
                        "src/b.ts",
                        NodeKind::Function,
                        1,
                        Some(true),
                    ),
                ])
                .unwrap();
        }
        let result = server.codegraph_export_map().await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json.is_object());
    }

    #[tokio::test]
    async fn export_map_empty() {
        let server = setup_server();
        let result = server.codegraph_export_map().await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json.is_object());
    }

    // -- codegraph_find_path ----------------------------------------------

    #[tokio::test]
    async fn find_path_existing() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node("n1", "start", "src/a.ts", NodeKind::Function, 1, None),
                    make_node("n2", "middle", "src/b.ts", NodeKind::Function, 1, None),
                    make_node("n3", "end", "src/c.ts", NodeKind::Function, 1, None),
                ])
                .unwrap();
            store
                .upsert_edges(&[
                    make_edge("n1", "n2", EdgeKind::Calls, "src/a.ts", 5),
                    make_edge("n2", "n3", EdgeKind::Calls, "src/b.ts", 3),
                ])
                .unwrap();
        }
        let result = server
            .codegraph_find_path(Parameters(FindPathParams {
                from: "start".to_string(),
                to: "end".to_string(),
                max_depth: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["path"].is_array());
        assert_eq!(json["pathLength"].as_u64().unwrap(), 3);
    }

    #[tokio::test]
    async fn find_path_no_route() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node("n1", "isolated_a", "src/a.ts", NodeKind::Function, 1, None),
                    make_node("n2", "isolated_b", "src/b.ts", NodeKind::Function, 1, None),
                ])
                .unwrap();
        }
        let result = server
            .codegraph_find_path(Parameters(FindPathParams {
                from: "isolated_a".to_string(),
                to: "isolated_b".to_string(),
                max_depth: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["message"].as_str().unwrap().contains("No call path"));
    }

    // -- codegraph_complexity ---------------------------------------------

    #[tokio::test]
    async fn complexity_analysis() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            let meta = serde_json::json!({
                "body": "function process(x) {\n  if (x > 0) {\n    return true;\n  }\n  return false;\n}"
            });
            store.conn.execute(
                "INSERT INTO nodes (id, type, name, file_path, start_line, end_line, language, source_hash, metadata) \
                 VALUES ('fn:a:1', 'function', 'process', 'src/a.ts', 1, 6, 'typescript', 'h1', ?1)",
                [meta.to_string()],
            ).unwrap();
        }
        let result = server
            .codegraph_complexity(Parameters(ComplexityParams {
                min_complexity: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["functions"].is_array());
    }

    #[tokio::test]
    async fn complexity_empty_graph() {
        let server = setup_server();
        let result = server
            .codegraph_complexity(Parameters(ComplexityParams {
                min_complexity: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json.is_object());
    }

    // -- codegraph_data_flow ----------------------------------------------

    #[tokio::test]
    async fn data_flow_analysis() {
        let server = setup_server();
        let result = server
            .codegraph_data_flow(Parameters(DataFlowParams {
                file_path: None,
                source: Some("let x = 10;\nlet y = x + 5;".to_string()),
                language: Some("javascript".to_string()),
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["chains"].is_array());
    }

    #[tokio::test]
    async fn data_flow_empty_source() {
        let server = setup_server();
        let result = server
            .codegraph_data_flow(Parameters(DataFlowParams {
                file_path: None,
                source: Some("".to_string()),
                language: Some("javascript".to_string()),
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["chains"].is_array());
    }

    #[tokio::test]
    async fn data_flow_missing_params() {
        let server = setup_server();
        let result = server
            .codegraph_data_flow(Parameters(DataFlowParams {
                file_path: None,
                source: None,
                language: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["error"].is_string());
    }

    // -- codegraph_dead_stores --------------------------------------------

    #[tokio::test]
    async fn dead_stores_detection() {
        let server = setup_server();
        let result = server
            .codegraph_dead_stores(Parameters(DataFlowParams {
                file_path: None,
                source: Some("let x = 10;\nlet y = 20;\nconsole.log(y);".to_string()),
                language: Some("javascript".to_string()),
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["stores"].is_array());
        let stores = json["stores"].as_array().unwrap();
        assert!(stores
            .iter()
            .any(|s| s["variable"].as_str().unwrap() == "x"));
    }

    #[tokio::test]
    async fn dead_stores_missing_params() {
        let server = setup_server();
        let result = server
            .codegraph_dead_stores(Parameters(DataFlowParams {
                file_path: None,
                source: None,
                language: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["error"].is_string());
    }

    // -- codegraph_find_uninitialized -------------------------------------

    #[tokio::test]
    async fn find_uninitialized_vars() {
        let server = setup_server();
        let result = server
            .codegraph_find_uninitialized(Parameters(DataFlowParams {
                file_path: None,
                source: Some("console.log(result);\nlet result = compute();".to_string()),
                language: Some("javascript".to_string()),
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["locations"].is_array());
    }

    #[tokio::test]
    async fn find_uninitialized_missing_params() {
        let server = setup_server();
        let result = server
            .codegraph_find_uninitialized(Parameters(DataFlowParams {
                file_path: None,
                source: None,
                language: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["error"].is_string());
    }

    // -- codegraph_reaching_defs ------------------------------------------

    #[tokio::test]
    async fn reaching_defs_analysis() {
        let server = setup_server();
        let result = server
            .codegraph_reaching_defs(Parameters(ReachingDefsParams {
                file_path: None,
                source: Some("let x = 10;\nlet y = 20;\nlet z = x + y;".to_string()),
                language: Some("javascript".to_string()),
                target_line: 3,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["reachingDefinitions"].is_array());
    }

    #[tokio::test]
    async fn reaching_defs_missing_params() {
        let server = setup_server();
        let result = server
            .codegraph_reaching_defs(Parameters(ReachingDefsParams {
                file_path: None,
                source: None,
                language: None,
                target_line: 1,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["error"].is_string());
    }

    // -- codegraph_query --------------------------------------------------

    #[tokio::test]
    async fn query_with_results() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_node(&make_node(
                    "n1",
                    "searchable",
                    "src/a.ts",
                    NodeKind::Function,
                    1,
                    None,
                ))
                .unwrap();
        }
        let result = server
            .codegraph_query(Parameters(QueryParams {
                query: "searchable".to_string(),
                limit: Some(5),
                language: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json.is_array());
    }

    #[tokio::test]
    async fn query_empty_results() {
        let server = setup_server();
        let result = server
            .codegraph_query(Parameters(QueryParams {
                query: "nonexistent".to_string(),
                limit: None,
                language: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json.is_array());
        assert!(json.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn query_with_language_filter() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_node(&make_node(
                    "n1",
                    "compute",
                    "src/a.ts",
                    NodeKind::Function,
                    1,
                    None,
                ))
                .unwrap();
        }
        let result = server
            .codegraph_query(Parameters(QueryParams {
                query: "compute".to_string(),
                limit: None,
                language: Some("python".to_string()),
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json.as_array().unwrap().is_empty(), "no Python nodes exist");
    }

    // -- codegraph_dependencies -------------------------------------------

    #[tokio::test]
    async fn dependencies_tool() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node("n1", "main", "src/main.ts", NodeKind::Function, 1, None),
                    make_node("n2", "dep1", "src/dep.ts", NodeKind::Function, 1, None),
                ])
                .unwrap();
            store
                .upsert_edge(&make_edge("n1", "n2", EdgeKind::Calls, "src/main.ts", 5))
                .unwrap();
        }
        let result = server
            .codegraph_dependencies(Parameters(SymbolDepthParams {
                symbol: "main".to_string(),
                max_depth: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["dependencyCount"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn dependencies_not_found() {
        let server = setup_server();
        let result = server
            .codegraph_dependencies(Parameters(SymbolDepthParams {
                symbol: "nonexistent".to_string(),
                max_depth: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["error"].is_string());
    }

    // -- codegraph_callers ------------------------------------------------

    #[tokio::test]
    async fn callers_tool() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node("n1", "helper", "src/helper.ts", NodeKind::Function, 1, None),
                    make_node("n2", "caller", "src/main.ts", NodeKind::Function, 1, None),
                ])
                .unwrap();
            store
                .upsert_edge(&make_edge("n2", "n1", EdgeKind::Calls, "src/main.ts", 5))
                .unwrap();
        }
        let result = server
            .codegraph_callers(Parameters(SymbolDepthDetailParams {
                symbol: "helper".to_string(),
                max_depth: None,
                detail_level: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["callerCount"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn callers_not_found() {
        let server = setup_server();
        let result = server
            .codegraph_callers(Parameters(SymbolDepthDetailParams {
                symbol: "nonexistent".to_string(),
                max_depth: None,
                detail_level: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["error"].is_string());
    }

    // -- codegraph_impact -------------------------------------------------

    #[tokio::test]
    async fn impact_tool() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_nodes(&[
                    make_node("n1", "core", "src/core.ts", NodeKind::Function, 1, None),
                    make_node("n2", "user", "src/user.ts", NodeKind::Function, 1, None),
                ])
                .unwrap();
            store
                .upsert_edge(&make_edge("n2", "n1", EdgeKind::Calls, "src/user.ts", 5))
                .unwrap();
        }
        let result = server
            .codegraph_impact(Parameters(ImpactParams {
                file_path: None,
                symbol: Some("core".to_string()),
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["totalAffected"].is_number());
        assert!(json["riskGroups"].is_array());
    }

    #[tokio::test]
    async fn impact_not_found() {
        let server = setup_server();
        let result = server
            .codegraph_impact(Parameters(ImpactParams {
                file_path: None,
                symbol: Some("nonexistent".to_string()),
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["error"].is_string());
    }

    // -- resolve_symbol ---------------------------------------------------

    #[test]
    fn resolve_symbol_by_name() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_node(&make_node(
                    "n1",
                    "myFunc",
                    "src/a.ts",
                    NodeKind::Function,
                    1,
                    None,
                ))
                .unwrap();
        }
        let node = resolve_symbol(&server.store, "myFunc");
        assert!(node.is_some());
        assert_eq!(node.unwrap().name, "myFunc");
    }

    #[test]
    fn resolve_symbol_by_id() {
        let server = setup_server();
        {
            let store = server.store.lock().unwrap();
            store
                .upsert_node(&make_node(
                    "n1",
                    "myFunc",
                    "src/a.ts",
                    NodeKind::Function,
                    1,
                    None,
                ))
                .unwrap();
        }
        let node = resolve_symbol(&server.store, "n1");
        assert!(node.is_some());
        assert_eq!(node.unwrap().id, "n1");
    }

    #[test]
    fn resolve_symbol_not_found() {
        let server = setup_server();
        let node = resolve_symbol(&server.store, "nonexistent");
        assert!(node.is_none());
    }

    // -- helper function tests --------------------------------------------

    #[test]
    fn mermaid_safe_escapes_special() {
        let result = mermaid_safe("foo[bar](baz){qux}");
        assert!(!result.contains('['));
        assert!(!result.contains(']'));
        assert!(!result.contains('('));
        assert!(!result.contains(')'));
    }

    #[test]
    fn mermaid_id_deterministic() {
        let id1 = mermaid_id("node:test:1");
        let id2 = mermaid_id("node:test:1");
        assert_eq!(id1, id2);
    }

    #[test]
    fn mermaid_id_different_inputs() {
        let id1 = mermaid_id("alpha");
        let id2 = mermaid_id("beta");
        assert_ne!(id1, id2);
    }

    #[test]
    fn json_text_helper() {
        let val = serde_json::json!({"key": "value"});
        let result = json_text(&val);
        assert!(result.contains("key"));
        assert!(result.contains("value"));
    }

    #[tokio::test]
    async fn query_results_include_context_annotation() {
        let conn = initialize_database(":memory:").expect("schema init");
        let store = GraphStore::from_connection(conn);
        let mut config = CodeGraphConfig::default();
        config
            .contexts
            .insert("src/legacy".to_string(), "Deprecated v1 API".to_string());
        let server = CodeGraphServer::with_config(store, PathBuf::from("."), config);
        {
            let s = server.store.lock().unwrap();
            s.upsert_node(&make_node(
                "n1",
                "old_handler",
                "src/legacy/handler.ts",
                NodeKind::Function,
                1,
                None,
            ))
            .unwrap();
        }
        let result = server
            .codegraph_query(Parameters(QueryParams {
                query: "old_handler".to_string(),
                limit: Some(5),
                language: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = json.as_array().expect("results should be array");
        assert!(!arr.is_empty(), "should find old_handler");
        let first = &arr[0];
        assert_eq!(
            first["context"].as_str(),
            Some("Deprecated v1 API"),
            "context annotation should be present for legacy path"
        );
    }

    #[tokio::test]
    async fn query_results_no_context_when_path_unmatched() {
        let conn = initialize_database(":memory:").expect("schema init");
        let store = GraphStore::from_connection(conn);
        let mut config = CodeGraphConfig::default();
        config
            .contexts
            .insert("src/legacy".to_string(), "Deprecated v1 API".to_string());
        let server = CodeGraphServer::with_config(store, PathBuf::from("."), config);
        {
            let s = server.store.lock().unwrap();
            s.upsert_node(&make_node(
                "n2",
                "new_handler",
                "src/api/handler.ts",
                NodeKind::Function,
                1,
                None,
            ))
            .unwrap();
        }
        let result = server
            .codegraph_query(Parameters(QueryParams {
                query: "new_handler".to_string(),
                limit: Some(5),
                language: None,
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        let arr = json.as_array().expect("results should be array");
        assert!(!arr.is_empty(), "should find new_handler");
        let first = &arr[0];
        assert!(
            first.get("context").is_none(),
            "no context annotation for non-legacy path"
        );
    }

    // =====================================================================
    // Codex JSON Schema Compatibility Audit
    // =====================================================================

    fn audit_schema_value(path: &str, value: &serde_json::Value) -> Vec<String> {
        let mut issues = Vec::new();
        let Some(obj) = value.as_object() else {
            return issues;
        };
        if obj.contains_key("$ref") {
            issues.push(format!("{path}: contains $ref reference"));
        }
        if let Some(ap) = obj.get("additionalProperties") {
            if ap.is_object() {
                issues.push(format!(
                    "{path}: additionalProperties is an object (must be absent or false)"
                ));
            }
        }
        for keyword in &["oneOf", "anyOf"] {
            if let Some(variants) = obj.get(*keyword).and_then(|v| v.as_array()) {
                let complex = variants
                    .iter()
                    .filter(|v| {
                        v.as_object()
                            .map(|o| {
                                o.contains_key("properties")
                                    || o.contains_key("$ref")
                                    || o.contains_key("oneOf")
                                    || o.contains_key("anyOf")
                            })
                            .unwrap_or(false)
                    })
                    .count();
                if complex > 0 {
                    issues.push(format!(
                        "{path}: {keyword} contains complex object variants"
                    ));
                }
            }
        }
        if let Some(props) = obj.get("properties").and_then(|v| v.as_object()) {
            for (key, prop_value) in props {
                issues.extend(audit_schema_value(&format!("{path}.{key}"), prop_value));
            }
        }
        if let Some(items) = obj.get("items") {
            issues.extend(audit_schema_value(&format!("{path}[items]"), items));
        }
        issues
    }

    #[test]
    fn codex_schema_compatibility_audit() {
        let tools = CodeGraphServer::tool_router().list_all();
        assert!(
            tools.len() >= 46,
            "Expected >=46 tools, got {}",
            tools.len()
        );

        let mut all_issues: Vec<String> = Vec::new();
        for tool in &tools {
            let schema = &tool.input_schema;
            let schema_value = tool.schema_as_json_value();
            if schema.get("type").and_then(|v| v.as_str()) != Some("object") {
                all_issues.push(format!("{}: top-level type is not 'object'", tool.name));
            }
            if let Some(ap) = schema.get("additionalProperties") {
                if !ap.is_boolean() || ap.as_bool() == Some(true) {
                    all_issues.push(format!("{}: additionalProperties is not false", tool.name));
                }
            }
            all_issues.extend(audit_schema_value(&tool.name, &schema_value));
        }

        if !all_issues.is_empty() {
            panic!(
                "Codex schema compatibility issues:\n{}",
                all_issues.join("\n")
            );
        }
    }

    #[test]
    fn all_tool_schemas_have_consistent_required_fields() {
        let tools = CodeGraphServer::tool_router().list_all();
        for tool in &tools {
            let schema = &tool.input_schema;
            if let Some(props) = schema.get("properties").and_then(|v| v.as_object()) {
                if let Some(required) = schema.get("required").and_then(|v| v.as_array()) {
                    for req in required {
                        let name = req.as_str().unwrap_or("");
                        assert!(
                            props.contains_key(name),
                            "{}: required '{}' not in properties",
                            tool.name,
                            name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn tool_schemas_no_definitions_block() {
        let tools = CodeGraphServer::tool_router().list_all();
        for tool in &tools {
            let schema = &tool.input_schema;
            assert!(
                !schema.contains_key("definitions"),
                "{}: contains 'definitions' block",
                tool.name
            );
            assert!(
                !schema.contains_key("$defs"),
                "{}: contains '$defs' block",
                tool.name
            );
        }
    }

    #[test]
    fn tool_count_is_at_least_46() {
        let tools = CodeGraphServer::tool_router().list_all();
        assert!(
            tools.len() >= 46,
            "Expected >=46 tools, got {}: {:?}",
            tools.len(),
            tools.iter().map(|t| t.name.as_ref()).collect::<Vec<_>>()
        );
    }
}

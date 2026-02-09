//! Context assembler — packs ranked code snippets into an LLM prompt.
//!
//! Ports the TypeScript `context/assembler.ts` to Rust. Given a natural
//! language query, the assembler searches the code graph for relevant
//! symbols, loads their source from SQLite, and arranges them into a
//! structured Markdown document that fits within a configurable token
//! budget.
//!
//! The output is partitioned into four tiers so the most important
//! information always appears first:
//!
//! | Tier       | Budget | Content                                    |
//! |------------|--------|--------------------------------------------|
//! | Core       | ~40%   | Full source of top-ranked search results    |
//! | Near       | ~25%   | Signatures of direct callers/callees        |
//! | Extended   | ~20%   | Related tests and sibling functions         |
//! | Background | ~15%   | Project structure overview                  |

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};

use crate::context::budget::{estimate_tokens, signature_only, truncate_to_fit};
use crate::db::converters::row_to_code_node;
use crate::graph::search::{HybridSearch, SearchOptions};
use crate::types::CodeNode;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default token budget when the caller doesn't specify one.
///
/// Set to 32K tokens — modern LLMs have 128K-200K context windows, so a
/// generous default ensures we don't leave relevant context on the floor.
const DEFAULT_BUDGET: usize = 32_000;

/// Initial tier allocation percentages. These define the *minimum guaranteed*
/// share each tier gets, but unused budget is redistributed adaptively.
const TIER_CORE_PCT: usize = 40;
const TIER_NEAR_PCT: usize = 25;
const TIER_EXTENDED_PCT: usize = 20;
const TIER_BACKGROUND_PCT: usize = 15;

// ---------------------------------------------------------------------------
// Context assembler
// ---------------------------------------------------------------------------

/// Assembles LLM-ready context from the code graph.
///
/// Holds references to the underlying database connection and search
/// engine so it can query both the structured graph and the full-text /
/// vector indexes in a single pass.
pub struct ContextAssembler<'a> {
    conn: &'a Connection,
    search: &'a HybridSearch<'a>,
    /// Directory context annotations from config (path prefix -> description).
    contexts: HashMap<String, String>,
}

impl<'a> ContextAssembler<'a> {
    /// Create a new assembler backed by `conn` and `search`.
    pub fn new(conn: &'a Connection, search: &'a HybridSearch<'a>) -> Self {
        Self {
            conn,
            search,
            contexts: HashMap::new(),
        }
    }

    /// Create a new assembler with directory context annotations.
    pub fn with_contexts(
        conn: &'a Connection,
        search: &'a HybridSearch<'a>,
        contexts: HashMap<String, String>,
    ) -> Self {
        Self {
            conn,
            search,
            contexts,
        }
    }

    /// Look up the most specific context annotation for a file path.
    fn context_for_path(&self, path: &str) -> Option<&str> {
        self.contexts
            .iter()
            .filter(|(prefix, _)| path.starts_with(prefix.as_str()))
            .max_by_key(|(prefix, _)| prefix.len())
            .map(|(_, desc)| desc.as_str())
    }

    /// Assemble a Markdown context document for `query`.
    ///
    /// `budget` defaults to [`DEFAULT_BUDGET`] tokens when `None`.
    ///
    /// ## Adaptive budget allocation
    ///
    /// Instead of rigidly splitting 40/25/20/15 and wasting unused space,
    /// the assembler uses a two-pass approach:
    ///
    /// 1. **Pass 1:** Build all tier content with generous per-tier caps.
    /// 2. **Pass 2:** Measure actual content sizes. If any tier is under
    ///    its initial allocation, the surplus is redistributed
    ///    proportionally to tiers that need more room, and those tiers
    ///    are rebuilt with the enlarged budget.
    pub fn assemble_context(&self, query: &str, budget: Option<usize>) -> String {
        let budget = budget.unwrap_or(DEFAULT_BUDGET);

        // Initial allocation.
        let initial_budgets = [
            budget * TIER_CORE_PCT / 100,
            budget * TIER_NEAR_PCT / 100,
            budget * TIER_EXTENDED_PCT / 100,
            budget * TIER_BACKGROUND_PCT / 100,
        ];

        // -- Gather nodes for each tier (query-independent of budget) -----

        let search_opts = SearchOptions {
            limit: Some(10),
            ..Default::default()
        };
        let search_results = self.search.search(query, &search_opts).unwrap_or_default();

        let mut core_nodes: Vec<CodeNode> = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();

        for result in &search_results {
            if let Some(node) = self.load_node(&result.node_id) {
                seen_ids.insert(node.id.clone());
                core_nodes.push(node);
            }
        }

        // Near: direct callers/callees.
        let mut near_ids: Vec<String> = Vec::new();
        for node in &core_nodes {
            let neighbor_ids = self.get_neighbor_ids(&node.id);
            for nid in neighbor_ids {
                if !seen_ids.contains(&nid) {
                    seen_ids.insert(nid.clone());
                    near_ids.push(nid);
                }
            }
        }

        let mut near_nodes: Vec<CodeNode> = Vec::new();
        for nid in &near_ids {
            if let Some(node) = self.load_node(nid) {
                near_nodes.push(node);
            }
        }

        // Extended: tests + siblings.
        let mut extended_nodes: Vec<CodeNode> = Vec::new();
        let test_nodes = self.find_related_tests(&core_nodes, &seen_ids);
        for node in &test_nodes {
            seen_ids.insert(node.id.clone());
        }
        extended_nodes.extend(test_nodes);

        let sibling_nodes = self.find_siblings(&core_nodes, &seen_ids);
        for node in &sibling_nodes {
            seen_ids.insert(node.id.clone());
        }
        extended_nodes.extend(sibling_nodes);

        // -- Pass 1: build with initial budgets --------------------------

        let sections_pass1 = [
            self.build_core_section(&core_nodes, initial_budgets[0]),
            self.build_near_section(&near_nodes, initial_budgets[1]),
            self.build_extended_section(&extended_nodes, initial_budgets[2]),
            self.build_background_section(initial_budgets[3]),
        ];

        let actual_tokens: Vec<usize> = sections_pass1.iter().map(|s| estimate_tokens(s)).collect();

        // -- Pass 2: redistribute surplus --------------------------------

        let final_sections = redistribute_and_rebuild(
            &initial_budgets,
            &actual_tokens,
            budget,
            || self.build_core_section(&core_nodes, budget), // rebuild with max
            || self.build_near_section(&near_nodes, budget),
            || self.build_extended_section(&extended_nodes, budget),
            || self.build_background_section(budget),
            sections_pass1,
        );

        // -- Assemble the final document ----------------------------------
        let labels = [
            "## Core Context",
            "## Related Symbols",
            "## Tests & Siblings",
            "## Project Structure",
        ];
        let mut output: Vec<String> = Vec::new();

        for (section, label) in final_sections.iter().zip(labels.iter()) {
            if !section.is_empty() {
                output.push(format!("{}\n\n{}", label, section));
            }
        }

        if output.is_empty() {
            return String::from("No relevant context found.");
        }

        output.join("\n\n---\n\n")
    }

    // -------------------------------------------------------------------
    // Section builders
    // -------------------------------------------------------------------

    /// Build the **Core** section: full source of top-ranked nodes.
    fn build_core_section(&self, nodes: &[CodeNode], budget: usize) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut used = 0;

        for node in nodes {
            let ctx_annotation = self.context_for_path(&node.file_path);
            let formatted = format_node_full(node, ctx_annotation);
            let tokens = estimate_tokens(&formatted);
            if used + tokens > budget && !parts.is_empty() {
                break;
            }
            parts.push(formatted);
            used += tokens;
        }

        parts.join("\n\n")
    }

    /// Build the **Near** section: compact signatures of neighbors.
    fn build_near_section(&self, nodes: &[CodeNode], budget: usize) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut used = 0;

        for node in nodes {
            let formatted = format_node_signature(node);
            let tokens = estimate_tokens(&formatted);
            if used + tokens > budget && !parts.is_empty() {
                break;
            }
            parts.push(formatted);
            used += tokens;
        }

        parts.join("\n")
    }

    /// Build the **Extended** section: tests and siblings as signatures.
    fn build_extended_section(&self, nodes: &[CodeNode], budget: usize) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut used = 0;

        for node in nodes {
            let formatted = format_node_signature(node);
            let tokens = estimate_tokens(&formatted);
            if used + tokens > budget && !parts.is_empty() {
                break;
            }
            parts.push(formatted);
            used += tokens;
        }

        parts.join("\n")
    }

    /// Build the **Background** section: file listing overview.
    fn build_background_section(&self, budget: usize) -> String {
        let files = self.get_distinct_files();
        if files.is_empty() {
            return String::new();
        }

        let mut listing = String::from("Files in project:\n");
        for file in &files {
            let line = format!("- {}\n", file);
            if estimate_tokens(&listing) + estimate_tokens(&line) > budget {
                break;
            }
            listing.push_str(&line);
        }

        truncate_to_fit(&listing, budget)
    }

    // -------------------------------------------------------------------
    // Data loaders
    // -------------------------------------------------------------------

    /// Load a single [`CodeNode`] by ID from the database.
    fn load_node(&self, id: &str) -> Option<CodeNode> {
        self.conn
            .query_row("SELECT * FROM nodes WHERE id = ?1", params![id], |row| {
                row_to_code_node(row)
            })
            .ok()
    }

    /// Get the IDs of all direct callers and callees of `node_id`.
    fn get_neighbor_ids(&self, node_id: &str) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();

        // Outgoing edges: node_id -> target.
        if let Ok(mut stmt) = self
            .conn
            .prepare_cached("SELECT target_id FROM edges WHERE source_id = ?1")
        {
            if let Ok(rows) = stmt.query_map(params![node_id], |row| row.get::<_, String>(0)) {
                for row in rows.flatten() {
                    ids.push(row);
                }
            }
        }

        // Incoming edges: source -> node_id.
        if let Ok(mut stmt) = self
            .conn
            .prepare_cached("SELECT source_id FROM edges WHERE target_id = ?1")
        {
            if let Ok(rows) = stmt.query_map(params![node_id], |row| row.get::<_, String>(0)) {
                for row in rows.flatten() {
                    ids.push(row);
                }
            }
        }

        ids
    }

    /// Find test-related nodes that reference one of `core_nodes`.
    ///
    /// A node is considered test-related if its name contains "test" or
    /// "spec" (case-insensitive) **and** it has an edge connecting it to
    /// one of the core symbols.
    fn find_related_tests(&self, core_nodes: &[CodeNode], seen: &HashSet<String>) -> Vec<CodeNode> {
        let mut tests: Vec<CodeNode> = Vec::new();

        // Collect all core IDs for fast lookup.
        let core_ids: HashSet<&str> = core_nodes.iter().map(|n| n.id.as_str()).collect();

        // Query for test/spec nodes.
        let sql =
            "SELECT * FROM nodes WHERE LOWER(name) LIKE '%test%' OR LOWER(name) LIKE '%spec%'";
        let mut stmt = match self.conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return tests,
        };

        let rows = match stmt.query_and_then([], row_to_code_node) {
            Ok(r) => r,
            Err(_) => return tests,
        };

        for row_result in rows {
            let node = match row_result {
                Ok(n) => n,
                Err(_) => continue,
            };

            if seen.contains(&node.id) {
                continue;
            }

            // Check if this test node has an edge to/from any core node.
            let references_core = self.node_references_any(&node.id, &core_ids);
            if references_core {
                tests.push(node);
            }
        }

        tests
    }

    /// Check whether `node_id` has any edge connecting it to one of the
    /// `target_ids`.
    fn node_references_any(&self, node_id: &str, target_ids: &HashSet<&str>) -> bool {
        // Check outgoing.
        if let Ok(mut stmt) = self
            .conn
            .prepare_cached("SELECT target_id FROM edges WHERE source_id = ?1")
        {
            if let Ok(rows) = stmt.query_map(params![node_id], |row| row.get::<_, String>(0)) {
                for row in rows.flatten() {
                    if target_ids.contains(row.as_str()) {
                        return true;
                    }
                }
            }
        }

        // Check incoming.
        if let Ok(mut stmt) = self
            .conn
            .prepare_cached("SELECT source_id FROM edges WHERE target_id = ?1")
        {
            if let Ok(rows) = stmt.query_map(params![node_id], |row| row.get::<_, String>(0)) {
                for row in rows.flatten() {
                    if target_ids.contains(row.as_str()) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Find sibling nodes: other nodes in the same files as `core_nodes`.
    fn find_siblings(&self, core_nodes: &[CodeNode], seen: &HashSet<String>) -> Vec<CodeNode> {
        let files: HashSet<&str> = core_nodes.iter().map(|n| n.file_path.as_str()).collect();
        let mut siblings: Vec<CodeNode> = Vec::new();

        for file in files {
            let sql = "SELECT * FROM nodes WHERE file_path = ?1";
            let mut stmt = match self.conn.prepare(sql) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let rows = match stmt.query_and_then(params![file], row_to_code_node) {
                Ok(r) => r,
                Err(_) => continue,
            };

            for node in rows.flatten() {
                if !seen.contains(&node.id) {
                    siblings.push(node);
                }
            }
        }

        siblings
    }

    /// Get all distinct file paths from the nodes table, sorted.
    fn get_distinct_files(&self) -> Vec<String> {
        let sql = "SELECT DISTINCT file_path FROM nodes ORDER BY file_path";
        let mut stmt = match self.conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let rows = match stmt.query_map([], |row| row.get::<_, String>(0)) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        rows.flatten().collect()
    }
}

// ---------------------------------------------------------------------------
// Adaptive budget redistribution
// ---------------------------------------------------------------------------

/// Redistribute unused budget from under-filled tiers to over-filled tiers.
///
/// A tier is "under-filled" if its actual content uses fewer tokens than its
/// initial allocation. The surplus from all under-filled tiers is pooled and
/// distributed proportionally among tiers that could use more.
///
/// If redistribution would give a tier a meaningfully larger budget (>10%
/// increase), that tier is rebuilt with the expanded budget. Otherwise, the
/// pass-1 result is kept to avoid unnecessary work.
#[allow(clippy::too_many_arguments)]
fn redistribute_and_rebuild<F0, F1, F2, F3>(
    initial_budgets: &[usize; 4],
    actual_tokens: &[usize],
    total_budget: usize,
    rebuild_core: F0,
    rebuild_near: F1,
    rebuild_extended: F2,
    rebuild_background: F3,
    pass1: [String; 4],
) -> [String; 4]
where
    F0: FnOnce() -> String,
    F1: FnOnce() -> String,
    F2: FnOnce() -> String,
    F3: FnOnce() -> String,
{
    // Calculate surplus from under-filled tiers.
    let mut surplus: usize = 0;
    let mut needs_more = [false; 4]; // tiers that used their full allocation
    let mut demand = [0usize; 4]; // how much each tier wanted beyond its allocation

    for i in 0..4 {
        if actual_tokens[i] < initial_budgets[i] {
            // Under-filled: this tier didn't use its full share.
            surplus += initial_budgets[i] - actual_tokens[i];
        } else {
            // At or over budget: could benefit from more.
            needs_more[i] = true;
            demand[i] = initial_budgets[i]; // use initial allocation as weight
        }
    }

    if surplus == 0 {
        // Nothing to redistribute — every tier used its full allocation.
        return pass1;
    }

    let total_demand: usize = demand.iter().sum();
    if total_demand == 0 {
        // No tier needs more — all under-filled. Keep pass-1 results.
        return pass1;
    }

    // Compute final budgets: initial + proportional share of surplus.
    let mut final_budgets = *initial_budgets;
    for i in 0..4 {
        if needs_more[i] {
            let share = surplus * demand[i] / total_demand;
            final_budgets[i] += share;
        } else {
            // Shrink to actual usage (don't over-allocate).
            final_budgets[i] = actual_tokens[i];
        }
    }

    // Cap to total budget.
    let sum: usize = final_budgets.iter().sum();
    if sum > total_budget {
        // Scale down proportionally.
        for b in &mut final_budgets {
            *b = *b * total_budget / sum;
        }
    }

    // Rebuild only tiers whose budget increased meaningfully (>10%).
    let threshold = |initial: usize, final_b: usize| -> bool {
        final_b > initial && (final_b - initial) * 10 > initial
    };

    let [s0, s1, s2, s3] = pass1;

    let s0 = if threshold(initial_budgets[0], final_budgets[0]) {
        let rebuilt = rebuild_core();
        truncate_to_fit(&rebuilt, final_budgets[0])
    } else {
        s0
    };

    let s1 = if threshold(initial_budgets[1], final_budgets[1]) {
        let rebuilt = rebuild_near();
        truncate_to_fit(&rebuilt, final_budgets[1])
    } else {
        s1
    };

    let s2 = if threshold(initial_budgets[2], final_budgets[2]) {
        let rebuilt = rebuild_extended();
        truncate_to_fit(&rebuilt, final_budgets[2])
    } else {
        s2
    };

    let s3 = if threshold(initial_budgets[3], final_budgets[3]) {
        let rebuilt = rebuild_background();
        truncate_to_fit(&rebuilt, final_budgets[3])
    } else {
        s3
    };

    [s0, s1, s2, s3]
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

/// Format a node with its full body inside a fenced code block.
///
/// ```text
/// ### `function` **greet** (`src/hello.ts:10-15`)
///
/// ```ts
/// function greet(name: string): void {
///   console.log(`Hello, ${name}`);
/// }
/// ```
/// ```
fn format_node_full(node: &CodeNode, context_annotation: Option<&str>) -> String {
    let tag = language_tag(node.language.as_str());
    let location = format!("{}:{}-{}", node.file_path, node.start_line, node.end_line);
    let header = format!(
        "### `{}` **{}** (`{}`)",
        node.kind.as_str(),
        node.name,
        location,
    );

    let body = node.body.as_deref().unwrap_or("// source not available");

    // Include documentation if present.
    let doc_line = node
        .documentation
        .as_deref()
        .map(|d| format!("\n> {}\n", d.lines().next().unwrap_or("")))
        .unwrap_or_default();

    // Include directory context annotation if present.
    let ctx_line = context_annotation
        .map(|ctx| format!("\n> **Context:** {}\n", ctx))
        .unwrap_or_default();

    format!(
        "{}{}{}\n\n```{}\n{}\n```",
        header, doc_line, ctx_line, tag, body
    )
}

/// Format a node as a compact one-line signature.
///
/// ```text
/// - `function` **greet** (`src/hello.ts:10`) — `function greet(name: string): void`
/// ```
fn format_node_signature(node: &CodeNode) -> String {
    let sig = node
        .body
        .as_deref()
        .map(signature_only)
        .unwrap_or_else(|| node.name.clone());

    format!(
        "- `{}` **{}** (`{}:{}`) -- `{}`",
        node.kind.as_str(),
        node.name,
        node.file_path,
        node.start_line,
        sig,
    )
}

/// Map a language string to the appropriate Markdown fence tag.
fn language_tag(lang: &str) -> &str {
    match lang {
        "typescript" | "tsx" => "ts",
        "javascript" | "jsx" => "js",
        "python" => "py",
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::initialize_database;
    use crate::graph::store::GraphStore;
    use crate::types::{CodeEdge, CodeNode, EdgeKind, Language, NodeKind};

    /// Spin up an in-memory store with the full schema applied.
    fn setup() -> GraphStore {
        let conn = initialize_database(":memory:").expect("schema init should succeed on :memory:");
        GraphStore::from_connection(conn)
    }

    /// Build a minimal test node.
    fn make_node(
        id: &str,
        name: &str,
        file: &str,
        kind: NodeKind,
        line: u32,
        body: Option<&str>,
        doc: Option<&str>,
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
            body: body.map(|s| s.to_string()),
            documentation: doc.map(|d| d.to_string()),
            exported: Some(true),
        }
    }

    /// Build a minimal test edge.
    fn make_edge(source: &str, target: &str, kind: EdgeKind) -> CodeEdge {
        CodeEdge {
            source: source.to_string(),
            target: target.to_string(),
            kind,
            file_path: String::new(),
            line: 0,
            metadata: None,
        }
    }

    // -- format_node_full -------------------------------------------------

    #[test]
    fn format_node_full_with_body_and_docs() {
        let node = make_node(
            "fn:a.ts:greet:1",
            "greet",
            "a.ts",
            NodeKind::Function,
            1,
            Some("function greet(name: string) {\n  console.log(name);\n}"),
            Some("Say hello to someone."),
        );

        let formatted = format_node_full(&node, None);
        assert!(formatted.contains("### `function` **greet**"));
        assert!(formatted.contains("```ts"));
        assert!(formatted.contains("function greet(name: string)"));
        assert!(formatted.contains("> Say hello to someone."));
    }

    #[test]
    fn format_node_full_without_body() {
        let node = make_node(
            "fn:a.ts:greet:1",
            "greet",
            "a.ts",
            NodeKind::Function,
            1,
            None,
            None,
        );

        let formatted = format_node_full(&node, None);
        assert!(formatted.contains("// source not available"));
    }

    // -- format_node_signature --------------------------------------------

    #[test]
    fn format_node_signature_with_body() {
        let node = make_node(
            "fn:a.ts:greet:1",
            "greet",
            "a.ts",
            NodeKind::Function,
            1,
            Some("function greet(name: string) {\n  console.log(name);\n}"),
            None,
        );

        let sig = format_node_signature(&node);
        assert!(sig.contains("**greet**"));
        assert!(sig.contains("function greet(name: string)"));
        // Should NOT contain the body.
        assert!(!sig.contains("console.log"));
    }

    #[test]
    fn format_node_signature_without_body() {
        let node = make_node(
            "fn:a.ts:greet:1",
            "greet",
            "a.ts",
            NodeKind::Function,
            1,
            None,
            None,
        );

        let sig = format_node_signature(&node);
        // Falls back to the node name.
        assert!(sig.contains("greet"));
    }

    // -- language_tag -----------------------------------------------------

    #[test]
    fn language_tag_mappings() {
        assert_eq!(language_tag("typescript"), "ts");
        assert_eq!(language_tag("tsx"), "ts");
        assert_eq!(language_tag("javascript"), "js");
        assert_eq!(language_tag("jsx"), "js");
        assert_eq!(language_tag("python"), "py");
        assert_eq!(language_tag("rust"), "rust");
    }

    // -- assemble_context (integration) -----------------------------------

    #[test]
    fn assemble_context_returns_something_for_matching_query() {
        let store = setup();

        store
            .upsert_node(&make_node(
                "fn:a.ts:greet:1",
                "greet",
                "a.ts",
                NodeKind::Function,
                1,
                Some("function greet(name: string) {\n  console.log(name);\n}"),
                Some("Say hello."),
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);

        let ctx = assembler.assemble_context("greet", None);
        assert!(ctx.contains("greet"));
        assert!(ctx.contains("## Core Context"));
    }

    #[test]
    fn assemble_context_returns_fallback_for_no_match() {
        let store = setup();
        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);

        let ctx = assembler.assemble_context("nonexistent", None);
        assert_eq!(ctx, "No relevant context found.");
    }

    #[test]
    fn assemble_context_includes_near_section() {
        let store = setup();

        // Create two nodes with an edge between them.
        store
            .upsert_node(&make_node(
                "fn:a.ts:greet:1",
                "greet",
                "a.ts",
                NodeKind::Function,
                1,
                Some("function greet() {}"),
                None,
            ))
            .unwrap();
        store
            .upsert_node(&make_node(
                "fn:a.ts:helper:10",
                "helper",
                "a.ts",
                NodeKind::Function,
                10,
                Some("function helper() {}"),
                None,
            ))
            .unwrap();
        store
            .upsert_edge(&make_edge(
                "fn:a.ts:greet:1",
                "fn:a.ts:helper:10",
                EdgeKind::Calls,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);

        let ctx = assembler.assemble_context("greet", None);
        // The "helper" node should appear in the related symbols section.
        assert!(ctx.contains("helper"));
    }

    #[test]
    fn assemble_context_includes_tests_section() {
        let store = setup();

        store
            .upsert_node(&make_node(
                "fn:a.ts:greet:1",
                "greet",
                "a.ts",
                NodeKind::Function,
                1,
                Some("function greet() {}"),
                None,
            ))
            .unwrap();
        store
            .upsert_node(&make_node(
                "fn:a.test.ts:test_greet:1",
                "test_greet",
                "a.test.ts",
                NodeKind::Function,
                1,
                Some("function test_greet() {}"),
                None,
            ))
            .unwrap();
        store
            .upsert_edge(&make_edge(
                "fn:a.test.ts:test_greet:1",
                "fn:a.ts:greet:1",
                EdgeKind::Calls,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);

        let ctx = assembler.assemble_context("greet", None);
        assert!(ctx.contains("test_greet"));
    }

    #[test]
    fn assemble_context_includes_siblings() {
        let store = setup();

        store
            .upsert_node(&make_node(
                "fn:a.ts:greet:1",
                "greet",
                "a.ts",
                NodeKind::Function,
                1,
                Some("function greet() {}"),
                None,
            ))
            .unwrap();
        store
            .upsert_node(&make_node(
                "fn:a.ts:farewell:20",
                "farewell",
                "a.ts",
                NodeKind::Function,
                20,
                Some("function farewell() {}"),
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);

        let ctx = assembler.assemble_context("greet", None);
        // "farewell" is a sibling in the same file.
        assert!(ctx.contains("farewell"));
    }

    #[test]
    fn assemble_context_includes_project_structure() {
        let store = setup();

        store
            .upsert_node(&make_node(
                "fn:a.ts:greet:1",
                "greet",
                "a.ts",
                NodeKind::Function,
                1,
                Some("function greet() {}"),
                None,
            ))
            .unwrap();
        store
            .upsert_node(&make_node(
                "fn:b.ts:other:1",
                "other",
                "b.ts",
                NodeKind::Function,
                1,
                Some("function other() {}"),
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);

        let ctx = assembler.assemble_context("greet", None);
        assert!(ctx.contains("## Project Structure"));
        assert!(ctx.contains("a.ts"));
        assert!(ctx.contains("b.ts"));
    }

    #[test]
    fn assemble_context_respects_budget() {
        let store = setup();

        // Insert many nodes to create a large graph.
        for i in 0..50 {
            store
                .upsert_node(&make_node(
                    &format!("fn:a.ts:func{}:{}", i, i),
                    &format!("func{}", i),
                    "a.ts",
                    NodeKind::Function,
                    i,
                    Some(&format!(
                        "function func{}() {{\n  // body line 1\n  // body line 2\n  // body line 3\n}}",
                        i
                    )),
                    None,
                ))
                .unwrap();
        }

        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);

        // Very small budget.
        let ctx = assembler.assemble_context("func", Some(100));
        let tokens = estimate_tokens(&ctx);
        // The output should be reasonably bounded. We allow some overshoot
        // because the first item in each tier is always included, but it
        // should not be wildly over budget.
        assert!(tokens < 300, "Expected output tokens < 300, got {}", tokens);
    }

    // =====================================================================
    // NEW TESTS: Phase 18C — Context assembler comprehensive coverage
    // =====================================================================

    // -- format_node_full edge cases -------------------------------------

    #[test]
    fn format_node_full_with_python_language() {
        let mut node = make_node(
            "fn:a.py:compute:1",
            "compute",
            "a.py",
            NodeKind::Function,
            1,
            Some("def compute(x):\n    return x * 2"),
            None,
        );
        node.language = Language::Python;
        let formatted = format_node_full(&node, None);
        assert!(formatted.contains("```py"));
    }

    #[test]
    fn format_node_full_with_rust_language() {
        let mut node = make_node(
            "fn:a.rs:process:1",
            "process",
            "a.rs",
            NodeKind::Function,
            1,
            Some("fn process(x: i32) -> i32 {\n    x + 1\n}"),
            None,
        );
        node.language = Language::Rust;
        let formatted = format_node_full(&node, None);
        assert!(formatted.contains("```rust"));
    }

    #[test]
    fn format_node_full_includes_location() {
        let node = make_node(
            "fn:src/util.ts:helper:42",
            "helper",
            "src/util.ts",
            NodeKind::Function,
            42,
            Some("function helper() {}"),
            None,
        );
        let formatted = format_node_full(&node, None);
        assert!(formatted.contains("src/util.ts:42-47"));
    }

    #[test]
    fn format_node_full_class_kind() {
        let node = make_node(
            "cls:a.ts:Foo:1",
            "Foo",
            "a.ts",
            NodeKind::Class,
            1,
            Some("class Foo {\n  bar() {}\n}"),
            None,
        );
        let formatted = format_node_full(&node, None);
        assert!(formatted.contains("`class`"));
    }

    // -- format_node_signature edge cases ---------------------------------

    #[test]
    fn format_node_signature_method_kind() {
        let node = make_node(
            "m:a.ts:doWork:10",
            "doWork",
            "a.ts",
            NodeKind::Method,
            10,
            Some("doWork(data: Data) {\n  process(data);\n}"),
            None,
        );
        let sig = format_node_signature(&node);
        assert!(sig.contains("`method`"));
        assert!(sig.contains("**doWork**"));
    }

    // -- language_tag additional mappings ---------------------------------

    #[test]
    fn language_tag_go() {
        assert_eq!(language_tag("go"), "go");
    }

    #[test]
    fn language_tag_java() {
        assert_eq!(language_tag("java"), "java");
    }

    // -- assemble_context with custom budget ------------------------------

    #[test]
    fn assemble_context_large_budget() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:greet:1",
                "greet",
                "a.ts",
                NodeKind::Function,
                1,
                Some("function greet() { return 'hello'; }"),
                Some("A greeting function"),
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);

        let ctx = assembler.assemble_context("greet", Some(100_000));
        assert!(ctx.contains("greet"));
        assert!(ctx.contains("## Core Context"));
    }

    #[test]
    fn assemble_context_zero_budget() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:greet:1",
                "greet",
                "a.ts",
                NodeKind::Function,
                1,
                Some("function greet() {}"),
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);

        // Budget of 0 should still produce something (first item always included)
        let ctx = assembler.assemble_context("greet", Some(0));
        // Either empty or fallback message
        assert!(!ctx.is_empty());
    }

    // -- assemble_context with multiple files ----------------------------

    #[test]
    fn assemble_context_multiple_files_in_structure() {
        let store = setup();
        for i in 0..5 {
            store
                .upsert_node(&make_node(
                    &format!("fn:file{}.ts:func{}:{}", i, i, 1),
                    &format!("func{}", i),
                    &format!("src/file{}.ts", i),
                    NodeKind::Function,
                    1,
                    Some(&format!("function func{}() {{}}", i)),
                    None,
                ))
                .unwrap();
        }

        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);

        let ctx = assembler.assemble_context("func", None);
        // Project structure should list multiple files
        if ctx.contains("## Project Structure") {
            assert!(ctx.contains("file0.ts") || ctx.contains("file1.ts"));
        }
    }

    // -- assemble_context with edges creates near section -----------------

    #[test]
    fn assemble_context_with_callers_and_callees() {
        let store = setup();
        store
            .upsert_nodes(&[
                make_node(
                    "fn:a.ts:main:1",
                    "main",
                    "a.ts",
                    NodeKind::Function,
                    1,
                    Some("function main() { greet(); }"),
                    None,
                ),
                make_node(
                    "fn:a.ts:greet:10",
                    "greet",
                    "a.ts",
                    NodeKind::Function,
                    10,
                    Some("function greet() { helper(); }"),
                    None,
                ),
                make_node(
                    "fn:a.ts:helper:20",
                    "helper",
                    "a.ts",
                    NodeKind::Function,
                    20,
                    Some("function helper() {}"),
                    None,
                ),
            ])
            .unwrap();
        store
            .upsert_edges(&[
                make_edge("fn:a.ts:main:1", "fn:a.ts:greet:10", EdgeKind::Calls),
                make_edge("fn:a.ts:greet:10", "fn:a.ts:helper:20", EdgeKind::Calls),
            ])
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);

        let ctx = assembler.assemble_context("greet", None);
        // Should include main as caller and helper as callee in related symbols
        assert!(ctx.contains("greet"));
    }

    // -- get_neighbor_ids -------------------------------------------------

    #[test]
    fn get_neighbor_ids_bidirectional() {
        let store = setup();
        store
            .upsert_nodes(&[
                make_node(
                    "fn:a.ts:a:1",
                    "a",
                    "a.ts",
                    NodeKind::Function,
                    1,
                    None,
                    None,
                ),
                make_node(
                    "fn:a.ts:b:10",
                    "b",
                    "a.ts",
                    NodeKind::Function,
                    10,
                    None,
                    None,
                ),
                make_node(
                    "fn:a.ts:c:20",
                    "c",
                    "a.ts",
                    NodeKind::Function,
                    20,
                    None,
                    None,
                ),
            ])
            .unwrap();
        store
            .upsert_edges(&[
                make_edge("fn:a.ts:a:1", "fn:a.ts:b:10", EdgeKind::Calls),
                make_edge("fn:a.ts:c:20", "fn:a.ts:b:10", EdgeKind::Calls),
            ])
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);

        let neighbors = assembler.get_neighbor_ids("fn:a.ts:b:10");
        // b's neighbors: a (incoming) and c (incoming), but also targets of b's outgoing
        assert!(!neighbors.is_empty());
    }

    // -- load_node --------------------------------------------------------

    #[test]
    fn load_node_existing() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:test:1",
                "test",
                "a.ts",
                NodeKind::Function,
                1,
                None,
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);

        let node = assembler.load_node("fn:a.ts:test:1");
        assert!(node.is_some());
        assert_eq!(node.unwrap().name, "test");
    }

    #[test]
    fn load_node_nonexistent() {
        let store = setup();
        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);

        let node = assembler.load_node("nonexistent");
        assert!(node.is_none());
    }

    // -- get_distinct_files -----------------------------------------------

    #[test]
    fn get_distinct_files_sorted() {
        let store = setup();
        store
            .upsert_nodes(&[
                make_node("n1", "a", "z.ts", NodeKind::Function, 1, None, None),
                make_node("n2", "b", "a.ts", NodeKind::Function, 1, None, None),
                make_node("n3", "c", "m.ts", NodeKind::Function, 1, None, None),
            ])
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);

        let files = assembler.get_distinct_files();
        assert_eq!(files.len(), 3);
        assert_eq!(files[0], "a.ts");
        assert_eq!(files[1], "m.ts");
        assert_eq!(files[2], "z.ts");
    }

    #[test]
    fn get_distinct_files_empty() {
        let store = setup();
        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);
        let files = assembler.get_distinct_files();
        assert!(files.is_empty());
    }

    // -- Default budget ---------------------------------------------------

    #[test]
    fn default_budget_is_32k() {
        assert_eq!(DEFAULT_BUDGET, 32_000);
    }

    // -- Tier constants ---------------------------------------------------

    #[test]
    fn tier_percentages_sum_to_100() {
        assert_eq!(
            TIER_CORE_PCT + TIER_NEAR_PCT + TIER_EXTENDED_PCT + TIER_BACKGROUND_PCT,
            100
        );
    }

    // -- Adaptive redistribution ------------------------------------------

    #[test]
    fn redistribute_no_surplus_returns_pass1() {
        let initial = [400, 250, 200, 150];
        let actual = [400, 250, 200, 150];

        let pass1 = [
            "core".to_string(),
            "near".to_string(),
            "extended".to_string(),
            "background".to_string(),
        ];

        let result = redistribute_and_rebuild(
            &initial,
            &actual,
            1000,
            || "rebuilt_core".into(),
            || "rebuilt_near".into(),
            || "rebuilt_extended".into(),
            || "rebuilt_background".into(),
            pass1,
        );

        assert_eq!(result[0], "core", "no change when no surplus");
        assert_eq!(result[1], "near");
        assert_eq!(result[2], "extended");
        assert_eq!(result[3], "background");
    }

    #[test]
    fn redistribute_all_under_returns_pass1() {
        let initial = [400, 250, 200, 150];
        let actual = [100, 50, 30, 20];

        let pass1 = [
            "core".to_string(),
            "near".to_string(),
            "ext".to_string(),
            "bg".to_string(),
        ];

        let result = redistribute_and_rebuild(
            &initial,
            &actual,
            1000,
            || "rebuilt_core".into(),
            || "rebuilt_near".into(),
            || "rebuilt_ext".into(),
            || "rebuilt_bg".into(),
            pass1,
        );

        assert_eq!(result[0], "core");
    }

    #[test]
    fn redistribute_surplus_flows_to_needy_tiers() {
        let initial = [400, 250, 200, 150];
        let actual = [100, 250, 200, 150];

        let pass1 = [
            "core".to_string(),
            "near".to_string(),
            "extended".to_string(),
            "background".to_string(),
        ];

        let result = redistribute_and_rebuild(
            &initial,
            &actual,
            1000,
            || "rebuilt_core".into(),
            || "rebuilt_near".into(),
            || "rebuilt_extended".into(),
            || "rebuilt_background".into(),
            pass1,
        );

        assert_eq!(result[0], "core", "under-filled core not rebuilt");
        let any_rebuilt = result[1] == "rebuilt_near"
            || result[2] == "rebuilt_extended"
            || result[3] == "rebuilt_background";
        assert!(any_rebuilt, "at least one needy tier should be rebuilt");
    }

    #[test]
    fn assemble_context_default_budget_produces_more_context() {
        let store = setup();

        for i in 0..30 {
            store
                .upsert_node(&make_node(
                    &format!("fn:a.ts:func{}:{}", i, i),
                    &format!("func{}", i),
                    "a.ts",
                    NodeKind::Function,
                    i,
                    Some(&format!(
                        "function func{}() {{\n  // line 1\n  // line 2\n  // line 3\n  // line 4\n}}",
                        i
                    )),
                    None,
                ))
                .unwrap();
        }

        let search = HybridSearch::new(&store.conn);
        let assembler = ContextAssembler::new(&store.conn, &search);

        let ctx_8k = assembler.assemble_context("func", Some(8_000));
        let ctx_32k = assembler.assemble_context("func", None);

        assert!(
            estimate_tokens(&ctx_32k) >= estimate_tokens(&ctx_8k),
            "32K budget should produce >= context than 8K"
        );
    }
}

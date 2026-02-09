//! Core MCP tool handler implementations (14 tools).
//!
//! Contains the business logic for: query, search, dependencies, callers,
//! callees, impact, structure, tests, context, diagram, node, dead_code,
//! frameworks, and languages.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::config::schema::CodeGraphConfig;
use crate::context::assembler::ContextAssembler;
use crate::graph::ranking::GraphRanking;
use crate::graph::search::{HybridSearch, SearchOptions};
use crate::graph::store::GraphStore;
use crate::graph::traversal::GraphTraversal;
use crate::resolution::dead_code::find_dead_code;
use crate::resolution::frameworks::detect_frameworks;
use crate::types::{CodeNode, NodeKind};

use super::server::{
    format_traversal_node, generate_graph_diagram, json_text, mermaid_id, mermaid_safe,
    parse_detail_level, resolve_symbol, DetailLevel,
};

// 1. codegraph_query
pub fn handle_query(
    store: &Arc<Mutex<GraphStore>>,
    query: &str,
    limit: Option<usize>,
    language: Option<String>,
    config: &CodeGraphConfig,
) -> String {
    let store = store.lock().unwrap_or_else(|e| e.into_inner());
    let search = HybridSearch::new(&store.conn);
    let opts = SearchOptions {
        limit: Some(limit.unwrap_or(20)),
        language,
        ..Default::default()
    };
    match search.search(query, &opts) {
        Ok(results) => {
            if config.contexts.is_empty() {
                json_text(&results)
            } else {
                let enriched: Vec<serde_json::Value> = results
                    .iter()
                    .map(|r| {
                        let mut v = serde_json::to_value(r).unwrap_or_default();
                        if let Some(ctx) = config.get_context_for_path(&r.file_path) {
                            v["context"] = serde_json::json!(ctx);
                        }
                        v
                    })
                    .collect();
                json_text(&enriched)
            }
        }
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 1b. codegraph_search
pub fn handle_search(
    store: &Arc<Mutex<GraphStore>>,
    query: &str,
    limit: Option<usize>,
    kind: Option<String>,
    config: &CodeGraphConfig,
) -> String {
    let store = store.lock().unwrap_or_else(|e| e.into_inner());
    let search = HybridSearch::new(&store.conn);
    let limit = limit.unwrap_or(10);
    match search.search_by_keyword(query, limit) {
        Ok(mut results) => {
            if let Some(ref kind_filter) = kind {
                results.retain(|r| r.kind == *kind_filter);
            }
            if config.contexts.is_empty() {
                json_text(&results)
            } else {
                let enriched: Vec<serde_json::Value> = results
                    .iter()
                    .map(|r| {
                        let mut v = serde_json::to_value(r).unwrap_or_default();
                        if let Some(ctx) = config.get_context_for_path(&r.file_path) {
                            v["context"] = serde_json::json!(ctx);
                        }
                        v
                    })
                    .collect();
                json_text(&enriched)
            }
        }
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 2. codegraph_dependencies
pub fn handle_dependencies(
    store_arc: &Arc<Mutex<GraphStore>>,
    symbol: &str,
    max_depth: Option<u32>,
) -> String {
    let node = match resolve_symbol(store_arc, symbol) {
        Some(n) => n,
        None => {
            return json_text(
                &serde_json::json!({"error": format!("Symbol \"{}\" not found in the graph.", symbol)}),
            )
        }
    };
    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    let traversal = GraphTraversal::new(&store);
    let depth = max_depth.unwrap_or(5).min(50);
    match traversal.find_dependencies(&node.id, depth) {
        Ok(deps) => json_text(&serde_json::json!({
            "source": {"id": node.id, "name": node.name, "kind": node.kind.as_str(), "filePath": node.file_path},
            "dependencyCount": deps.len(),
            "dependencies": deps.iter().map(|d| serde_json::json!({
                "id": d.node.id, "name": d.node.name, "kind": d.node.kind.as_str(),
                "filePath": d.node.file_path, "startLine": d.node.start_line, "depth": d.depth,
            })).collect::<Vec<_>>(),
        })),
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 3. codegraph_callers
pub fn handle_callers(
    store_arc: &Arc<Mutex<GraphStore>>,
    symbol: &str,
    max_depth: Option<u32>,
    detail_level: Option<String>,
) -> String {
    let level = parse_detail_level(detail_level.as_deref());
    let node = match resolve_symbol(store_arc, symbol) {
        Some(n) => n,
        None => {
            return json_text(
                &serde_json::json!({"error": format!("Symbol \"{}\" not found in the graph.", symbol)}),
            )
        }
    };
    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    let traversal = GraphTraversal::new(&store);
    let depth = max_depth.unwrap_or(5).min(50);
    match traversal.find_callers(&node.id, depth) {
        Ok(callers) => json_text(&serde_json::json!({
            "target": {"id": node.id, "name": node.name, "kind": node.kind.as_str(), "filePath": node.file_path},
            "callerCount": callers.len(),
            "callers": callers.iter().map(|c| format_traversal_node(c, level)).collect::<Vec<_>>(),
        })),
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 4. codegraph_callees
pub fn handle_callees(
    store_arc: &Arc<Mutex<GraphStore>>,
    symbol: &str,
    max_depth: Option<u32>,
    detail_level: Option<String>,
) -> String {
    let level = parse_detail_level(detail_level.as_deref());
    let node = match resolve_symbol(store_arc, symbol) {
        Some(n) => n,
        None => {
            return json_text(
                &serde_json::json!({"error": format!("Symbol \"{}\" not found in the graph.", symbol)}),
            )
        }
    };
    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    let traversal = GraphTraversal::new(&store);
    let depth = max_depth.unwrap_or(5).min(50);
    match traversal.find_callees(&node.id, depth) {
        Ok(callees) => json_text(&serde_json::json!({
            "source": {"id": node.id, "name": node.name, "kind": node.kind.as_str(), "filePath": node.file_path},
            "calleeCount": callees.len(),
            "callees": callees.iter().map(|c| format_traversal_node(c, level)).collect::<Vec<_>>(),
        })),
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 5. codegraph_impact
pub fn handle_impact(
    store_arc: &Arc<Mutex<GraphStore>>,
    file_path: Option<String>,
    symbol: Option<String>,
) -> String {
    let targets: Vec<CodeNode> = if let Some(ref sym) = symbol {
        match resolve_symbol(store_arc, sym) {
            Some(n) => vec![n],
            None => {
                return json_text(
                    &serde_json::json!({"error": format!("Symbol \"{}\" not found in the graph.", sym)}),
                )
            }
        }
    } else if let Some(ref fp) = file_path {
        let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
        match store.get_nodes_by_file(fp) {
            Ok(nodes) if !nodes.is_empty() => nodes,
            _ => {
                return json_text(
                    &serde_json::json!({"error": format!("No symbols found in file \"{}\".", fp)}),
                )
            }
        }
    } else {
        return json_text(
            &serde_json::json!({"error": "Either 'file_path' or 'symbol' must be provided."}),
        );
    };

    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    let ranking = GraphRanking::new(&store);
    let traversal = GraphTraversal::new(&store);

    let mut all_affected: HashMap<String, (CodeNode, u32)> = HashMap::new();
    let mut affected_files: HashSet<String> = HashSet::new();

    for target in &targets {
        let impact = ranking.compute_impact(&target.id);
        for fp in &impact.affected_files {
            affected_files.insert(fp.clone());
        }
        if let Ok(callers) = traversal.find_callers(&target.id, 10) {
            for c in callers {
                let existing_depth = all_affected.get(&c.node.id).map(|(_, d)| *d);
                if existing_depth.is_none_or(|d| c.depth < d) {
                    all_affected.insert(c.node.id.clone(), (c.node, c.depth));
                }
            }
        }
    }

    let mut high = Vec::new();
    let mut medium = Vec::new();
    let mut low = Vec::new();

    for (node, depth) in all_affected.values() {
        let entry = serde_json::json!({
            "id": node.id, "name": node.name, "kind": node.kind.as_str(),
            "filePath": node.file_path, "depth": depth,
        });
        if *depth <= 1 {
            high.push(entry);
        } else if *depth <= 3 {
            medium.push(entry);
        } else {
            low.push(entry);
        }
    }

    let mut risk_groups = Vec::new();
    if !high.is_empty() {
        risk_groups.push(serde_json::json!({"risk": "high", "symbols": high}));
    }
    if !medium.is_empty() {
        risk_groups.push(serde_json::json!({"risk": "medium", "symbols": medium}));
    }
    if !low.is_empty() {
        risk_groups.push(serde_json::json!({"risk": "low", "symbols": low}));
    }

    let mut sorted_files: Vec<_> = affected_files.into_iter().collect();
    sorted_files.sort();

    json_text(&serde_json::json!({
        "analyzedSymbols": targets.iter().map(|t| serde_json::json!({
            "id": t.id, "name": t.name, "kind": t.kind.as_str(),
        })).collect::<Vec<_>>(),
        "totalAffected": all_affected.len(),
        "affectedFiles": sorted_files,
        "affectedFileCount": sorted_files.len(),
        "riskGroups": risk_groups,
    }))
}

// 6. codegraph_structure
pub fn handle_structure(
    store_arc: &Arc<Mutex<GraphStore>>,
    path: Option<String>,
    depth: Option<usize>,
) -> String {
    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    let limit = depth.unwrap_or(10);

    let stats = match store.get_stats() {
        Ok(s) => s,
        Err(e) => return json_text(&serde_json::json!({"error": e.to_string()})),
    };

    let all_nodes = match store.get_all_nodes() {
        Ok(nodes) => nodes,
        Err(e) => return json_text(&serde_json::json!({"error": e.to_string()})),
    };

    let scoped_nodes: Vec<&CodeNode> = if let Some(ref p) = path {
        all_nodes
            .iter()
            .filter(|n| n.file_path.starts_with(p))
            .collect()
    } else {
        all_nodes.iter().collect()
    };

    if scoped_nodes.is_empty() {
        return json_text(&serde_json::json!({
            "error": if let Some(p) = path {
                format!("No symbols found under path \"{}\".", p)
            } else {
                "The code graph is empty. Index a directory first.".to_string()
            }
        }));
    }

    let mut files_by_dir: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_files = HashSet::new();
    for node in &scoped_nodes {
        all_files.insert(node.file_path.clone());
        let parts: Vec<&str> = node.file_path.rsplitn(2, '/').collect();
        let dir = if parts.len() > 1 {
            parts[1].to_string()
        } else {
            ".".to_string()
        };
        let files = files_by_dir.entry(dir).or_default();
        if !files.contains(&node.file_path) {
            files.push(node.file_path.clone());
        }
    }

    let ranking = GraphRanking::new(&store);
    let page_rank = ranking.compute_page_rank(0.85, 100);
    let node_id_set: HashSet<&str> = scoped_nodes.iter().map(|n| n.id.as_str()).collect();
    let scoped_ranks: Vec<_> = page_rank
        .iter()
        .filter(|r| node_id_set.contains(r.node_id.as_str()))
        .take(limit)
        .collect();

    let top_symbols: Vec<serde_json::Value> = scoped_ranks
        .iter()
        .map(|r| {
            let node = store.get_node(&r.node_id).ok().flatten();
            match node {
                Some(n) => serde_json::json!({
                    "id": n.id, "name": n.name, "kind": n.kind.as_str(),
                    "filePath": n.file_path, "score": r.score,
                }),
                None => serde_json::json!({
                    "id": r.node_id, "name": r.node_id, "kind": "unknown",
                    "filePath": "", "score": r.score,
                }),
            }
        })
        .collect();

    let mut kind_counts: HashMap<&str, usize> = HashMap::new();
    for node in &scoped_nodes {
        *kind_counts.entry(node.kind.as_str()).or_insert(0) += 1;
    }

    let mut modules: Vec<serde_json::Value> = files_by_dir
        .iter()
        .map(|(dir, files)| serde_json::json!({"directory": dir, "fileCount": files.len()}))
        .collect();
    modules.sort_by(|a, b| {
        b["fileCount"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["fileCount"].as_u64().unwrap_or(0))
    });
    modules.truncate(limit);

    json_text(&serde_json::json!({
        "stats": {
            "totalNodes": stats.nodes,
            "totalEdges": stats.edges,
            "totalFiles": stats.files,
            "scopedNodes": scoped_nodes.len(),
            "scopedFiles": all_files.len(),
        },
        "symbolsByKind": kind_counts,
        "topSymbols": top_symbols,
        "modules": modules,
    }))
}

// 7. codegraph_tests
pub fn handle_tests(store_arc: &Arc<Mutex<GraphStore>>, symbol: &str) -> String {
    let node = match resolve_symbol(store_arc, symbol) {
        Some(n) => n,
        None => {
            return json_text(
                &serde_json::json!({"error": format!("Symbol \"{}\" not found in the graph.", symbol)}),
            )
        }
    };

    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());

    // Fast path: use is_test column to find indexed test nodes that call
    // the target (directly or transitively up to depth 5).
    let test_nodes = find_tests_via_is_test(&store, &node.id);

    // Fallback: if the is_test column query returned nothing (possibly an
    // older database without the column populated), use the heuristic
    // traversal approach.
    let test_nodes = if test_nodes.is_empty() {
        let traversal = GraphTraversal::new(&store);
        match traversal.find_tests(&node.id) {
            Ok(nodes) => nodes,
            Err(e) => return json_text(&serde_json::json!({"error": e.to_string()})),
        }
    } else {
        test_nodes
    };

    if test_nodes.is_empty() {
        return json_text(&serde_json::json!({
            "symbol": {"id": node.id, "name": node.name, "kind": node.kind.as_str()},
            "testCount": 0,
            "message": format!("No tests found that reference \"{}\".", node.name),
        }));
    }

    let mut by_file: HashMap<&str, Vec<serde_json::Value>> = HashMap::new();
    for test in &test_nodes {
        by_file
            .entry(&test.file_path)
            .or_default()
            .push(serde_json::json!({
                "id": test.id, "name": test.name, "kind": test.kind.as_str(),
                "startLine": test.start_line,
            }));
    }

    let test_files: Vec<serde_json::Value> = by_file
        .into_iter()
        .map(|(fp, symbols)| serde_json::json!({"filePath": fp, "symbols": symbols}))
        .collect();

    json_text(&serde_json::json!({
        "symbol": {"id": node.id, "name": node.name, "kind": node.kind.as_str(), "filePath": node.file_path},
        "testCount": test_nodes.len(),
        "testFiles": test_files,
    }))
}

/// Find test nodes that reference the target node using the `is_test` column.
///
/// Uses a recursive CTE to traverse callers up to depth 5, then filters
/// to nodes where `is_test = 1`. This is faster than the heuristic approach
/// because it uses an indexed column instead of LIKE patterns.
fn find_tests_via_is_test(store: &GraphStore, node_id: &str) -> Vec<CodeNode> {
    use crate::db::converters::row_to_code_node;

    const FIND_TESTS_IS_TEST_SQL: &str = "\
WITH RECURSIVE callers(id, depth, path) AS (
    SELECT source_id, 1, target_id || '<-' || source_id
    FROM edges
    WHERE target_id = ?1

    UNION

    SELECT e.source_id, c.depth + 1, c.path || '<-' || e.source_id
    FROM callers c
    JOIN edges e ON e.target_id = c.id
    WHERE c.depth < 5
      AND instr(c.path, e.source_id) = 0
)
SELECT DISTINCT n.*
FROM callers c
JOIN nodes n ON n.id = c.id
WHERE n.is_test = 1
ORDER BY n.file_path ASC, n.start_line ASC";

    let mut stmt = match store.conn.prepare_cached(FIND_TESTS_IS_TEST_SQL) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let rows = match stmt.query_and_then(rusqlite::params![node_id], row_to_code_node) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    rows.filter_map(|r| r.ok()).collect()
}

// 8. codegraph_context
pub fn handle_context(
    store_arc: &Arc<Mutex<GraphStore>>,
    query: &str,
    budget: Option<usize>,
    detail_level: Option<String>,
) -> String {
    let level = parse_detail_level(detail_level.as_deref());
    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    let search = HybridSearch::new(&store.conn);

    let base_budget = budget.unwrap_or(8000).min(100_000);
    let effective_budget = match level {
        DetailLevel::Summary => Some(base_budget / 2),
        DetailLevel::Standard => Some(base_budget),
        DetailLevel::Full => Some((base_budget * 2).min(100_000)),
    };

    let assembler = ContextAssembler::new(&store.conn, &search);
    assembler.assemble_context(query, effective_budget)
}

// 9. codegraph_diagram
pub fn handle_diagram(
    store_arc: &Arc<Mutex<GraphStore>>,
    symbol: Option<String>,
    diagram_type: Option<String>,
) -> String {
    let dt = diagram_type.as_deref().unwrap_or("dependency");

    if dt == "module" {
        let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
        let all_edges = match store.get_all_edges() {
            Ok(e) => e,
            Err(e) => return json_text(&serde_json::json!({"error": e.to_string()})),
        };
        let all_nodes = match store.get_all_nodes() {
            Ok(n) => n,
            Err(e) => return json_text(&serde_json::json!({"error": e.to_string()})),
        };

        if all_nodes.is_empty() {
            return json_text(&serde_json::json!({"error": "The code graph is empty."}));
        }

        let node_file_map: HashMap<&str, &str> = all_nodes
            .iter()
            .map(|n| (n.id.as_str(), n.file_path.as_str()))
            .collect();

        let mut file_edges: HashMap<&str, HashSet<&str>> = HashMap::new();
        for edge in &all_edges {
            let src_file = node_file_map.get(edge.source.as_str());
            let tgt_file = node_file_map.get(edge.target.as_str());
            if let (Some(&sf), Some(&tf)) = (src_file, tgt_file) {
                if sf != tf {
                    file_edges.entry(sf).or_default().insert(tf);
                }
            }
        }

        let mut lines = Vec::new();
        lines.push("```mermaid".to_string());
        lines.push("graph LR".to_string());
        lines.push("  %% Module dependency diagram".to_string());

        let mut all_files = HashSet::new();
        for (src, targets) in &file_edges {
            all_files.insert(*src);
            for tgt in targets {
                all_files.insert(*tgt);
            }
        }
        for file in &all_files {
            lines.push(format!(
                "  {}[\"{}\"]",
                mermaid_id(file),
                mermaid_safe(file)
            ));
        }
        for (src, targets) in &file_edges {
            let src_mid = mermaid_id(src);
            for tgt in targets {
                lines.push(format!("  {} --> {}", src_mid, mermaid_id(tgt)));
            }
        }
        lines.push("```".to_string());
        return lines.join("\n");
    }

    let sym = match symbol {
        Some(ref s) => s.as_str(),
        None => {
            return json_text(
                &serde_json::json!({"error": "A 'symbol' is required for dependency and call diagrams."}),
            )
        }
    };

    let node = match resolve_symbol(store_arc, sym) {
        Some(n) => n,
        None => {
            return json_text(
                &serde_json::json!({"error": format!("Symbol \"{}\" not found in the graph.", sym)}),
            )
        }
    };

    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    let traversal = GraphTraversal::new(&store);

    match traversal.get_neighborhood(&node.id, 2) {
        Ok(neighborhood) => {
            if dt == "call" {
                let call_edges: Vec<_> = neighborhood
                    .edges
                    .iter()
                    .filter(|e| e.kind == crate::types::EdgeKind::Calls)
                    .cloned()
                    .collect();
                generate_graph_diagram(&node, &neighborhood.nodes, &call_edges, "Call Graph")
            } else {
                generate_graph_diagram(
                    &node,
                    &neighborhood.nodes,
                    &neighborhood.edges,
                    "Dependency Graph",
                )
            }
        }
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 10. codegraph_node
pub fn handle_node(
    store_arc: &Arc<Mutex<GraphStore>>,
    symbol: &str,
    include_relations: Option<bool>,
    detail_level: Option<String>,
) -> String {
    let level = parse_detail_level(detail_level.as_deref());
    let node = match resolve_symbol(store_arc, symbol) {
        Some(n) => n,
        None => {
            let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
            let like_query = format!("%{}%", symbol);
            let mut stmt = match store
                .conn
                .prepare_cached("SELECT * FROM nodes WHERE name LIKE ?1 ORDER BY name ASC LIMIT 10")
            {
                Ok(s) => s,
                Err(e) => return json_text(&serde_json::json!({"error": e.to_string()})),
            };
            let suggestions: Vec<String> = stmt
                .query_map(rusqlite::params![like_query], |row| row.get::<_, String>(2))
                .ok()
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default();

            return json_text(&serde_json::json!({
                "error": format!("Symbol \"{}\" not found in the graph.", symbol),
                "suggestions": suggestions,
            }));
        }
    };

    if level == DetailLevel::Summary {
        let mut result = serde_json::json!({
            "name": node.name,
            "kind": node.kind.as_str(),
            "filePath": node.file_path,
            "startLine": node.start_line,
        });
        if let Some(ref qn) = node.qualified_name {
            result["qualifiedName"] = serde_json::json!(qn);
        }
        if let Some(ref body) = node.body {
            let sig = body.lines().next().unwrap_or("");
            if !sig.is_empty() {
                result["signature"] = serde_json::json!(sig);
            }
        }
        return json_text(&result);
    }

    let mut result = serde_json::json!({
        "id": node.id,
        "name": node.name,
        "kind": node.kind.as_str(),
        "filePath": node.file_path,
        "startLine": node.start_line,
        "endLine": node.end_line,
        "language": node.language.as_str(),
        "exported": node.exported,
    });

    if let Some(ref qn) = node.qualified_name {
        result["qualifiedName"] = serde_json::json!(qn);
    }
    if let Some(ref doc) = node.documentation {
        result["documentation"] = serde_json::json!(doc);
    }
    if let Some(ref body) = node.body {
        result["body"] = serde_json::json!(body);
    }

    let show_relations = include_relations.unwrap_or(false) || level == DetailLevel::Full;
    if show_relations {
        let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
        let traversal = GraphTraversal::new(&store);

        if let Ok(callers) = traversal.find_callers(&node.id, 1) {
            result["callers"] =
                serde_json::json!(callers.iter().map(|c| serde_json::json!({
                "name": c.node.name, "kind": c.node.kind.as_str(), "filePath": c.node.file_path,
            })).collect::<Vec<_>>());
        }
        if let Ok(callees) = traversal.find_callees(&node.id, 1) {
            result["callees"] =
                serde_json::json!(callees.iter().map(|c| serde_json::json!({
                "name": c.node.name, "kind": c.node.kind.as_str(), "filePath": c.node.file_path,
            })).collect::<Vec<_>>());
        }
        if let Ok(out_edges) = store.get_out_edges(&node.id, None) {
            result["outgoingEdges"] = serde_json::json!(out_edges
                .iter()
                .map(|e| serde_json::json!({"target": e.target, "kind": e.kind.as_str()}))
                .collect::<Vec<_>>());
        }
        if let Ok(in_edges) = store.get_in_edges(&node.id, None) {
            result["incomingEdges"] = serde_json::json!(in_edges
                .iter()
                .map(|e| serde_json::json!({"source": e.source, "kind": e.kind.as_str()}))
                .collect::<Vec<_>>());
        }
    }

    json_text(&result)
}

// 11. codegraph_dead_code
pub fn handle_dead_code(
    store_arc: &Arc<Mutex<GraphStore>>,
    kinds: Option<String>,
    include_exported: Option<bool>,
) -> String {
    let kind_filter: Vec<NodeKind> = kinds
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .filter_map(NodeKind::from_str_loose)
        .collect();

    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    let results = find_dead_code(&store.conn, &kind_filter);
    let _ = include_exported;

    if results.is_empty() {
        return json_text(&serde_json::json!({
            "deadCodeCount": 0,
            "message": "No dead code found. All symbols have incoming references (or are excluded as exports/tests/entry points).",
        }));
    }

    let mut by_file: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    for r in &results {
        by_file
            .entry(r.file_path.clone())
            .or_default()
            .push(serde_json::json!({
                "id": r.id, "name": r.name, "kind": r.kind, "line": r.start_line,
            }));
    }

    let mut files: Vec<serde_json::Value> = by_file
        .into_iter()
        .map(|(fp, symbols)| serde_json::json!({"filePath": fp, "symbols": symbols}))
        .collect();
    files.sort_by(|a, b| a["filePath"].as_str().cmp(&b["filePath"].as_str()));

    json_text(&serde_json::json!({
        "deadCodeCount": results.len(),
        "files": files,
    }))
}

// 12. codegraph_frameworks
pub fn handle_frameworks(
    store_arc: &Arc<Mutex<GraphStore>>,
    project_dir: Option<String>,
) -> String {
    let dir = if let Some(ref d) = project_dir {
        d.clone()
    } else {
        let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
        match store.get_all_nodes() {
            Ok(nodes) if !nodes.is_empty() => {
                let mut paths: Vec<&str> = nodes.iter().map(|n| n.file_path.as_str()).collect();
                paths.sort();
                if let Some(first) = paths.first() {
                    first.rsplitn(2, '/').last().unwrap_or(".").to_string()
                } else {
                    ".".to_string()
                }
            }
            _ => ".".to_string(),
        }
    };

    let frameworks = detect_frameworks(&dir);

    if frameworks.is_empty() {
        return json_text(&serde_json::json!({
            "frameworkCount": 0,
            "message": format!("No recognized frameworks detected in \"{}\".", dir),
        }));
    }

    let entries: Vec<serde_json::Value> = frameworks
        .iter()
        .map(|f| {
            serde_json::json!({
                "name": f.name, "version": f.version,
                "language": f.language, "category": f.category, "confidence": f.confidence,
            })
        })
        .collect();

    json_text(&serde_json::json!({
        "frameworkCount": frameworks.len(),
        "projectDir": dir,
        "frameworks": entries,
    }))
}

// 13. codegraph_languages
pub fn handle_languages(store_arc: &Arc<Mutex<GraphStore>>) -> String {
    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());

    let lang_query = "\
        SELECT language, COUNT(DISTINCT file_path) as file_count, COUNT(*) as symbol_count \
        FROM nodes GROUP BY language ORDER BY symbol_count DESC";
    let edge_count_query = "\
        SELECT n.language, COUNT(*) as edge_count \
        FROM edges e JOIN nodes n ON n.id = e.source_id GROUP BY n.language";

    let lang_stats: Vec<(String, i64, i64)> = match store.conn.prepare(lang_query) {
        Ok(mut stmt) => {
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            });
            match rows {
                Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
                Err(_) => Vec::new(),
            }
        }
        Err(e) => return json_text(&serde_json::json!({"error": e.to_string()})),
    };

    if lang_stats.is_empty() {
        return json_text(&serde_json::json!({
            "languageCount": 0,
            "message": "No indexed files found. Run 'codegraph index <dir>' first.",
        }));
    }

    let mut edge_counts: HashMap<String, i64> = HashMap::new();
    if let Ok(mut stmt) = store.conn.prepare(edge_count_query) {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        }) {
            for row in rows.flatten() {
                edge_counts.insert(row.0, row.1);
            }
        }
    }

    let total_symbols: i64 = lang_stats.iter().map(|(_, _, s)| s).sum();
    let languages: Vec<serde_json::Value> = lang_stats
        .iter()
        .map(|(lang, files, symbols)| {
            let pct = if total_symbols > 0 {
                (*symbols as f64 / total_symbols as f64) * 100.0
            } else {
                0.0
            };
            serde_json::json!({
                "language": lang, "files": files, "symbols": symbols,
                "edges": edge_counts.get(lang).copied().unwrap_or(0),
                "percentage": format!("{:.1}%", pct),
            })
        })
        .collect();

    let total_files: i64 = lang_stats.iter().map(|(_, f, _)| f).sum();
    let total_edges: i64 = edge_counts.values().sum();

    json_text(&serde_json::json!({
        "languageCount": lang_stats.len(),
        "totalFiles": total_files,
        "totalSymbols": total_symbols,
        "totalEdges": total_edges,
        "languages": languages,
    }))
}

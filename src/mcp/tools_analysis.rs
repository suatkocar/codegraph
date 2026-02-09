//! Analysis MCP tool handler implementations (7 tools).
//!
//! Contains the business logic for: stats, circular_imports, project_tree,
//! find_references, export_map, import_graph, and file.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::graph::store::GraphStore;
use crate::graph::traversal::GraphTraversal;
use crate::types::CodeNode;

use super::server::{json_text, mermaid_id, mermaid_safe, resolve_symbol};

// 32. codegraph_stats
pub fn handle_stats(store_arc: &Arc<Mutex<GraphStore>>) -> String {
    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    match store.get_stats() {
        Ok(stats) => {
            let unresolved = store.get_unresolved_ref_count().unwrap_or(0);
            json_text(&serde_json::json!({
                "nodes": stats.nodes,
                "edges": stats.edges,
                "files": stats.files,
                "unresolvedRefs": unresolved,
            }))
        }
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 33. codegraph_circular_imports
pub fn handle_circular_imports(store_arc: &Arc<Mutex<GraphStore>>) -> String {
    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    let traversal = GraphTraversal::new(&store);
    match traversal.detect_cycles() {
        Ok(cycles) => {
            if cycles.is_empty() {
                return json_text(&serde_json::json!({
                    "cycleCount": 0,
                    "message": "No circular imports detected.",
                }));
            }
            json_text(&serde_json::json!({
                "cycleCount": cycles.len(),
                "cycles": cycles.iter().map(|c| serde_json::json!({
                    "size": c.size,
                    "nodes": c.node_ids,
                })).collect::<Vec<_>>(),
            }))
        }
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 34. codegraph_project_tree
pub fn handle_project_tree(store_arc: &Arc<Mutex<GraphStore>>, max_depth: Option<usize>) -> String {
    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    let all_nodes = match store.get_all_nodes() {
        Ok(n) => n,
        Err(e) => return json_text(&serde_json::json!({"error": e.to_string()})),
    };

    let mut dir_files: HashMap<String, HashSet<String>> = HashMap::new();
    for node in &all_nodes {
        let parts: Vec<&str> = node.file_path.rsplitn(2, '/').collect();
        let dir = if parts.len() > 1 {
            parts[1].to_string()
        } else {
            ".".to_string()
        };
        dir_files
            .entry(dir)
            .or_default()
            .insert(node.file_path.clone());
    }

    let depth = max_depth.unwrap_or(3);
    let mut tree: Vec<serde_json::Value> = dir_files
        .iter()
        .filter(|(dir, _)| dir.matches('/').count() < depth)
        .map(|(dir, files)| {
            let symbol_count = all_nodes
                .iter()
                .filter(|n| {
                    let parts: Vec<&str> = n.file_path.rsplitn(2, '/').collect();
                    let ndir = if parts.len() > 1 { parts[1] } else { "." };
                    ndir == dir
                })
                .count();
            serde_json::json!({
                "directory": dir,
                "fileCount": files.len(),
                "symbolCount": symbol_count,
            })
        })
        .collect();
    tree.sort_by(|a, b| a["directory"].as_str().cmp(&b["directory"].as_str()));

    json_text(&serde_json::json!({
        "directoryCount": tree.len(),
        "totalFiles": all_nodes.iter().map(|n| &n.file_path).collect::<HashSet<_>>().len(),
        "tree": tree,
    }))
}

// 35. codegraph_find_references
pub fn handle_find_references(store_arc: &Arc<Mutex<GraphStore>>, symbol: &str) -> String {
    let node = match resolve_symbol(store_arc, symbol) {
        Some(n) => n,
        None => {
            return json_text(
                &serde_json::json!({"error": format!("Symbol \"{}\" not found.", symbol)}),
            )
        }
    };

    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    let in_edges = store.get_in_edges(&node.id, None).unwrap_or_default();
    let out_edges = store.get_out_edges(&node.id, None).unwrap_or_default();

    let mut refs: Vec<serde_json::Value> = Vec::new();
    for edge in &in_edges {
        if let Ok(Some(src)) = store.get_node(&edge.source) {
            refs.push(serde_json::json!({
                "direction": "incoming", "kind": edge.kind.as_str(),
                "symbol": src.name, "file": edge.file_path, "line": edge.line,
            }));
        }
    }
    for edge in &out_edges {
        if let Ok(Some(tgt)) = store.get_node(&edge.target) {
            refs.push(serde_json::json!({
                "direction": "outgoing", "kind": edge.kind.as_str(),
                "symbol": tgt.name, "file": edge.file_path, "line": edge.line,
            }));
        }
    }

    json_text(&serde_json::json!({
        "symbol": {"name": node.name, "kind": node.kind.as_str(), "file": node.file_path},
        "referenceCount": refs.len(),
        "references": refs,
    }))
}

// 36. codegraph_export_map
pub fn handle_export_map(store_arc: &Arc<Mutex<GraphStore>>) -> String {
    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    let all_nodes = match store.get_all_nodes() {
        Ok(n) => n,
        Err(e) => return json_text(&serde_json::json!({"error": e.to_string()})),
    };

    let exported: Vec<&CodeNode> = all_nodes
        .iter()
        .filter(|n| n.exported == Some(true))
        .collect();

    let mut by_file: HashMap<&str, Vec<serde_json::Value>> = HashMap::new();
    for node in &exported {
        by_file
            .entry(&node.file_path)
            .or_default()
            .push(serde_json::json!({
                "name": node.name, "kind": node.kind.as_str(),
                "line": node.start_line,
                "qualifiedName": node.qualified_name,
            }));
    }

    let mut files: Vec<serde_json::Value> = by_file
        .into_iter()
        .map(|(fp, symbols)| serde_json::json!({"filePath": fp, "exports": symbols}))
        .collect();
    files.sort_by(|a, b| a["filePath"].as_str().cmp(&b["filePath"].as_str()));

    json_text(&serde_json::json!({
        "totalExports": exported.len(),
        "fileCount": files.len(),
        "files": files,
    }))
}

// 37. codegraph_import_graph
pub fn handle_import_graph(store_arc: &Arc<Mutex<GraphStore>>, scope: Option<String>) -> String {
    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    let all_edges = match store.get_all_edges() {
        Ok(e) => e,
        Err(e) => return json_text(&serde_json::json!({"error": e.to_string()})),
    };
    let all_nodes = match store.get_all_nodes() {
        Ok(n) => n,
        Err(e) => return json_text(&serde_json::json!({"error": e.to_string()})),
    };

    let import_edges: Vec<_> = all_edges
        .iter()
        .filter(|e| e.kind == crate::types::EdgeKind::Imports)
        .collect();

    let node_file_map: HashMap<&str, &str> = all_nodes
        .iter()
        .map(|n| (n.id.as_str(), n.file_path.as_str()))
        .collect();

    let mut file_imports: HashMap<&str, HashSet<&str>> = HashMap::new();
    for edge in &import_edges {
        let src_file = node_file_map.get(edge.source.as_str());
        let tgt_file = node_file_map.get(edge.target.as_str());
        if let (Some(&sf), Some(&tf)) = (src_file, tgt_file) {
            if sf != tf {
                if let Some(ref s) = scope {
                    if !sf.starts_with(s.as_str()) && !tf.starts_with(s.as_str()) {
                        continue;
                    }
                }
                file_imports.entry(sf).or_default().insert(tf);
            }
        }
    }

    let mut lines = vec!["```mermaid".to_string(), "graph LR".to_string()];
    let mut all_files = HashSet::new();
    for (src, targets) in &file_imports {
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
    for (src, targets) in &file_imports {
        for tgt in targets {
            lines.push(format!(
                "  {} -->|imports| {}",
                mermaid_id(src),
                mermaid_id(tgt)
            ));
        }
    }
    lines.push("```".to_string());
    lines.join("\n")
}

// 38. codegraph_file
pub fn handle_file(store_arc: &Arc<Mutex<GraphStore>>, file_path: &str) -> String {
    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    match store.get_nodes_by_file(file_path) {
        Ok(nodes) => {
            if nodes.is_empty() {
                return json_text(&serde_json::json!({
                    "error": format!("No symbols found in file '{}'", file_path),
                }));
            }
            json_text(&serde_json::json!({
                "filePath": file_path,
                "symbolCount": nodes.len(),
                "symbols": nodes.iter().map(|n| serde_json::json!({
                    "id": n.id, "name": n.name, "kind": n.kind.as_str(),
                    "startLine": n.start_line, "endLine": n.end_line,
                    "exported": n.exported, "qualifiedName": n.qualified_name,
                })).collect::<Vec<_>>(),
            }))
        }
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

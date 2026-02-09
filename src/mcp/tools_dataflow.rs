//! Data flow MCP tool handler implementations (6 tools).
//!
//! Contains the business logic for: find_path, complexity, data_flow,
//! dead_stores, find_uninitialized, and reaching_defs.

use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::graph::complexity;
use crate::graph::dataflow;
use crate::graph::store::GraphStore;
use crate::graph::traversal::GraphTraversal;
use crate::indexer::parser::CodeParser;

use super::server::{json_text, resolve_symbol};

/// Resolve source code and language from either a file path or explicit parameters.
///
/// When `file_path` is provided, reads the file and detects the language from its extension.
/// Otherwise, requires both `source` and `language` to be provided.
fn resolve_source_input(
    file_path: Option<&str>,
    source: Option<&str>,
    language: Option<&str>,
    project_root: &Path,
) -> Result<(String, String), String> {
    if let Some(path) = file_path {
        let validated = crate::observability::validate_path(path, project_root)?;
        let contents = std::fs::read_to_string(&validated)
            .map_err(|e| format!("Failed to read file \"{}\": {}", path, e))?;
        let lang = CodeParser::detect_language(path)
            .ok_or_else(|| format!("Cannot detect language for file \"{}\"", path))?;
        Ok((contents, lang.as_str().to_string()))
    } else {
        match (source, language) {
            (Some(s), Some(l)) => Ok((s.to_string(), l.to_string())),
            _ => Err("Either file_path or both source and language must be provided.".to_string()),
        }
    }
}

// 39. codegraph_find_path
pub fn handle_find_path(
    store_arc: &Arc<Mutex<GraphStore>>,
    from: &str,
    to: &str,
    max_depth: Option<u32>,
) -> String {
    let from_node = match resolve_symbol(store_arc, from) {
        Some(n) => n,
        None => {
            return json_text(
                &serde_json::json!({"error": format!("Source symbol \"{}\" not found.", from)}),
            )
        }
    };
    let to_node = match resolve_symbol(store_arc, to) {
        Some(n) => n,
        None => {
            return json_text(
                &serde_json::json!({"error": format!("Target symbol \"{}\" not found.", to)}),
            )
        }
    };

    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    let traversal = GraphTraversal::new(&store);
    match traversal.find_call_path(&from_node.id, &to_node.id, max_depth.unwrap_or(10)) {
        Ok(Some(path)) => json_text(&serde_json::json!({
            "found": true,
            "pathLength": path.len(),
            "path": path.iter().map(|n| serde_json::json!({
                "name": n.name, "kind": n.kind.as_str(), "file": n.file_path, "line": n.start_line,
            })).collect::<Vec<_>>(),
        })),
        Ok(None) => json_text(&serde_json::json!({
            "found": false,
            "message": format!("No call path found from \"{}\" to \"{}\".", from, to),
        })),
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 40. codegraph_complexity
pub fn handle_complexity(
    store_arc: &Arc<Mutex<GraphStore>>,
    min_complexity: Option<u32>,
) -> String {
    let store = store_arc.lock().unwrap_or_else(|e| e.into_inner());
    let mut results = complexity::calculate_all_complexities(&store.conn);
    let threshold = min_complexity.unwrap_or(5);
    results.retain(|r| r.cyclomatic >= threshold);
    results.sort_by(|a, b| b.cyclomatic.cmp(&a.cyclomatic));

    json_text(&serde_json::json!({
        "threshold": threshold,
        "functionCount": results.len(),
        "functions": results.iter().take(50).map(|r| serde_json::json!({
            "name": r.name, "file": r.file_path,
            "cyclomatic": r.cyclomatic, "cognitive": r.cognitive,
            "lineCount": r.line_count,
        })).collect::<Vec<_>>(),
    }))
}

// 41. codegraph_data_flow
pub fn handle_data_flow(
    file_path: Option<&str>,
    source: Option<&str>,
    language: Option<&str>,
    project_root: &Path,
) -> String {
    let (src, lang) = match resolve_source_input(file_path, source, language, project_root) {
        Ok(v) => v,
        Err(e) => return json_text(&serde_json::json!({"error": e})),
    };
    let chains = dataflow::find_def_use_chains(&src, &lang);
    json_text(&serde_json::json!({
        "variableCount": chains.len(),
        "chains": chains.iter().map(|c| serde_json::json!({
            "variable": c.variable,
            "definitions": c.definitions.iter().map(|d| serde_json::json!({"line": d.line, "column": d.column})).collect::<Vec<_>>(),
            "uses": c.uses.iter().map(|u| serde_json::json!({"line": u.line, "column": u.column})).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    }))
}

// 42. codegraph_dead_stores
pub fn handle_dead_stores(
    file_path: Option<&str>,
    source: Option<&str>,
    language: Option<&str>,
    project_root: &Path,
) -> String {
    let (src, lang) = match resolve_source_input(file_path, source, language, project_root) {
        Ok(v) => v,
        Err(e) => return json_text(&serde_json::json!({"error": e})),
    };
    let stores = dataflow::find_dead_stores(&src, &lang);
    json_text(&serde_json::json!({
        "deadStoreCount": stores.len(),
        "stores": stores.iter().map(|s| serde_json::json!({
            "variable": s.variable, "line": s.line, "assignedValue": s.assigned_value,
        })).collect::<Vec<_>>(),
    }))
}

// 43. codegraph_find_uninitialized
pub fn handle_find_uninitialized(
    file_path: Option<&str>,
    source: Option<&str>,
    language: Option<&str>,
    project_root: &Path,
) -> String {
    let (src, lang) = match resolve_source_input(file_path, source, language, project_root) {
        Ok(v) => v,
        Err(e) => return json_text(&serde_json::json!({"error": e})),
    };
    let locations = dataflow::find_uninitialized_uses(&src, &lang);
    json_text(&serde_json::json!({
        "uninitializedCount": locations.len(),
        "locations": locations.iter().map(|l| serde_json::json!({
            "line": l.line, "column": l.column,
        })).collect::<Vec<_>>(),
    }))
}

// 44. codegraph_reaching_defs
pub fn handle_reaching_defs(
    file_path: Option<&str>,
    source: Option<&str>,
    language: Option<&str>,
    target_line: u32,
    project_root: &Path,
) -> String {
    let (src, lang) = match resolve_source_input(file_path, source, language, project_root) {
        Ok(v) => v,
        Err(e) => return json_text(&serde_json::json!({"error": e})),
    };
    let chains = dataflow::find_reaching_defs(&src, &lang, target_line);
    json_text(&serde_json::json!({
        "targetLine": target_line,
        "variableCount": chains.len(),
        "reachingDefinitions": chains.iter().map(|c| serde_json::json!({
            "variable": c.variable,
            "definitions": c.definitions.iter().map(|d| serde_json::json!({"line": d.line})).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Create a temp file with the given extension and content, return its path.
    fn temp_source_file(ext: &str, content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(ext)
            .tempfile()
            .expect("create temp file");
        f.write_all(content.as_bytes()).expect("write temp file");
        f.flush().expect("flush temp file");
        f
    }

    // -- resolve_source_input ------------------------------------------------

    #[test]
    fn resolve_source_input_from_file() {
        let f = temp_source_file(".js", "let x = 1;");
        let root = f.path().parent().unwrap();
        let (src, lang) =
            resolve_source_input(Some(f.path().to_str().unwrap()), None, None, root).unwrap();
        assert_eq!(src, "let x = 1;");
        assert_eq!(lang, "javascript");
    }

    #[test]
    fn resolve_source_input_from_source_and_language() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (src, lang) =
            resolve_source_input(None, Some("x = 1"), Some("python"), tmp.path()).unwrap();
        assert_eq!(src, "x = 1");
        assert_eq!(lang, "python");
    }

    #[test]
    fn resolve_source_input_file_path_takes_precedence() {
        let f = temp_source_file(".py", "y = 2");
        let root = f.path().parent().unwrap();
        let (src, lang) = resolve_source_input(
            Some(f.path().to_str().unwrap()),
            Some("ignored"),
            Some("ignored"),
            root,
        )
        .unwrap();
        assert_eq!(src, "y = 2");
        assert_eq!(lang, "python");
    }

    #[test]
    fn resolve_source_input_missing_all_params() {
        let tmp = tempfile::TempDir::new().unwrap();
        let err = resolve_source_input(None, None, None, tmp.path()).unwrap_err();
        assert!(err.contains("Either file_path or both source and language"));
    }

    #[test]
    fn resolve_source_input_missing_language() {
        let tmp = tempfile::TempDir::new().unwrap();
        let err = resolve_source_input(None, Some("code"), None, tmp.path()).unwrap_err();
        assert!(err.contains("Either file_path or both source and language"));
    }

    #[test]
    fn resolve_source_input_nonexistent_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let err =
            resolve_source_input(Some("/nonexistent/path.js"), None, None, tmp.path()).unwrap_err();
        assert!(err.contains("Path not found") || err.contains("Path traversal"));
    }

    #[test]
    fn resolve_source_input_unknown_extension() {
        let f = temp_source_file(".xyz", "stuff");
        let root = f.path().parent().unwrap();
        let err =
            resolve_source_input(Some(f.path().to_str().unwrap()), None, None, root).unwrap_err();
        assert!(err.contains("Cannot detect language"));
    }

    // -- handle_data_flow with file_path -------------------------------------

    #[test]
    fn data_flow_from_file() {
        let f = temp_source_file(".js", "let x = 10;\nlet y = x + 5;");
        let root = f.path().parent().unwrap();
        let result = handle_data_flow(Some(f.path().to_str().unwrap()), None, None, root);
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["chains"].is_array());
        assert!(json["variableCount"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn data_flow_from_source() {
        let tmp = tempfile::TempDir::new().unwrap();
        let result = handle_data_flow(None, Some("let x = 10;"), Some("javascript"), tmp.path());
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["chains"].is_array());
    }

    // -- handle_dead_stores with file_path -----------------------------------

    #[test]
    fn dead_stores_from_file() {
        let f = temp_source_file(".py", "x = 10\ny = 20\nprint(y)\n");
        let root = f.path().parent().unwrap();
        let result = handle_dead_stores(Some(f.path().to_str().unwrap()), None, None, root);
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["stores"].is_array());
    }

    // -- handle_find_uninitialized with file_path ----------------------------

    #[test]
    fn find_uninitialized_from_file() {
        let f = temp_source_file(".js", "console.log(result);\nlet result = compute();");
        let root = f.path().parent().unwrap();
        let result = handle_find_uninitialized(Some(f.path().to_str().unwrap()), None, None, root);
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["locations"].is_array());
    }

    // -- handle_reaching_defs with file_path ---------------------------------

    #[test]
    fn reaching_defs_from_file() {
        let f = temp_source_file(".rs", "let x = 10;\nlet y = 20;\nlet z = x + y;");
        let root = f.path().parent().unwrap();
        let result = handle_reaching_defs(Some(f.path().to_str().unwrap()), None, None, 3, root);
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["reachingDefinitions"].is_array());
        assert_eq!(json["targetLine"].as_u64().unwrap(), 3);
    }

    // -- language detection for various extensions ---------------------------

    #[test]
    fn language_detection_typescript() {
        let f = temp_source_file(".ts", "const x: number = 1;");
        let root = f.path().parent().unwrap();
        let (_, lang) =
            resolve_source_input(Some(f.path().to_str().unwrap()), None, None, root).unwrap();
        assert_eq!(lang, "typescript");
    }

    #[test]
    fn language_detection_rust() {
        let f = temp_source_file(".rs", "let x = 1;");
        let root = f.path().parent().unwrap();
        let (_, lang) =
            resolve_source_input(Some(f.path().to_str().unwrap()), None, None, root).unwrap();
        assert_eq!(lang, "rust");
    }

    #[test]
    fn language_detection_go() {
        let f = temp_source_file(".go", "var x = 1");
        let root = f.path().parent().unwrap();
        let (_, lang) =
            resolve_source_input(Some(f.path().to_str().unwrap()), None, None, root).unwrap();
        assert_eq!(lang, "go");
    }

    #[test]
    fn language_detection_java() {
        let f = temp_source_file(".java", "int x = 1;");
        let root = f.path().parent().unwrap();
        let (_, lang) =
            resolve_source_input(Some(f.path().to_str().unwrap()), None, None, root).unwrap();
        assert_eq!(lang, "java");
    }

    // -- error path tests ----------------------------------------------------

    #[test]
    fn data_flow_error_on_missing_params() {
        let tmp = tempfile::TempDir::new().unwrap();
        let result = handle_data_flow(None, None, None, tmp.path());
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json["error"].as_str().unwrap().contains("Either file_path"));
    }

    #[test]
    fn dead_stores_error_on_nonexistent_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let result = handle_dead_stores(Some("/no/such/file.py"), None, None, tmp.path());
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        let err = json["error"].as_str().unwrap();
        assert!(err.contains("Path not found") || err.contains("Path traversal"));
    }
}

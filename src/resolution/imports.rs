//! Cross-file import path resolution.
//!
//! Resolves relative import specifiers (e.g., `./utils`, `../helpers/auth`) to
//! actual file paths in the indexed codebase, then creates direct symbol-to-symbol
//! edges that connect the graph across file boundaries.
//!
//! # Strategy
//!
//! 1. For each `Imports` edge with a relative specifier (`./` or `../`):
//!    - Resolve the path relative to the importing file's directory
//!    - Try common extension patterns (.ts, .tsx, .js, .jsx, .py, /index.ts, etc.)
//!    - If the resolved file exists in indexed files, create cross-file edges
//! 2. When imported names are specified (e.g., `import { foo, bar } from './utils'`):
//!    - Create direct `Imports` edges from the importing file to each named symbol
//! 3. When no names are specified (e.g., `import * as utils from './utils'`):
//!    - Create an `Imports` edge from the file to all exported symbols in the target

use std::collections::{HashMap, HashSet};
use std::path::{Component, PathBuf};

use crate::types::{CodeEdge, CodeNode, EdgeKind};

/// Extension patterns to try when resolving import specifiers.
/// Ordered by likelihood for each language ecosystem.
const EXTENSION_PATTERNS: &[&str] = &[
    "",           // exact match (specifier already has extension)
    ".ts",        // TypeScript
    ".tsx",       // TypeScript JSX
    ".js",        // JavaScript
    ".jsx",       // JavaScript JSX
    ".mjs",       // ES Module JS
    ".cjs",       // CommonJS
    "/index.ts",  // TypeScript barrel
    "/index.tsx", // TypeScript JSX barrel
    "/index.js",  // JavaScript barrel
    "/index.jsx", // JavaScript JSX barrel
    ".py",        // Python
    ".rs",        // Rust (mod.rs pattern handled separately)
    ".go",        // Go
    ".java",      // Java
    ".rb",        // Ruby
    ".php",       // PHP
    ".swift",     // Swift
    ".kt",        // Kotlin
    ".kts",       // Kotlin Script
];

/// Resolve all import edges in the graph, creating cross-file symbol links.
///
/// Takes the existing edges (from single-file extraction), the complete set
/// of indexed file paths, and the node index, and returns additional edges
/// that link imports to their actual target symbols.
pub fn resolve_imports(
    edges: &[CodeEdge],
    indexed_files: &HashSet<String>,
    node_index: &HashMap<String, Vec<CodeNode>>,
    nodes_by_file: &HashMap<String, Vec<CodeNode>>,
) -> Vec<CodeEdge> {
    let mut resolved_edges = Vec::new();

    for edge in edges {
        if edge.kind != EdgeKind::Imports {
            continue;
        }

        // Only process module:<specifier> targets with relative paths
        let specifier = match edge.target.strip_prefix("module:") {
            Some(s) => s,
            None => continue,
        };

        // Skip absolute/package imports (not resolvable to local files)
        if !is_relative_import(specifier) {
            continue;
        }

        // Resolve the specifier to an actual file path
        let importing_file = edge.file_path.as_str();
        let resolved_path = match resolve_specifier(importing_file, specifier, indexed_files) {
            Some(p) => p,
            None => continue,
        };

        // Extract imported names from metadata
        let imported_names: Vec<&str> = edge
            .metadata
            .as_ref()
            .and_then(|m| m.get("names"))
            .map(|names| names.split(',').map(|s| s.trim()).collect())
            .unwrap_or_default();

        let target_nodes = nodes_by_file.get(&resolved_path);

        if imported_names.is_empty() {
            // Wildcard/default import: link to all exported symbols in target file
            if let Some(target_file_nodes) = target_nodes {
                for target_node in target_file_nodes {
                    if target_node.exported == Some(true) {
                        resolved_edges.push(CodeEdge {
                            source: edge.source.clone(),
                            target: target_node.id.clone(),
                            kind: EdgeKind::Imports,
                            file_path: edge.file_path.clone(),
                            line: edge.line,
                            metadata: Some(
                                [("resolved".to_string(), resolved_path.clone())]
                                    .into_iter()
                                    .collect(),
                            ),
                        });
                    }
                }
            }
        } else {
            // Named imports: link to specific symbols
            for name in &imported_names {
                // First try: find by name in the target file
                let target_node = target_nodes
                    .and_then(|nodes| nodes.iter().find(|n| n.name == *name));

                if let Some(target) = target_node {
                    resolved_edges.push(CodeEdge {
                        source: edge.source.clone(),
                        target: target.id.clone(),
                        kind: EdgeKind::Imports,
                        file_path: edge.file_path.clone(),
                        line: edge.line,
                        metadata: Some(
                            [("resolved".to_string(), resolved_path.clone())]
                                .into_iter()
                                .collect(),
                        ),
                    });
                } else {
                    // Second try: look up in global node index
                    if let Some(candidates) = node_index.get(*name) {
                        // Prefer the candidate from the resolved file
                        let best = candidates
                            .iter()
                            .find(|n| n.file_path == resolved_path)
                            .or_else(|| candidates.first());

                        if let Some(target) = best {
                            resolved_edges.push(CodeEdge {
                                source: edge.source.clone(),
                                target: target.id.clone(),
                                kind: EdgeKind::Imports,
                                file_path: edge.file_path.clone(),
                                line: edge.line,
                                metadata: Some(
                                    [("resolved".to_string(), resolved_path.clone())]
                                        .into_iter()
                                        .collect(),
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    resolved_edges
}

/// Check if an import specifier is a relative path.
fn is_relative_import(specifier: &str) -> bool {
    specifier.starts_with("./") || specifier.starts_with("../")
}

/// Resolve a relative import specifier to an actual indexed file path.
///
/// Given: importing file `src/routes/api.ts` and specifier `../utils/auth`,
/// tries: `src/utils/auth.ts`, `src/utils/auth.tsx`, `src/utils/auth/index.ts`, etc.
fn resolve_specifier(
    importing_file: &str,
    specifier: &str,
    indexed_files: &HashSet<String>,
) -> Option<String> {
    // Get the directory of the importing file
    let importing_dir = match importing_file.rfind('/') {
        Some(pos) => &importing_file[..pos],
        None => "",
    };

    // Join with specifier and normalize
    let joined = if importing_dir.is_empty() {
        specifier.to_string()
    } else {
        format!("{}/{}", importing_dir, specifier)
    };

    let normalized = normalize_path(&joined);

    // Try each extension pattern
    for ext in EXTENSION_PATTERNS {
        let candidate = format!("{}{}", normalized, ext);
        if indexed_files.contains(&candidate) {
            return Some(candidate);
        }
    }

    None
}

/// Normalize a file path by resolving `.` and `..` components.
///
/// `src/routes/../utils/./auth` → `src/utils/auth`
fn normalize_path(path: &str) -> String {
    let pb = PathBuf::from(path);
    let mut components: Vec<String> = Vec::new();

    for component in pb.components() {
        match component {
            Component::CurDir => {} // skip `.`
            Component::ParentDir => {
                // Go up one level
                components.pop();
            }
            Component::Normal(s) => {
                components.push(s.to_string_lossy().to_string());
            }
            _ => {}
        }
    }

    components.join("/")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Language, NodeKind};

    fn make_node(
        id: &str,
        name: &str,
        file: &str,
        kind: NodeKind,
        exported: Option<bool>,
    ) -> CodeNode {
        CodeNode {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            file_path: file.to_string(),
            start_line: 1,
            end_line: 10,
            start_column: 0,
            end_column: 1,
            language: Language::TypeScript,
            body: None,
            documentation: None,
            exported,
        }
    }

    fn make_import_edge(
        source_file: &str,
        module_spec: &str,
        line: u32,
        names: Option<&str>,
    ) -> CodeEdge {
        let metadata = names.map(|n| {
            [("names".to_string(), n.to_string())]
                .into_iter()
                .collect()
        });
        CodeEdge {
            source: format!("file:{}", source_file),
            target: format!("module:{}", module_spec),
            kind: EdgeKind::Imports,
            file_path: source_file.to_string(),
            line,
            metadata,
        }
    }

    // -- normalize_path -------------------------------------------------------

    #[test]
    fn normalize_resolves_dotdot() {
        assert_eq!(normalize_path("src/routes/../utils/auth"), "src/utils/auth");
    }

    #[test]
    fn normalize_resolves_dot() {
        assert_eq!(normalize_path("src/./utils/./auth"), "src/utils/auth");
    }

    #[test]
    fn normalize_handles_multiple_dotdot() {
        assert_eq!(
            normalize_path("src/a/b/../../c/d"),
            "src/c/d"
        );
    }

    // -- resolve_specifier ----------------------------------------------------

    #[test]
    fn resolves_relative_ts_import() {
        let files: HashSet<String> = ["src/utils.ts", "src/main.ts"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let result = resolve_specifier("src/main.ts", "./utils", &files);
        assert_eq!(result, Some("src/utils.ts".to_string()));
    }

    #[test]
    fn resolves_dotdot_import() {
        let files: HashSet<String> = ["src/utils/auth.ts", "src/routes/api.ts"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let result = resolve_specifier("src/routes/api.ts", "../utils/auth", &files);
        assert_eq!(result, Some("src/utils/auth.ts".to_string()));
    }

    #[test]
    fn resolves_index_barrel() {
        let files: HashSet<String> = ["src/utils/index.ts", "src/main.ts"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let result = resolve_specifier("src/main.ts", "./utils", &files);
        assert_eq!(result, Some("src/utils/index.ts".to_string()));
    }

    #[test]
    fn resolves_exact_extension() {
        let files: HashSet<String> = ["src/config.json", "src/main.ts"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        // If specifier already has extension, exact match first
        let result = resolve_specifier("src/main.ts", "./config.json", &files);
        assert_eq!(result, Some("src/config.json".to_string()));
    }

    #[test]
    fn returns_none_for_missing_file() {
        let files: HashSet<String> = ["src/main.ts"].iter().map(|s| s.to_string()).collect();

        let result = resolve_specifier("src/main.ts", "./nonexistent", &files);
        assert_eq!(result, None);
    }

    #[test]
    fn skips_non_relative_imports() {
        assert!(!is_relative_import("express"));
        assert!(!is_relative_import("@types/node"));
        assert!(is_relative_import("./utils"));
        assert!(is_relative_import("../helpers"));
    }

    // -- resolve_imports (integration) ----------------------------------------

    #[test]
    fn resolves_named_imports_to_symbols() {
        let utils_fn = make_node(
            "fn:src/utils.ts:validate:5",
            "validate",
            "src/utils.ts",
            NodeKind::Function,
            Some(true),
        );
        let utils_class = make_node(
            "class:src/utils.ts:Parser:20",
            "Parser",
            "src/utils.ts",
            NodeKind::Class,
            Some(true),
        );

        let edges = vec![make_import_edge(
            "src/main.ts",
            "./utils",
            1,
            Some("validate,Parser"),
        )];

        let indexed_files: HashSet<String> = ["src/main.ts", "src/utils.ts"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let mut node_index: HashMap<String, Vec<CodeNode>> = HashMap::new();
        node_index
            .entry("validate".to_string())
            .or_default()
            .push(utils_fn.clone());
        node_index
            .entry("Parser".to_string())
            .or_default()
            .push(utils_class.clone());

        let mut nodes_by_file: HashMap<String, Vec<CodeNode>> = HashMap::new();
        nodes_by_file.insert(
            "src/utils.ts".to_string(),
            vec![utils_fn, utils_class],
        );

        let resolved = resolve_imports(&edges, &indexed_files, &node_index, &nodes_by_file);

        assert_eq!(resolved.len(), 2);
        assert!(resolved
            .iter()
            .any(|e| e.target == "fn:src/utils.ts:validate:5"));
        assert!(resolved
            .iter()
            .any(|e| e.target == "class:src/utils.ts:Parser:20"));
        // All resolved edges should have metadata with resolved path
        assert!(resolved
            .iter()
            .all(|e| e.metadata.as_ref().unwrap().contains_key("resolved")));
    }

    #[test]
    fn resolves_wildcard_imports_to_exported_symbols() {
        let exported_fn = make_node(
            "fn:src/utils.ts:helper:1",
            "helper",
            "src/utils.ts",
            NodeKind::Function,
            Some(true),
        );
        let private_fn = make_node(
            "fn:src/utils.ts:internal:10",
            "internal",
            "src/utils.ts",
            NodeKind::Function,
            None, // not exported
        );

        let edges = vec![make_import_edge("src/main.ts", "./utils", 1, None)];

        let indexed_files: HashSet<String> = ["src/main.ts", "src/utils.ts"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let node_index: HashMap<String, Vec<CodeNode>> = HashMap::new();

        let mut nodes_by_file: HashMap<String, Vec<CodeNode>> = HashMap::new();
        nodes_by_file.insert(
            "src/utils.ts".to_string(),
            vec![exported_fn, private_fn],
        );

        let resolved = resolve_imports(&edges, &indexed_files, &node_index, &nodes_by_file);

        // Only the exported function should be linked
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].target, "fn:src/utils.ts:helper:1");
    }

    #[test]
    fn skips_package_imports() {
        let edges = vec![make_import_edge("src/main.ts", "express", 1, Some("Router"))];

        let indexed_files: HashSet<String> =
            ["src/main.ts"].iter().map(|s| s.to_string()).collect();
        let node_index: HashMap<String, Vec<CodeNode>> = HashMap::new();
        let nodes_by_file: HashMap<String, Vec<CodeNode>> = HashMap::new();

        let resolved = resolve_imports(&edges, &indexed_files, &node_index, &nodes_by_file);
        assert!(resolved.is_empty());
    }
}

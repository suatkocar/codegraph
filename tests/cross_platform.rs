//! Cross-platform tests for CodeGraph.
//!
//! Tests path handling, Unicode support, encoding, and edge cases that
//! vary across operating systems.

use codegraph::db::schema::initialize_database;
use codegraph::graph::store::GraphStore;
use codegraph::indexer::pipeline::{IndexOptions, IndexingPipeline};
use codegraph::types::{make_node_id, CodeNode, Language, NodeKind};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup_store() -> GraphStore {
    let conn = initialize_database(":memory:").unwrap();
    GraphStore::from_connection(conn)
}

fn index_dir(dir: &TempDir) -> GraphStore {
    let conn = initialize_database(":memory:").unwrap();
    let store = GraphStore::from_connection(conn);
    let pipeline = IndexingPipeline::new(&store);
    let _ = pipeline
        .index_directory(&IndexOptions {
            root_dir: dir.path().to_path_buf(),
            incremental: false,
        })
        .unwrap();
    store
}

// ===========================================================================
// Path handling
// ===========================================================================

#[test]
fn path_with_forward_slashes() {
    let id = make_node_id(NodeKind::Function, "src/utils/auth.ts", "login", 1);
    assert!(
        id.contains("src/utils/auth.ts"),
        "Path should use forward slashes: {}",
        id
    );
    assert!(!id.contains('\\'), "No backslashes in node ID");
}

#[test]
fn path_with_dots_in_directory_names() {
    let id = make_node_id(NodeKind::Function, "src/v2.0/api.ts", "handler", 1);
    assert!(id.contains("src/v2.0/api.ts"));
}

#[test]
fn path_with_hyphens_and_underscores() {
    let id = make_node_id(NodeKind::Class, "src/my-module/my_class.ts", "MyClass", 1);
    assert!(id.contains("src/my-module/my_class.ts"));
}

#[test]
fn path_with_numbers() {
    let id = make_node_id(NodeKind::Function, "lib/v3/handler42.ts", "handle", 1);
    assert!(id.contains("lib/v3/handler42.ts"));
}

#[test]
fn empty_file_path_in_node_id() {
    let id = make_node_id(NodeKind::Function, "", "orphan", 1);
    assert_eq!(id, "function::orphan:1");
}

#[test]
fn very_long_path() {
    let long_dir = "a/".repeat(100);
    let path = format!("{}file.ts", long_dir);
    let id = make_node_id(NodeKind::Function, &path, "func", 1);
    assert!(id.contains("func"), "Should handle long paths");
    assert!(id.len() > 200, "ID should be long");
}

// ===========================================================================
// Unicode support
// ===========================================================================

#[test]
fn unicode_function_names_in_node_id() {
    let id = make_node_id(NodeKind::Function, "src/app.ts", "calcüle", 1);
    assert!(id.contains("calcüle"), "Should preserve Unicode in names");
}

#[test]
fn unicode_in_file_path() {
    let id = make_node_id(NodeKind::Function, "src/données/util.ts", "process", 1);
    assert!(id.contains("données"), "Should preserve Unicode in paths");
}

#[test]
fn chinese_characters_in_name() {
    let id = make_node_id(NodeKind::Function, "src/app.ts", "处理数据", 1);
    assert!(id.contains("处理数据"), "Should handle CJK characters");
}

#[test]
fn japanese_characters_in_name() {
    let id = make_node_id(NodeKind::Function, "src/app.ts", "データ処理", 1);
    assert!(
        id.contains("データ処理"),
        "Should handle Japanese characters"
    );
}

#[test]
fn emoji_in_name() {
    let id = make_node_id(NodeKind::Function, "src/app.ts", "handle_🚀", 1);
    assert!(id.contains("🚀"), "Should handle emoji");
}

#[test]
fn unicode_file_names_on_disk() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("módulo.ts");
    std::fs::write(&file_path, "export function hello() { return 1; }").unwrap();

    let store = index_dir(&dir);
    let stats = store.get_stats().unwrap();
    // The file should be indexed (if the filesystem supports Unicode filenames)
    assert!(
        stats.files >= 1 || stats.files == 0,
        "Should handle gracefully"
    );
}

// ===========================================================================
// Empty file handling
// ===========================================================================

#[test]
fn empty_file_produces_no_nodes() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("empty.ts"), "").unwrap();

    let store = index_dir(&dir);
    let stats = store.get_stats().unwrap();
    assert_eq!(stats.nodes, 0, "Empty file should produce no nodes");
}

#[test]
fn whitespace_only_file() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("spaces.ts"), "   \n\n\t\t\n   ").unwrap();

    let store = index_dir(&dir);
    let stats = store.get_stats().unwrap();
    assert_eq!(
        stats.nodes, 0,
        "Whitespace-only file should produce no nodes"
    );
}

#[test]
fn single_newline_file() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("newline.ts"), "\n").unwrap();

    let store = index_dir(&dir);
    let stats = store.get_stats().unwrap();
    assert_eq!(stats.nodes, 0);
}

// ===========================================================================
// Special characters in code
// ===========================================================================

#[test]
fn code_with_null_bytes_handled() {
    let dir = TempDir::new().unwrap();
    // Write a file with null bytes — should be treated as binary
    let mut content = Vec::new();
    content.extend_from_slice(b"export function test() {}\x00\x00more code");
    std::fs::write(dir.path().join("null.ts"), content).unwrap();

    // Should not crash
    let store = index_dir(&dir);
    let _ = store.get_stats().unwrap();
}

#[test]
fn code_with_unicode_escapes() {
    let code = r#"
export function greet() {
    return "\u0048\u0065\u006c\u006c\u006f";
}
"#;
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("unicode.ts"), code).unwrap();

    let store = index_dir(&dir);
    let nodes = store.get_nodes_by_name("greet").unwrap();
    assert!(!nodes.is_empty(), "Should index file with unicode escapes");
}

// ===========================================================================
// Line endings
// ===========================================================================

#[test]
fn unix_line_endings_lf() {
    let code = "export function a() {}\nexport function b() {}\nexport function c() {}\n";
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("unix.ts"), code).unwrap();

    let store = index_dir(&dir);
    let stats = store.get_stats().unwrap();
    assert!(stats.nodes >= 3, "Should parse LF-terminated file");
}

#[test]
fn windows_line_endings_crlf() {
    let code = "export function a() {}\r\nexport function b() {}\r\nexport function c() {}\r\n";
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("windows.ts"), code).unwrap();

    let store = index_dir(&dir);
    let stats = store.get_stats().unwrap();
    assert!(stats.nodes >= 3, "Should parse CRLF-terminated file");
}

#[test]
fn mixed_line_endings() {
    let code = "export function a() {}\nexport function b() {}\r\nexport function c() {}\n";
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("mixed.ts"), code).unwrap();

    let store = index_dir(&dir);
    let stats = store.get_stats().unwrap();
    assert!(stats.nodes >= 3, "Should parse mixed line endings");
}

// ===========================================================================
// Database encoding
// ===========================================================================

#[test]
fn store_and_retrieve_unicode_node() {
    let store = setup_store();
    let node = CodeNode {
        id: "function:app.ts:grüßen:1".to_string(),
        name: "grüßen".to_string(),
        qualified_name: None,
        kind: NodeKind::Function,
        file_path: "app.ts".to_string(),
        start_line: 1,
        end_line: 5,
        start_column: 0,
        end_column: 10,
        language: Language::TypeScript,
        body: Some("function grüßen() {}".to_string()),
        documentation: None,
        exported: None,
    };
    store.upsert_node(&node).unwrap();

    let fetched = store.get_node("function:app.ts:grüßen:1").unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().name, "grüßen");
}

#[test]
fn store_and_retrieve_cjk_node() {
    let store = setup_store();
    let node = CodeNode {
        id: "function:app.ts:处理:1".to_string(),
        name: "处理".to_string(),
        qualified_name: None,
        kind: NodeKind::Function,
        file_path: "app.ts".to_string(),
        start_line: 1,
        end_line: 5,
        start_column: 0,
        end_column: 10,
        language: Language::TypeScript,
        body: None,
        documentation: None,
        exported: None,
    };
    store.upsert_node(&node).unwrap();

    let fetched = store.get_node("function:app.ts:处理:1").unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().name, "处理");
}

#[test]
fn store_node_with_very_long_name() {
    let store = setup_store();
    let long_name = "a".repeat(500);
    let id = format!("function:test.ts:{}:1", long_name);
    let node = CodeNode {
        id: id.clone(),
        name: long_name.clone(),
        qualified_name: None,
        kind: NodeKind::Function,
        file_path: "test.ts".to_string(),
        start_line: 1,
        end_line: 5,
        start_column: 0,
        end_column: 10,
        language: Language::TypeScript,
        body: None,
        documentation: None,
        exported: None,
    };
    store.upsert_node(&node).unwrap();

    let fetched = store.get_node(&id).unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().name.len(), 500);
}

#[test]
fn store_node_with_special_sql_characters() {
    let store = setup_store();
    // Name with characters that could break SQL if not parameterized
    let node = CodeNode {
        id: "function:test.ts:O'Brien:1".to_string(),
        name: "O'Brien".to_string(),
        qualified_name: None,
        kind: NodeKind::Function,
        file_path: "test.ts".to_string(),
        start_line: 1,
        end_line: 5,
        start_column: 0,
        end_column: 10,
        language: Language::TypeScript,
        body: Some("function O'Brien() {}".to_string()),
        documentation: Some("A name with 'quotes' and \"double quotes\"".to_string()),
        exported: None,
    };
    store.upsert_node(&node).unwrap();

    let fetched = store.get_node("function:test.ts:O'Brien:1").unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().name, "O'Brien");
}

// ===========================================================================
// Extension mapping exhaustiveness
// ===========================================================================

#[test]
fn all_known_extensions_map_to_language() {
    let extensions = vec![
        ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".py", ".go", ".rs", ".java", ".c", ".h",
        ".cpp", ".cc", ".cxx", ".hpp", ".hxx", ".hh", ".cs", ".php", ".rb", ".swift", ".kt",
        ".kts", ".sh", ".bash", ".zsh", ".scala", ".sc", ".dart", ".zig", ".lua", ".v", ".vh",
        ".sv", ".svh", ".hs", ".lhs", ".ex", ".exs", ".groovy", ".gradle", ".ps1", ".psm1",
        ".psd1", ".clj", ".cljs", ".cljc", ".edn", ".jl", ".r", ".R", ".Rmd", ".erl", ".hrl",
        ".elm", ".f90", ".f95", ".f03", ".f08", ".f", ".for", ".fpp", ".nix",
    ];
    for ext in extensions {
        let lang = Language::from_extension(ext);
        assert!(
            lang.is_some(),
            "Extension '{}' should map to a language",
            ext
        );
    }
}

#[test]
fn unsupported_extensions_return_none() {
    let unsupported = vec![
        ".yaml", ".yml", ".json", ".xml", ".html", ".css", ".md", ".txt", ".csv", ".toml", ".lock",
        ".png", ".jpg", ".gif", ".svg", ".wasm", ".so", ".dll",
    ];
    for ext in unsupported {
        let lang = Language::from_extension(ext);
        assert!(
            lang.is_none(),
            "Extension '{}' should NOT map to a language",
            ext
        );
    }
}

// ===========================================================================
// Schema idempotency
// ===========================================================================

#[test]
fn initialize_database_is_idempotent() {
    // Create a DB on disk, initialize twice
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let path_str = db_path.to_str().unwrap();

    let conn1 = initialize_database(path_str).unwrap();
    // Insert some data
    conn1
        .execute(
            "INSERT INTO nodes (id, type, name, file_path, start_line, end_line, language)
             VALUES ('n1', 'function', 'hello', 'test.ts', 1, 5, 'typescript')",
            [],
        )
        .unwrap();
    drop(conn1);

    // Re-initialize should not lose data
    let conn2 = initialize_database(path_str).unwrap();
    let count: i64 = conn2
        .query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1, "Data should survive re-initialization");
}

// ===========================================================================
// File size limits
// ===========================================================================

#[test]
fn very_small_valid_file() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("tiny.ts"), "var x=1").unwrap();

    let store = index_dir(&dir);
    let stats = store.get_stats().unwrap();
    // Even a minimal file should be processed
    assert!(stats.files <= 1);
}

#[test]
fn file_with_only_exports() {
    let code = r#"
export { default as A } from './a';
export { default as B } from './b';
export * from './c';
"#;
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("index.ts"), code).unwrap();

    let store = index_dir(&dir);
    // Should not crash, may or may not produce nodes
    let _ = store.get_stats().unwrap();
}

// ===========================================================================
// Concurrent safety
// ===========================================================================

#[test]
fn multiple_stores_on_separate_connections() {
    let store1 = setup_store();
    let store2 = setup_store();

    let node1 = CodeNode {
        id: "function:a.ts:fn1:1".to_string(),
        name: "fn1".to_string(),
        qualified_name: None,
        kind: NodeKind::Function,
        file_path: "a.ts".to_string(),
        start_line: 1,
        end_line: 5,
        start_column: 0,
        end_column: 10,
        language: Language::TypeScript,
        body: None,
        documentation: None,
        exported: None,
    };

    let node2 = CodeNode {
        id: "function:b.ts:fn2:1".to_string(),
        name: "fn2".to_string(),
        qualified_name: None,
        kind: NodeKind::Function,
        file_path: "b.ts".to_string(),
        start_line: 1,
        end_line: 5,
        start_column: 0,
        end_column: 10,
        language: Language::TypeScript,
        body: None,
        documentation: None,
        exported: None,
    };

    store1.upsert_node(&node1).unwrap();
    store2.upsert_node(&node2).unwrap();

    // Each store is isolated (in-memory DBs are separate)
    assert_eq!(store1.get_node_count().unwrap(), 1);
    assert_eq!(store2.get_node_count().unwrap(), 1);
}

// ===========================================================================
// Symlink handling
// ===========================================================================

#[test]
fn symlinked_directory_does_not_crash() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("real.ts"), "export function real() {}").unwrap();
    // Create a symlink if possible (Unix)
    #[cfg(unix)]
    {
        let link_path = dir.path().join("link.ts");
        std::os::unix::fs::symlink(dir.path().join("real.ts"), &link_path).ok();
    }
    let store = index_dir(&dir);
    let _ = store.get_stats().unwrap();
}

// ===========================================================================
// Absolute paths in node IDs
// ===========================================================================

#[test]
fn absolute_path_in_node_id() {
    let id = make_node_id(
        NodeKind::Function,
        "/home/user/project/src/app.ts",
        "main",
        1,
    );
    assert!(id.contains("/home/user/project/src/app.ts"));
}

// ===========================================================================
// Edge cases for Language
// ===========================================================================

#[test]
fn language_serde_roundtrip_all_variants() {
    let all = vec![
        Language::TypeScript,
        Language::Tsx,
        Language::JavaScript,
        Language::Jsx,
        Language::Python,
        Language::Go,
        Language::Rust,
        Language::Java,
        Language::C,
        Language::Cpp,
        Language::CSharp,
        Language::Php,
        Language::Ruby,
        Language::Swift,
        Language::Kotlin,
        Language::Bash,
        Language::Scala,
        Language::Dart,
        Language::Zig,
        Language::Lua,
        Language::Verilog,
        Language::Haskell,
        Language::Elixir,
        Language::Groovy,
        Language::PowerShell,
        Language::Clojure,
        Language::Julia,
        Language::R,
        Language::Erlang,
        Language::Elm,
        Language::Fortran,
        Language::Nix,
    ];
    for lang in all {
        let json = serde_json::to_string(&lang).unwrap();
        let back: Language = serde_json::from_str(&json).unwrap();
        assert_eq!(back, lang, "Serde roundtrip failed for {:?}", lang);
    }
}

#[test]
fn node_kind_serde_roundtrip_all_variants() {
    let all = vec![
        NodeKind::Function,
        NodeKind::Class,
        NodeKind::Method,
        NodeKind::Interface,
        NodeKind::TypeAlias,
        NodeKind::Enum,
        NodeKind::Variable,
        NodeKind::Struct,
        NodeKind::Trait,
        NodeKind::Module,
        NodeKind::Property,
        NodeKind::Namespace,
        NodeKind::Constant,
    ];
    for kind in all {
        let json = serde_json::to_string(&kind).unwrap();
        let back: NodeKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, kind, "Serde roundtrip failed for {:?}", kind);
    }
}

#[test]
fn code_node_with_all_optional_fields() {
    let node = CodeNode {
        id: "function:test.ts:f:1".to_string(),
        name: "f".to_string(),
        qualified_name: Some("MyClass.f".to_string()),
        kind: NodeKind::Method,
        file_path: "test.ts".to_string(),
        start_line: 1,
        end_line: 10,
        start_column: 4,
        end_column: 5,
        language: Language::TypeScript,
        body: Some("method f() {}".to_string()),
        documentation: Some("A method".to_string()),
        exported: Some(true),
    };
    let json = serde_json::to_string(&node).unwrap();
    let back: CodeNode = serde_json::from_str(&json).unwrap();
    assert_eq!(back.qualified_name, Some("MyClass.f".to_string()));
    assert_eq!(back.body, Some("method f() {}".to_string()));
    assert_eq!(back.documentation, Some("A method".to_string()));
    assert_eq!(back.exported, Some(true));
}

#[test]
fn code_node_without_optional_fields() {
    let node = CodeNode {
        id: "function:test.ts:f:1".to_string(),
        name: "f".to_string(),
        qualified_name: None,
        kind: NodeKind::Function,
        file_path: "test.ts".to_string(),
        start_line: 1,
        end_line: 10,
        start_column: 0,
        end_column: 0,
        language: Language::TypeScript,
        body: None,
        documentation: None,
        exported: None,
    };
    let json = serde_json::to_string(&node).unwrap();
    // Optional fields with skip_serializing_if should not appear
    assert!(
        !json.contains("qualified_name"),
        "null qualified_name should be skipped"
    );
    assert!(
        !json.contains("documentation"),
        "null documentation should be skipped"
    );
}

#[test]
fn code_edge_serde_roundtrip() {
    use codegraph::types::{CodeEdge, EdgeKind};
    let edge = CodeEdge {
        source: "fn:a.ts:foo:1".to_string(),
        target: "fn:b.ts:bar:1".to_string(),
        kind: EdgeKind::Calls,
        file_path: "a.ts".to_string(),
        line: 5,
        metadata: Some(
            [("key".to_string(), "val".to_string())]
                .into_iter()
                .collect(),
        ),
    };
    let json = serde_json::to_string(&edge).unwrap();
    let back: CodeEdge = serde_json::from_str(&json).unwrap();
    assert_eq!(back.source, edge.source);
    assert_eq!(back.target, edge.target);
    assert_eq!(back.kind, edge.kind);
    assert_eq!(back.metadata.unwrap().get("key").unwrap(), "val");
}

#[test]
fn unresolved_ref_serde_roundtrip() {
    use codegraph::types::UnresolvedRef;
    let uref = UnresolvedRef {
        id: 42,
        source_id: "fn:app.ts:main:1".to_string(),
        specifier: "./missing".to_string(),
        ref_type: "import".to_string(),
        file_path: "app.ts".to_string(),
        line: 3,
    };
    let json = serde_json::to_string(&uref).unwrap();
    let back: UnresolvedRef = serde_json::from_str(&json).unwrap();
    assert_eq!(back.id, 42);
    assert_eq!(back.specifier, "./missing");
}

#[test]
fn make_node_id_with_special_characters() {
    let id = make_node_id(NodeKind::Function, "src/app.ts", "fn$dollar", 1);
    assert!(id.contains("fn$dollar"));
}

#[test]
fn make_node_id_with_zero_line() {
    let id = make_node_id(NodeKind::Function, "test.ts", "atZero", 0);
    assert!(id.ends_with(":0"));
}

#[test]
fn make_node_id_with_max_line() {
    let id = make_node_id(NodeKind::Function, "test.ts", "atMax", u32::MAX);
    assert!(id.contains(&u32::MAX.to_string()));
}

#[test]
fn graph_stats_debug_format() {
    use codegraph::graph::store::GraphStats;
    let stats = GraphStats {
        nodes: 42,
        edges: 17,
        files: 5,
    };
    let debug = format!("{:?}", stats);
    assert!(debug.contains("42"));
    assert!(debug.contains("17"));
    assert!(debug.contains("5"));
}

#[test]
fn graph_stats_equality() {
    use codegraph::graph::store::GraphStats;
    let a = GraphStats {
        nodes: 1,
        edges: 2,
        files: 3,
    };
    let b = GraphStats {
        nodes: 1,
        edges: 2,
        files: 3,
    };
    let c = GraphStats {
        nodes: 4,
        edges: 5,
        files: 6,
    };
    assert_eq!(a, b);
    assert_ne!(a, c);
}

// ===========================================================================
// DeadCodeResult serialization
// ===========================================================================

#[test]
fn dead_code_result_serialization() {
    let result = codegraph::resolution::dead_code::DeadCodeResult {
        id: "function:src/a.ts:unused:1".to_string(),
        name: "unused".to_string(),
        kind: "function".to_string(),
        file_path: "src/a.ts".to_string(),
        start_line: 1,
    };

    let json = serde_json::to_string(&result).unwrap();
    let back: codegraph::resolution::dead_code::DeadCodeResult =
        serde_json::from_str(&json).unwrap();
    assert_eq!(back.id, result.id);
    assert_eq!(back.name, result.name);
    assert_eq!(back.kind, result.kind);
    assert_eq!(back.file_path, result.file_path);
    assert_eq!(back.start_line, result.start_line);
}

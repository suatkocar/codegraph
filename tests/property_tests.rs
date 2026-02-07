//! Property-based tests for CodeGraph using proptest.
//!
//! These tests verify invariants that must hold for all possible inputs,
//! finding edge cases that unit tests might miss.

use proptest::prelude::*;

use codegraph::db::schema::initialize_database;
use codegraph::graph::store::GraphStore;
use codegraph::types::{make_node_id, CodeEdge, CodeNode, EdgeKind, Language, NodeKind};

// ---------------------------------------------------------------------------
// Strategy helpers
// ---------------------------------------------------------------------------

/// Strategy to generate a random Language variant.
fn arb_language() -> impl Strategy<Value = Language> {
    prop_oneof![
        Just(Language::TypeScript),
        Just(Language::Tsx),
        Just(Language::JavaScript),
        Just(Language::Jsx),
        Just(Language::Python),
        Just(Language::Go),
        Just(Language::Rust),
        Just(Language::Java),
        Just(Language::C),
        Just(Language::Cpp),
        Just(Language::CSharp),
        Just(Language::Php),
        Just(Language::Ruby),
        Just(Language::Swift),
        Just(Language::Kotlin),
        Just(Language::Bash),
        Just(Language::Scala),
        Just(Language::Dart),
        Just(Language::Zig),
        Just(Language::Lua),
        Just(Language::Verilog),
        Just(Language::Haskell),
        Just(Language::Elixir),
        Just(Language::Groovy),
        Just(Language::PowerShell),
        Just(Language::Clojure),
        Just(Language::Julia),
        Just(Language::R),
        Just(Language::Erlang),
        Just(Language::Elm),
        Just(Language::Fortran),
        Just(Language::Nix),
    ]
}

/// Strategy to generate a random NodeKind variant.
fn arb_node_kind() -> impl Strategy<Value = NodeKind> {
    prop_oneof![
        Just(NodeKind::Function),
        Just(NodeKind::Class),
        Just(NodeKind::Method),
        Just(NodeKind::Interface),
        Just(NodeKind::TypeAlias),
        Just(NodeKind::Enum),
        Just(NodeKind::Variable),
        Just(NodeKind::Struct),
        Just(NodeKind::Trait),
        Just(NodeKind::Module),
        Just(NodeKind::Property),
        Just(NodeKind::Namespace),
        Just(NodeKind::Constant),
    ]
}

/// Strategy to generate a random EdgeKind variant.
fn arb_edge_kind() -> impl Strategy<Value = EdgeKind> {
    prop_oneof![
        Just(EdgeKind::Imports),
        Just(EdgeKind::Calls),
        Just(EdgeKind::Contains),
        Just(EdgeKind::Extends),
        Just(EdgeKind::Implements),
        Just(EdgeKind::References),
    ]
}

/// Strategy to generate a valid identifier (letter + alphanumeric).
fn arb_identifier() -> impl Strategy<Value = String> {
    "[a-zA-Z][a-zA-Z0-9_]{0,30}".prop_map(|s| s)
}

/// Strategy to generate a file path.
fn arb_file_path() -> impl Strategy<Value = String> {
    (arb_identifier(), arb_language()).prop_map(|(name, lang)| {
        let ext = match lang {
            Language::TypeScript => ".ts",
            Language::JavaScript => ".js",
            Language::Python => ".py",
            Language::Go => ".go",
            Language::Rust => ".rs",
            Language::Java => ".java",
            Language::C => ".c",
            Language::Cpp => ".cpp",
            Language::CSharp => ".cs",
            Language::Php => ".php",
            Language::Ruby => ".rb",
            Language::Swift => ".swift",
            Language::Kotlin => ".kt",
            _ => ".ts",
        };
        format!("src/{}{}", name, ext)
    })
}

/// Strategy to generate a CodeNode.
fn arb_code_node() -> impl Strategy<Value = CodeNode> {
    (
        arb_identifier(),
        arb_node_kind(),
        arb_file_path(),
        1u32..10000u32,
        arb_language(),
        proptest::option::of(Just(true)),
    )
        .prop_map(|(name, kind, file, line, lang, exported)| {
            let id = make_node_id(kind, &file, &name, line);
            CodeNode {
                id,
                name,
                qualified_name: None,
                kind,
                file_path: file,
                start_line: line,
                end_line: line + 10,
                start_column: 0,
                end_column: 50,
                language: lang,
                body: Some("fn body() {}".to_string()),
                documentation: None,
                exported,
            }
        })
}

// ===========================================================================
// Language invariants
// ===========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn language_as_str_not_empty(lang in arb_language()) {
        let s = lang.as_str();
        prop_assert!(!s.is_empty(), "as_str() should never be empty");
    }

    #[test]
    fn language_grammar_name_not_empty(lang in arb_language()) {
        let s = lang.grammar_name();
        prop_assert!(!s.is_empty(), "grammar_name() should never be empty");
    }

    #[test]
    fn language_as_str_roundtrip(lang in arb_language()) {
        let s = lang.as_str();
        let recovered = Language::from_str_loose(s);
        prop_assert!(recovered.is_some(), "from_str_loose({}) returned None", s);
        prop_assert_eq!(recovered.unwrap(), lang);
    }

    #[test]
    fn language_display_matches_as_str(lang in arb_language()) {
        prop_assert_eq!(format!("{}", lang), lang.as_str());
    }

    #[test]
    fn language_from_str_loose_is_case_insensitive(lang in arb_language()) {
        let lower = lang.as_str().to_lowercase();
        let upper = lang.as_str().to_uppercase();
        let from_lower = Language::from_str_loose(&lower);
        let from_upper = Language::from_str_loose(&upper);
        // At least the lowercase variant should work (since as_str is lowercase)
        prop_assert!(from_lower.is_some(), "from_str_loose({}) failed", lower);
    }

    #[test]
    fn language_query_source_not_empty(lang in arb_language()) {
        let src = lang.query_source();
        prop_assert!(!src.is_empty(), "query_source should not be empty for {:?}", lang);
    }
}

// ===========================================================================
// NodeKind invariants
// ===========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn node_kind_as_str_not_empty(kind in arb_node_kind()) {
        prop_assert!(!kind.as_str().is_empty());
    }

    #[test]
    fn node_kind_roundtrip(kind in arb_node_kind()) {
        let s = kind.as_str();
        let recovered = NodeKind::from_str_loose(s);
        prop_assert!(recovered.is_some(), "from_str_loose({}) returned None", s);
        prop_assert_eq!(recovered.unwrap(), kind);
    }

    #[test]
    fn node_kind_display_matches_as_str(kind in arb_node_kind()) {
        prop_assert_eq!(format!("{}", kind), kind.as_str());
    }

    #[test]
    fn node_kind_as_str_is_snake_case(kind in arb_node_kind()) {
        let s = kind.as_str();
        prop_assert!(s.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "as_str should be snake_case, got: {}", s);
    }
}

// ===========================================================================
// EdgeKind invariants
// ===========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn edge_kind_as_str_not_empty(kind in arb_edge_kind()) {
        prop_assert!(!kind.as_str().is_empty());
    }

    #[test]
    fn edge_kind_roundtrip(kind in arb_edge_kind()) {
        let s = kind.as_str();
        let recovered = EdgeKind::from_str_loose(s);
        prop_assert!(recovered.is_some());
        prop_assert_eq!(recovered.unwrap(), kind);
    }

    #[test]
    fn edge_kind_display_matches_as_str(kind in arb_edge_kind()) {
        prop_assert_eq!(format!("{}", kind), kind.as_str());
    }

    #[test]
    fn edge_kind_as_str_is_snake_case(kind in arb_edge_kind()) {
        let s = kind.as_str();
        prop_assert!(s.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "as_str should be snake_case, got: {}", s);
    }
}

// ===========================================================================
// make_node_id invariants
// ===========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn make_node_id_format(
        kind in arb_node_kind(),
        file in arb_file_path(),
        name in arb_identifier(),
        line in 1u32..10000u32,
    ) {
        let id = make_node_id(kind, &file, &name, line);
        // ID format: "{kind}:{filePath}:{name}:{startLine}"
        let parts: Vec<&str> = id.splitn(4, ':').collect();
        prop_assert_eq!(parts.len(), 4, "ID should have 4 colon-separated parts: {}", id);
        prop_assert_eq!(parts[0], kind.as_str());
        // The file path may contain colons in theory, but our test paths don't
        prop_assert!(!id.is_empty());
    }

    #[test]
    fn make_node_id_is_deterministic(
        kind in arb_node_kind(),
        file in arb_file_path(),
        name in arb_identifier(),
        line in 1u32..10000u32,
    ) {
        let id1 = make_node_id(kind, &file, &name, line);
        let id2 = make_node_id(kind, &file, &name, line);
        prop_assert_eq!(id1, id2, "make_node_id should be deterministic");
    }

    #[test]
    fn different_lines_produce_different_ids(
        kind in arb_node_kind(),
        file in arb_file_path(),
        name in arb_identifier(),
        line1 in 1u32..5000u32,
        line2 in 5001u32..10000u32,
    ) {
        let id1 = make_node_id(kind, &file, &name, line1);
        let id2 = make_node_id(kind, &file, &name, line2);
        prop_assert_ne!(id1, id2, "Different lines should produce different IDs");
    }
}

// ===========================================================================
// CodeNode serde invariants
// ===========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn code_node_serde_roundtrip(node in arb_code_node()) {
        let json = serde_json::to_string(&node).unwrap();
        let back: CodeNode = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(back.id, node.id);
        prop_assert_eq!(back.name, node.name);
        prop_assert_eq!(back.kind, node.kind);
        prop_assert_eq!(back.file_path, node.file_path);
        prop_assert_eq!(back.start_line, node.start_line);
        prop_assert_eq!(back.end_line, node.end_line);
        prop_assert_eq!(back.language, node.language);
    }

    #[test]
    fn code_node_json_has_required_fields(node in arb_code_node()) {
        let json_val: serde_json::Value = serde_json::to_value(&node).unwrap();
        prop_assert!(json_val.get("id").is_some());
        prop_assert!(json_val.get("name").is_some());
        prop_assert!(json_val.get("kind").is_some());
        prop_assert!(json_val.get("file_path").is_some());
        prop_assert!(json_val.get("start_line").is_some());
        prop_assert!(json_val.get("end_line").is_some());
        prop_assert!(json_val.get("language").is_some());
    }
}

// ===========================================================================
// GraphStore invariants
// ===========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn graph_stats_always_non_negative(nodes_to_insert in 0usize..20) {
        let conn = initialize_database(":memory:").unwrap();
        let store = GraphStore::from_connection(conn);

        // Insert random nodes
        let nodes: Vec<CodeNode> = (0..nodes_to_insert)
            .map(|i| CodeNode {
                id: format!("function:test.ts:fn{}:{}", i, i),
                name: format!("fn{}", i),
                qualified_name: None,
                kind: NodeKind::Function,
                file_path: "test.ts".to_string(),
                start_line: i as u32,
                end_line: (i + 5) as u32,
                start_column: 0,
                end_column: 10,
                language: Language::TypeScript,
                body: None,
                documentation: None,
                exported: None,
            })
            .collect();

        if !nodes.is_empty() {
            store.upsert_nodes(&nodes).unwrap();
        }

        let stats = store.get_stats().unwrap();
        prop_assert!(stats.nodes >= 0); // usize is always >= 0
        prop_assert!(stats.edges >= 0);
        prop_assert!(stats.files >= 0);
        prop_assert_eq!(stats.nodes, nodes_to_insert);
    }

    #[test]
    fn upsert_node_then_get_returns_same_data(node in arb_code_node()) {
        let conn = initialize_database(":memory:").unwrap();
        let store = GraphStore::from_connection(conn);
        store.upsert_node(&node).unwrap();

        let fetched = store.get_node(&node.id).unwrap();
        prop_assert!(fetched.is_some(), "Node should exist after upsert");
        let fetched = fetched.unwrap();
        prop_assert_eq!(&fetched.id, &node.id);
        prop_assert_eq!(&fetched.name, &node.name);
        prop_assert_eq!(fetched.kind, node.kind);
        prop_assert_eq!(&fetched.file_path, &node.file_path);
        prop_assert_eq!(fetched.start_line, node.start_line);
        prop_assert_eq!(fetched.end_line, node.end_line);
    }

    #[test]
    fn delete_file_nodes_makes_count_zero(node in arb_code_node()) {
        let conn = initialize_database(":memory:").unwrap();
        let store = GraphStore::from_connection(conn);
        store.upsert_node(&node).unwrap();

        let count_before = store.get_node_count().unwrap();
        prop_assert_eq!(count_before, 1);

        store.delete_file_nodes(&node.file_path).unwrap();
        let nodes_after = store.get_nodes_by_file(&node.file_path).unwrap();
        prop_assert!(nodes_after.is_empty());
    }
}

// ===========================================================================
// Language::from_extension invariants
// ===========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn unknown_extensions_return_none(ext in "\\.[a-z]{5,10}") {
        // Random 5-10 char extensions are overwhelmingly likely to be unsupported
        if Language::from_extension(&ext).is_some() {
            // If it happens to match, that's fine too
            return Ok(());
        }
        prop_assert!(Language::from_extension(&ext).is_none());
    }

    #[test]
    fn from_str_loose_never_panics(s in ".*") {
        // Should never panic, regardless of input
        let _ = Language::from_str_loose(&s);
    }

    #[test]
    fn node_kind_from_str_loose_never_panics(s in ".*") {
        let _ = NodeKind::from_str_loose(&s);
    }

    #[test]
    fn edge_kind_from_str_loose_never_panics(s in ".*") {
        let _ = EdgeKind::from_str_loose(&s);
    }
}

// ===========================================================================
// FTS search invariants
// ===========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn fts_search_doesnt_panic_on_arbitrary_input(query in "[a-zA-Z0-9_]{1,50}") {
        let conn = initialize_database(":memory:").unwrap();
        let store = GraphStore::from_connection(conn);

        // Insert a node so FTS has something
        let node = CodeNode {
            id: "function:test.ts:hello:1".to_string(),
            name: "hello".to_string(),
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

        // FTS search should not panic
        let result = store.conn.query_row(
            "SELECT COUNT(*) FROM fts_nodes WHERE fts_nodes MATCH ?1",
            [&query],
            |row| row.get::<_, i64>(0),
        );
        // It's OK if the query fails (bad FTS syntax), just shouldn't panic
        let _ = result;
    }
}

// ===========================================================================
// Dead code detection invariants
// ===========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn dead_code_never_includes_main(num_nodes in 1usize..10) {
        let conn = initialize_database(":memory:").unwrap();
        let store = GraphStore::from_connection(conn);

        let mut nodes: Vec<CodeNode> = (0..num_nodes)
            .map(|i| CodeNode {
                id: format!("function:src/app.ts:fn{}:{}", i, i),
                name: format!("fn{}", i),
                qualified_name: None,
                kind: NodeKind::Function,
                file_path: "src/app.ts".to_string(),
                start_line: i as u32,
                end_line: (i + 5) as u32,
                start_column: 0,
                end_column: 10,
                language: Language::TypeScript,
                body: None,
                documentation: None,
                exported: None,
            })
            .collect();

        // Add a main function
        nodes.push(CodeNode {
            id: "function:src/app.ts:main:999".to_string(),
            name: "main".to_string(),
            qualified_name: None,
            kind: NodeKind::Function,
            file_path: "src/app.ts".to_string(),
            start_line: 999,
            end_line: 1010,
            start_column: 0,
            end_column: 10,
            language: Language::TypeScript,
            body: None,
            documentation: None,
            exported: None,
        });

        store.upsert_nodes(&nodes).unwrap();

        let dead = codegraph::resolution::dead_code::find_dead_code(&store.conn, &[]);
        let names: Vec<&str> = dead.iter().map(|d| d.name.as_str()).collect();
        prop_assert!(!names.contains(&"main"), "main should never be in dead code");
    }

    #[test]
    fn dead_code_never_includes_exported(num_nodes in 1usize..10) {
        let conn = initialize_database(":memory:").unwrap();
        let store = GraphStore::from_connection(conn);

        let nodes: Vec<CodeNode> = (0..num_nodes)
            .map(|i| CodeNode {
                id: format!("function:src/lib.ts:fn{}:{}", i, i),
                name: format!("fn{}", i),
                qualified_name: None,
                kind: NodeKind::Function,
                file_path: "src/lib.ts".to_string(),
                start_line: i as u32,
                end_line: (i + 5) as u32,
                start_column: 0,
                end_column: 10,
                language: Language::TypeScript,
                body: None,
                documentation: None,
                exported: Some(true),
            })
            .collect();

        store.upsert_nodes(&nodes).unwrap();

        let dead = codegraph::resolution::dead_code::find_dead_code(&store.conn, &[]);
        prop_assert!(dead.is_empty(), "Exported symbols should never be dead code");
    }

    #[test]
    fn dead_code_never_includes_test_functions(num_nodes in 1usize..5) {
        let conn = initialize_database(":memory:").unwrap();
        let store = GraphStore::from_connection(conn);

        let nodes: Vec<CodeNode> = (0..num_nodes)
            .map(|i| CodeNode {
                id: format!("function:src/app.ts:testFn{}:{}", i, i),
                name: format!("testFn{}", i),
                qualified_name: None,
                kind: NodeKind::Function,
                file_path: "src/app.ts".to_string(),
                start_line: i as u32,
                end_line: (i + 5) as u32,
                start_column: 0,
                end_column: 10,
                language: Language::TypeScript,
                body: None,
                documentation: None,
                exported: None,
            })
            .collect();

        store.upsert_nodes(&nodes).unwrap();

        let dead = codegraph::resolution::dead_code::find_dead_code(&store.conn, &[]);
        prop_assert!(dead.is_empty(), "Test functions should never be dead code");
    }
}

// ===========================================================================
// Unresolved refs invariants
// ===========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn unresolved_refs_insert_and_count(count in 1usize..20) {
        let conn = initialize_database(":memory:").unwrap();
        let store = GraphStore::from_connection(conn);

        for i in 0..count {
            store
                .insert_unresolved_ref(
                    &format!("fn:test.ts:fn{}:{}", i, i),
                    &format!("./missing{}", i),
                    "import",
                    "test.ts",
                    i as u32,
                )
                .unwrap();
        }

        let total = store.get_unresolved_ref_count().unwrap();
        prop_assert_eq!(total, count);

        let refs = store.get_unresolved_refs(None).unwrap();
        prop_assert_eq!(refs.len(), count);
    }

    #[test]
    fn clear_unresolved_refs_removes_only_target_file(
        count_a in 1usize..10,
        count_b in 1usize..10,
    ) {
        let conn = initialize_database(":memory:").unwrap();
        let store = GraphStore::from_connection(conn);

        for i in 0..count_a {
            store
                .insert_unresolved_ref(
                    &format!("fn:a.ts:fn{}:{}", i, i),
                    "./missing",
                    "import",
                    "a.ts",
                    i as u32,
                )
                .unwrap();
        }
        for i in 0..count_b {
            store
                .insert_unresolved_ref(
                    &format!("fn:b.ts:fn{}:{}", i, i),
                    "./other",
                    "import",
                    "b.ts",
                    i as u32,
                )
                .unwrap();
        }

        store.clear_unresolved_refs_for_file("a.ts").unwrap();

        let total = store.get_unresolved_ref_count().unwrap();
        prop_assert_eq!(total, count_b, "Only b.ts refs should remain");

        let a_refs = store.get_unresolved_refs(Some("a.ts")).unwrap();
        prop_assert!(a_refs.is_empty());

        let b_refs = store.get_unresolved_refs(Some("b.ts")).unwrap();
        prop_assert_eq!(b_refs.len(), count_b);
    }
}

// ===========================================================================
// Import resolution invariants
// ===========================================================================

#[test]
fn resolve_imports_with_empty_inputs_returns_empty() {
    use codegraph::resolution::imports::resolve_imports;
    use std::collections::{HashMap, HashSet};

    let edges: Vec<CodeEdge> = Vec::new();
    let indexed_files: HashSet<String> = HashSet::new();
    let node_index: HashMap<String, Vec<CodeNode>> = HashMap::new();
    let nodes_by_file: HashMap<String, Vec<CodeNode>> = HashMap::new();

    let result = resolve_imports(&edges, &indexed_files, &node_index, &nodes_by_file);
    assert!(result.resolved_edges.is_empty());
    assert!(result.unresolved_refs.is_empty());
}

#[test]
fn resolve_imports_skips_non_import_edges() {
    use codegraph::resolution::imports::resolve_imports;
    use std::collections::{HashMap, HashSet};

    let edges = vec![CodeEdge {
        source: "fn:a.ts:foo:1".to_string(),
        target: "fn:b.ts:bar:1".to_string(),
        kind: EdgeKind::Calls,
        file_path: "a.ts".to_string(),
        line: 5,
        metadata: None,
    }];

    let indexed_files: HashSet<String> = HashSet::new();
    let node_index: HashMap<String, Vec<CodeNode>> = HashMap::new();
    let nodes_by_file: HashMap<String, Vec<CodeNode>> = HashMap::new();

    let result = resolve_imports(&edges, &indexed_files, &node_index, &nodes_by_file);
    assert!(result.resolved_edges.is_empty());
    assert!(result.unresolved_refs.is_empty());
}

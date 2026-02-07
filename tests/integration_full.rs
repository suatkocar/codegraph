//! Full end-to-end integration tests for CodeGraph.
//!
//! These tests create temporary directories with real source files, run the
//! indexing pipeline, and verify the resulting graph through the store API.

use std::path::PathBuf;

use codegraph::db::schema::initialize_database;
use codegraph::graph::store::GraphStore;
use codegraph::indexer::pipeline::{IndexOptions, IndexingPipeline};
use codegraph::resolution::dead_code::find_dead_code;
use codegraph::types::{Language, NodeKind};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a temp directory with source files, index it, and return the store.
fn setup_with_code(files: &[(&str, &str)]) -> (TempDir, GraphStore) {
    let dir = TempDir::new().unwrap();
    for (path, content) in files {
        let full_path = dir.path().join(path);
        std::fs::create_dir_all(full_path.parent().unwrap()).unwrap();
        std::fs::write(&full_path, content).unwrap();
    }
    let conn = initialize_database(":memory:").unwrap();
    let store = GraphStore::from_connection(conn);
    let pipeline = IndexingPipeline::new(&store);
    let _result = pipeline
        .index_directory(&IndexOptions {
            root_dir: dir.path().to_path_buf(),
            incremental: false,
        })
        .unwrap();
    (dir, store)
}

// ===========================================================================
// 1. Python file indexing
// ===========================================================================

#[test]
fn index_python_file_with_functions() {
    let code = r#"
def greet(name):
    """Say hello."""
    return f"Hello, {name}!"

def farewell(name):
    return f"Goodbye, {name}!"

class Greeter:
    def __init__(self, name):
        self.name = name

    def say_hello(self):
        return greet(self.name)
"#;
    let (_dir, store) = setup_with_code(&[("main.py", code)]);
    let stats = store.get_stats().unwrap();
    assert!(stats.nodes >= 3, "Expected >= 3 nodes, got {}", stats.nodes);
    assert_eq!(stats.files, 1);

    // Check we can find functions by name
    let greet_nodes = store.get_nodes_by_name("greet").unwrap();
    assert!(!greet_nodes.is_empty(), "Should find 'greet' function");
    assert_eq!(greet_nodes[0].language, Language::Python);

    // Class should be found
    let class_nodes = store.get_nodes_by_name("Greeter").unwrap();
    assert!(!class_nodes.is_empty(), "Should find 'Greeter' class");
}

#[test]
fn index_python_with_imports() {
    let utils = r#"
def validate(data):
    return True

def sanitize(data):
    return data.strip()
"#;
    let main = r#"
from utils import validate, sanitize

def process(data):
    if validate(data):
        return sanitize(data)
    return None
"#;
    let (_dir, store) = setup_with_code(&[("utils.py", utils), ("main.py", main)]);
    let stats = store.get_stats().unwrap();
    assert!(stats.files >= 2, "Expected >= 2 files, got {}", stats.files);
    assert!(stats.nodes >= 3, "Expected >= 3 nodes, got {}", stats.nodes);
}

// ===========================================================================
// 2. TypeScript file indexing
// ===========================================================================

#[test]
fn index_typescript_with_classes_and_methods() {
    let code = r#"
export class UserService {
    private users: Map<string, User> = new Map();

    addUser(name: string): void {
        this.users.set(name, { name });
    }

    getUser(name: string): User | undefined {
        return this.users.get(name);
    }

    deleteUser(name: string): boolean {
        return this.users.delete(name);
    }
}

export interface User {
    name: string;
}

export function createService(): UserService {
    return new UserService();
}
"#;
    let (_dir, store) = setup_with_code(&[("service.ts", code)]);
    let stats = store.get_stats().unwrap();
    assert!(stats.nodes >= 3, "Expected >= 3 nodes, got {}", stats.nodes);

    let service_nodes = store.get_nodes_by_name("UserService").unwrap();
    assert!(!service_nodes.is_empty(), "Should find UserService class");
    assert_eq!(service_nodes[0].language, Language::TypeScript);
}

#[test]
fn index_typescript_with_cross_file_imports() {
    let types = r#"
export interface Config {
    port: number;
    host: string;
}

export const DEFAULT_PORT = 3000;
"#;
    let server = r#"
import { Config, DEFAULT_PORT } from './types';

export function startServer(config: Config): void {
    console.log(`Starting on ${config.host}:${config.port || DEFAULT_PORT}`);
}
"#;
    let (_dir, store) = setup_with_code(&[("types.ts", types), ("server.ts", server)]);
    let stats = store.get_stats().unwrap();
    assert!(stats.files >= 2, "Expected >= 2 files, got {}", stats.files);
    assert!(stats.edges > 0, "Expected edges from imports");
}

// ===========================================================================
// 3. Rust file indexing
// ===========================================================================

#[test]
fn index_rust_file_with_structs_and_impl() {
    let code = r#"
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }

    pub fn distance(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

pub trait Shape {
    fn area(&self) -> f64;
}

pub enum Color {
    Red,
    Green,
    Blue,
}
"#;
    let (_dir, store) = setup_with_code(&[("geometry.rs", code)]);
    let stats = store.get_stats().unwrap();
    assert!(stats.nodes >= 2, "Expected >= 2 nodes, got {}", stats.nodes);

    let point_nodes = store.get_nodes_by_name("Point").unwrap();
    assert!(!point_nodes.is_empty(), "Should find Point struct");
}

// ===========================================================================
// 4. Go file indexing
// ===========================================================================

#[test]
fn index_go_file_with_functions_and_structs() {
    let code = r#"
package main

import "fmt"

type Server struct {
    Host string
    Port int
}

func NewServer(host string, port int) *Server {
    return &Server{Host: host, Port: port}
}

func (s *Server) Start() error {
    fmt.Printf("Listening on %s:%d\n", s.Host, s.Port)
    return nil
}

func main() {
    s := NewServer("localhost", 8080)
    s.Start()
}
"#;
    let (_dir, store) = setup_with_code(&[("main.go", code)]);
    let stats = store.get_stats().unwrap();
    assert!(stats.nodes >= 2, "Expected >= 2 nodes, got {}", stats.nodes);
}

// ===========================================================================
// 5. Java file indexing
// ===========================================================================

#[test]
fn index_java_file_with_class_and_methods() {
    let code = r#"
package com.example;

public class Calculator {
    public int add(int a, int b) {
        return a + b;
    }

    public int multiply(int a, int b) {
        return a * b;
    }

    public static Calculator create() {
        return new Calculator();
    }
}
"#;
    let (_dir, store) = setup_with_code(&[("Calculator.java", code)]);
    let stats = store.get_stats().unwrap();
    assert!(stats.nodes >= 1, "Expected >= 1 node, got {}", stats.nodes);
}

// ===========================================================================
// 6. Multi-language project
// ===========================================================================

#[test]
fn index_multi_language_project() {
    let ts_file = r#"
export function hello(): string {
    return "Hello from TypeScript";
}
"#;
    let py_file = r#"
def hello():
    return "Hello from Python"
"#;
    let rs_file = r#"
pub fn hello() -> String {
    "Hello from Rust".to_string()
}
"#;
    let go_file = r#"
package main

func Hello() string {
    return "Hello from Go"
}
"#;

    let files = &[
        ("src/hello.ts", ts_file),
        ("src/hello.py", py_file),
        ("src/hello.rs", rs_file),
        ("src/hello.go", go_file),
    ];
    let (_dir, store) = setup_with_code(files);
    let stats = store.get_stats().unwrap();
    assert!(stats.files >= 4, "Expected >= 4 files, got {}", stats.files);
    assert!(stats.nodes >= 4, "Expected >= 4 nodes, got {}", stats.nodes);
}

// ===========================================================================
// 7. Empty directory
// ===========================================================================

#[test]
fn index_empty_directory() {
    let dir = TempDir::new().unwrap();
    let conn = initialize_database(":memory:").unwrap();
    let store = GraphStore::from_connection(conn);
    let pipeline = IndexingPipeline::new(&store);
    let result = pipeline
        .index_directory(&IndexOptions {
            root_dir: dir.path().to_path_buf(),
            incremental: false,
        })
        .unwrap();

    assert_eq!(result.files_indexed, 0);
    assert_eq!(result.nodes_created, 0);
    assert_eq!(result.edges_created, 0);
    let stats = store.get_stats().unwrap();
    assert_eq!(stats.nodes, 0);
    assert_eq!(stats.edges, 0);
    assert_eq!(stats.files, 0);
}

// ===========================================================================
// 8. Files with syntax errors (graceful handling)
// ===========================================================================

#[test]
fn index_file_with_syntax_errors_does_not_crash() {
    let broken_ts = r#"
export function broken( {
    // missing closing paren and brace
    return 42
"#;
    let valid_ts = r#"
export function valid(): number {
    return 1;
}
"#;
    // Should not panic — partial extraction or skip
    let (_dir, store) = setup_with_code(&[("broken.ts", broken_ts), ("valid.ts", valid_ts)]);

    // The valid file should still be indexed
    let valid_nodes = store.get_nodes_by_name("valid").unwrap();
    assert!(!valid_nodes.is_empty(), "valid.ts should still be indexed");
}

// ===========================================================================
// 9. Binary files are skipped
// ===========================================================================

#[test]
fn binary_files_are_skipped() {
    let binary_content = (0..=255u8).collect::<Vec<u8>>();
    let dir = TempDir::new().unwrap();

    // Write a binary file with a recognized extension
    let bin_path = dir.path().join("data.ts");
    std::fs::write(&bin_path, &binary_content).unwrap();

    // Write a valid file too
    let valid_path = dir.path().join("valid.ts");
    std::fs::write(&valid_path, "export function ok() {}").unwrap();

    let conn = initialize_database(":memory:").unwrap();
    let store = GraphStore::from_connection(conn);
    let pipeline = IndexingPipeline::new(&store);
    let _result = pipeline
        .index_directory(&IndexOptions {
            root_dir: dir.path().to_path_buf(),
            incremental: false,
        })
        .unwrap();

    // The valid file should be indexed, binary should be handled gracefully
    let stats = store.get_stats().unwrap();
    assert!(stats.files >= 1, "Valid file should be indexed");
}

// ===========================================================================
// 10. Incremental indexing
// ===========================================================================

#[test]
fn incremental_indexing_skips_unchanged_files() {
    let code = r#"export function hello() { return 1; }"#;
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("hello.ts");
    std::fs::write(&file_path, code).unwrap();

    let conn = initialize_database(":memory:").unwrap();
    let store = GraphStore::from_connection(conn);
    let pipeline = IndexingPipeline::new(&store);

    // First index
    let result1 = pipeline
        .index_directory(&IndexOptions {
            root_dir: dir.path().to_path_buf(),
            incremental: false,
        })
        .unwrap();
    assert!(result1.files_indexed >= 1);

    // Second index (incremental) — same file, should skip
    let result2 = pipeline
        .index_directory(&IndexOptions {
            root_dir: dir.path().to_path_buf(),
            incremental: true,
        })
        .unwrap();
    assert_eq!(
        result2.files_indexed, 0,
        "Unchanged file should be skipped on incremental"
    );
    assert_eq!(result2.files_skipped, 1);
}

// ===========================================================================
// 11. Cross-file references produce edges
// ===========================================================================

#[test]
fn cross_file_calls_produce_edges() {
    let util_ts = r#"
export function formatDate(d: Date): string {
    return d.toISOString();
}
"#;
    let app_ts = r#"
import { formatDate } from './util';

export function renderDate(): string {
    return formatDate(new Date());
}
"#;
    let (_dir, store) = setup_with_code(&[("util.ts", util_ts), ("app.ts", app_ts)]);
    let stats = store.get_stats().unwrap();
    assert!(
        stats.edges >= 1,
        "Expected import edges, got {}",
        stats.edges
    );
}

// ===========================================================================
// 12. Node lookup by type
// ===========================================================================

#[test]
fn get_nodes_by_type_returns_correct_kinds() {
    let code = r#"
export class MyClass {}
export function myFunction() {}
export interface MyInterface {}
"#;
    let (_dir, store) = setup_with_code(&[("types.ts", code)]);

    let classes = store.get_nodes_by_type("class").unwrap();
    assert!(!classes.is_empty(), "Should find class nodes");
    for node in &classes {
        assert_eq!(node.kind, NodeKind::Class);
    }

    let functions = store.get_nodes_by_type("function").unwrap();
    assert!(!functions.is_empty(), "Should find function nodes");
    for node in &functions {
        assert_eq!(node.kind, NodeKind::Function);
    }
}

// ===========================================================================
// 13. Get nodes by file path
// ===========================================================================

#[test]
fn get_nodes_by_file_returns_file_nodes() {
    let a_code = r#"export function alpha() {}"#;
    let b_code = r#"export function beta() {}"#;
    let (_dir, store) = setup_with_code(&[("a.ts", a_code), ("b.ts", b_code)]);

    let a_nodes = store.get_nodes_by_file("a.ts").unwrap();
    assert!(!a_nodes.is_empty(), "Should find nodes in a.ts");
    for node in &a_nodes {
        assert_eq!(node.file_path, "a.ts");
    }
}

// ===========================================================================
// 14. Dead code detection on indexed project
// ===========================================================================

#[test]
fn dead_code_on_indexed_project() {
    let used_code = r#"
export function usedHelper() { return 1; }
"#;
    let unused_code = r#"
function unusedInternal() { return 42; }
"#;
    let caller_code = r#"
import { usedHelper } from './used';
export function main() {
    return usedHelper();
}
"#;
    let files = &[
        ("used.ts", used_code),
        ("unused.ts", unused_code),
        ("app.ts", caller_code),
    ];
    let (_dir, store) = setup_with_code(files);

    let dead = find_dead_code(&store.conn, &[]);
    // At a minimum, unusedInternal should appear as dead code
    let names: Vec<&str> = dead.iter().map(|d| d.name.as_str()).collect();
    assert!(
        names.contains(&"unusedInternal"),
        "unusedInternal should be dead code, found: {:?}",
        names
    );
}

// ===========================================================================
// 15. Replace file data
// ===========================================================================

#[test]
fn replace_file_data_updates_graph() {
    let initial_code = r#"export function oldName() {}"#;
    let (_dir, store) = setup_with_code(&[("app.ts", initial_code)]);

    // Verify initial state
    let old_nodes = store.get_nodes_by_name("oldName").unwrap();
    assert!(!old_nodes.is_empty(), "Should find oldName initially");

    // Replace with new data
    let new_node = codegraph::types::CodeNode {
        id: "function:app.ts:newName:1".to_string(),
        name: "newName".to_string(),
        qualified_name: None,
        kind: NodeKind::Function,
        file_path: "app.ts".to_string(),
        start_line: 1,
        end_line: 1,
        start_column: 0,
        end_column: 30,
        language: Language::TypeScript,
        body: Some("export function newName() {}".to_string()),
        documentation: None,
        exported: Some(true),
    };
    store.replace_file_data("app.ts", &[new_node], &[]).unwrap();

    // Old name gone, new name present
    let old_after = store.get_nodes_by_name("oldName").unwrap();
    assert!(old_after.is_empty(), "oldName should be gone");
    let new_after = store.get_nodes_by_name("newName").unwrap();
    assert!(!new_after.is_empty(), "newName should be present");
}

// ===========================================================================
// 16. Unresolved references
// ===========================================================================

#[test]
fn unresolved_references_can_be_stored_and_queried() {
    let (_dir, store) = setup_with_code(&[("dummy.ts", "export function x() {}")]);

    store
        .insert_unresolved_ref("fn:dummy.ts:x:1", "nonexistent", "import", "dummy.ts", 5)
        .unwrap();

    let refs = store.get_unresolved_refs(None).unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].specifier, "nonexistent");
    assert_eq!(refs[0].ref_type, "import");
    assert_eq!(refs[0].file_path, "dummy.ts");
    assert_eq!(refs[0].line, 5);

    let count = store.get_unresolved_ref_count().unwrap();
    assert_eq!(count, 1);

    // Clear and verify
    store.clear_unresolved_refs_for_file("dummy.ts").unwrap();
    let count_after = store.get_unresolved_ref_count().unwrap();
    assert_eq!(count_after, 0);
}

// ===========================================================================
// 17. C file indexing
// ===========================================================================

#[test]
fn index_c_file_with_functions() {
    let code = r#"
#include <stdio.h>

void greet(const char* name) {
    printf("Hello, %s!\n", name);
}

int add(int a, int b) {
    return a + b;
}

int main() {
    greet("World");
    return add(1, 2);
}
"#;
    let (_dir, store) = setup_with_code(&[("main.c", code)]);
    let stats = store.get_stats().unwrap();
    assert!(
        stats.nodes >= 2,
        "Expected >= 2 nodes from C file, got {}",
        stats.nodes
    );
}

// ===========================================================================
// 18. C++ file indexing
// ===========================================================================

#[test]
fn index_cpp_file_with_class() {
    let code = r#"
#include <string>
#include <vector>

class Container {
public:
    void add(const std::string& item) {
        items.push_back(item);
    }

    size_t size() const {
        return items.size();
    }

private:
    std::vector<std::string> items;
};
"#;
    let (_dir, store) = setup_with_code(&[("container.cpp", code)]);
    let stats = store.get_stats().unwrap();
    assert!(
        stats.nodes >= 1,
        "Expected >= 1 node from C++ file, got {}",
        stats.nodes
    );
}

// ===========================================================================
// 19. PHP file indexing
// ===========================================================================

#[test]
fn index_php_file_with_class() {
    let code = r#"<?php

class UserRepository {
    public function find(int $id): ?User {
        return null;
    }

    public function save(User $user): void {
        // persist
    }
}

function helper(): string {
    return "help";
}
"#;
    let (_dir, store) = setup_with_code(&[("UserRepository.php", code)]);
    let stats = store.get_stats().unwrap();
    assert!(
        stats.nodes >= 1,
        "Expected >= 1 node from PHP file, got {}",
        stats.nodes
    );
}

// ===========================================================================
// 20. Ruby file indexing
// ===========================================================================

#[test]
fn index_ruby_file_with_class_and_methods() {
    let code = r#"
class Greeter
  def initialize(name)
    @name = name
  end

  def greet
    "Hello, #{@name}!"
  end

  def self.default
    new("World")
  end
end

def standalone_function
  42
end
"#;
    let (_dir, store) = setup_with_code(&[("greeter.rb", code)]);
    let stats = store.get_stats().unwrap();
    assert!(
        stats.nodes >= 1,
        "Expected >= 1 node from Ruby file, got {}",
        stats.nodes
    );
}

// ===========================================================================
// 21. Swift file indexing
// ===========================================================================

#[test]
fn index_swift_file_with_struct_and_enum() {
    let code = r#"
struct Point {
    var x: Double
    var y: Double

    func distance(to other: Point) -> Double {
        let dx = x - other.x
        let dy = y - other.y
        return (dx * dx + dy * dy).squareRoot()
    }
}

enum Direction {
    case north
    case south
    case east
    case west
}

func navigate(direction: Direction) -> String {
    switch direction {
    case .north: return "Going north"
    case .south: return "Going south"
    case .east: return "Going east"
    case .west: return "Going west"
    }
}
"#;
    let (_dir, store) = setup_with_code(&[("geometry.swift", code)]);
    let stats = store.get_stats().unwrap();
    assert!(
        stats.nodes >= 1,
        "Expected >= 1 node from Swift file, got {}",
        stats.nodes
    );
}

// ===========================================================================
// 22. Kotlin file indexing
// ===========================================================================

#[test]
fn index_kotlin_file_with_class() {
    let code = r#"
data class User(val name: String, val age: Int)

class UserService {
    fun findUser(name: String): User? {
        return null
    }

    fun createUser(name: String, age: Int): User {
        return User(name, age)
    }
}

fun greet(user: User): String {
    return "Hello, ${user.name}!"
}
"#;
    let (_dir, store) = setup_with_code(&[("User.kt", code)]);
    let stats = store.get_stats().unwrap();
    assert!(
        stats.nodes >= 1,
        "Expected >= 1 node from Kotlin file, got {}",
        stats.nodes
    );
}

// ===========================================================================
// 23. C# file indexing
// ===========================================================================

#[test]
fn index_csharp_file_with_class() {
    let code = r#"
using System;

namespace Example
{
    public class Calculator
    {
        public int Add(int a, int b)
        {
            return a + b;
        }

        public int Multiply(int a, int b)
        {
            return a * b;
        }
    }
}
"#;
    let (_dir, store) = setup_with_code(&[("Calculator.cs", code)]);
    let stats = store.get_stats().unwrap();
    assert!(
        stats.nodes >= 1,
        "Expected >= 1 node from C# file, got {}",
        stats.nodes
    );
}

// ===========================================================================
// 24. JavaScript (JSX) file indexing
// ===========================================================================

#[test]
fn index_jsx_file() {
    let code = r#"
import React from 'react';

function App() {
    return <div>Hello World</div>;
}

export default App;
"#;
    let (_dir, store) = setup_with_code(&[("App.jsx", code)]);
    let stats = store.get_stats().unwrap();
    assert!(
        stats.nodes >= 1,
        "Expected >= 1 node from JSX file, got {}",
        stats.nodes
    );
}

// ===========================================================================
// 25. TSX file indexing
// ===========================================================================

#[test]
fn index_tsx_file() {
    let code = r#"
import React from 'react';

interface Props {
    name: string;
}

export const Greeting: React.FC<Props> = ({ name }) => {
    return <h1>Hello, {name}!</h1>;
};
"#;
    let (_dir, store) = setup_with_code(&[("Greeting.tsx", code)]);
    let stats = store.get_stats().unwrap();
    assert!(
        stats.nodes >= 1,
        "Expected >= 1 node from TSX file, got {}",
        stats.nodes
    );
}

// ===========================================================================
// 26. Node upsert is idempotent
// ===========================================================================

#[test]
fn upsert_same_node_twice_is_idempotent() {
    let code = r#"export function stable() { return 1; }"#;
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("stable.ts"), code).unwrap();

    let conn = initialize_database(":memory:").unwrap();
    let store = GraphStore::from_connection(conn);
    let pipeline = IndexingPipeline::new(&store);

    // Index twice (full, not incremental)
    for _ in 0..2 {
        pipeline
            .index_directory(&IndexOptions {
                root_dir: dir.path().to_path_buf(),
                incremental: false,
            })
            .unwrap();
    }

    let nodes = store.get_nodes_by_name("stable").unwrap();
    assert_eq!(nodes.len(), 1, "Upsert should not create duplicates");
}

// ===========================================================================
// 27. Delete file nodes
// ===========================================================================

#[test]
fn delete_file_nodes_clears_graph() {
    let (_dir, store) = setup_with_code(&[("app.ts", "export function toDelete() {}")]);

    let before = store.get_node_count().unwrap();
    assert!(before >= 1);

    store.delete_file_nodes("app.ts").unwrap();
    let after = store.get_node_count().unwrap();
    assert_eq!(after, 0, "All nodes from the file should be deleted");
}

// ===========================================================================
// 28. Deeply nested directory structure
// ===========================================================================

#[test]
fn index_deeply_nested_files() {
    let code = r#"export function deep() { return "nested"; }"#;
    let (_dir, store) = setup_with_code(&[("a/b/c/d/e/deep.ts", code)]);
    let stats = store.get_stats().unwrap();
    assert!(stats.nodes >= 1, "Should index file in deep directories");
}

// ===========================================================================
// 29. Multiple files same directory
// ===========================================================================

#[test]
fn multiple_files_in_same_directory() {
    let files: Vec<(&str, &str)> = (0..5)
        .map(|i| {
            // Using static strings for the closure
            match i {
                0 => ("src/a.ts", "export function a() {}"),
                1 => ("src/b.ts", "export function b() {}"),
                2 => ("src/c.ts", "export function c() {}"),
                3 => ("src/d.ts", "export function d() {}"),
                4 => ("src/e.ts", "export function e() {}"),
                _ => unreachable!(),
            }
        })
        .collect();
    let (_dir, store) = setup_with_code(&files);
    let stats = store.get_stats().unwrap();
    assert!(stats.files >= 5, "Expected >= 5 files, got {}", stats.files);
}

// ===========================================================================
// 30. Edge retrieval (in/out edges)
// ===========================================================================

#[test]
fn edge_retrieval_by_node_id() {
    let code1 = r#"
export function caller() {
    return callee();
}
"#;
    let code2 = r#"
export function callee() {
    return 42;
}
"#;
    let (_dir, store) = setup_with_code(&[("caller.ts", code1), ("callee.ts", code2)]);

    let all_edges = store.get_all_edges().unwrap();
    if !all_edges.is_empty() {
        let first = &all_edges[0];
        let out_edges = store.get_out_edges(&first.source, None).unwrap();
        assert!(!out_edges.is_empty(), "Should find outgoing edges");
    }
}

// ===========================================================================
// 31. Graph stats consistency
// ===========================================================================

#[test]
fn graph_stats_are_consistent() {
    let code = r#"
export class A { method() {} }
export function b() { return new A().method(); }
export interface C {}
"#;
    let (_dir, store) = setup_with_code(&[("src.ts", code)]);

    let stats = store.get_stats().unwrap();
    let node_count = store.get_node_count().unwrap();
    let edge_count = store.get_edge_count().unwrap();
    let file_count = store.get_file_count().unwrap();

    assert_eq!(stats.nodes, node_count);
    assert_eq!(stats.edges, edge_count);
    assert_eq!(stats.files, file_count);
}

// ===========================================================================
// 32. Index result reports correct counts
// ===========================================================================

#[test]
fn index_result_reports_correct_counts() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("a.ts"), "export function a() {}").unwrap();
    std::fs::write(dir.path().join("b.py"), "def b(): pass").unwrap();

    let conn = initialize_database(":memory:").unwrap();
    let store = GraphStore::from_connection(conn);
    let pipeline = IndexingPipeline::new(&store);
    let result = pipeline
        .index_directory(&IndexOptions {
            root_dir: dir.path().to_path_buf(),
            incremental: false,
        })
        .unwrap();

    assert_eq!(result.files_indexed, 2);
    assert!(result.nodes_created >= 2);
    assert!(
        result.duration_ms < 60_000,
        "Should finish within 60 seconds"
    );
}

// ===========================================================================
// 33. Qualified names for methods
// ===========================================================================

#[test]
fn qualified_names_for_class_methods() {
    let code = r#"
export class UserService {
    findUser(id: string) {
        return null;
    }

    deleteUser(id: string) {
        return false;
    }
}
"#;
    let (_dir, store) = setup_with_code(&[("service.ts", code)]);

    let all_nodes = store.get_all_nodes().unwrap();
    let methods: Vec<_> = all_nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Method)
        .collect();

    // Methods should have qualified names like UserService.findUser
    for method in &methods {
        if let Some(ref qn) = method.qualified_name {
            assert!(
                qn.contains('.'),
                "Qualified name should contain dot: {}",
                qn
            );
        }
    }
}

// ===========================================================================
// 34. FTS search works after indexing
// ===========================================================================

#[test]
fn fts_search_after_indexing() {
    let code = r#"
export function authenticateUser(username: string, password: string): boolean {
    return username === "admin" && password === "secret";
}
"#;
    let (_dir, store) = setup_with_code(&[("auth.ts", code)]);

    // FTS should find the function
    let count: i64 = store
        .conn
        .query_row(
            "SELECT COUNT(*) FROM fts_nodes WHERE fts_nodes MATCH 'authenticateUser'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(count >= 1, "FTS should find authenticateUser");
}

// ===========================================================================
// 35. Batch upsert nodes
// ===========================================================================

#[test]
fn batch_upsert_nodes_in_transaction() {
    let conn = initialize_database(":memory:").unwrap();
    let store = GraphStore::from_connection(conn);

    let nodes: Vec<codegraph::types::CodeNode> = (0..100)
        .map(|i| codegraph::types::CodeNode {
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
            body: Some(format!("function fn{}() {{}}", i)),
            documentation: None,
            exported: Some(true),
        })
        .collect();

    store.upsert_nodes(&nodes).unwrap();
    let count = store.get_node_count().unwrap();
    assert_eq!(count, 100, "Should have exactly 100 nodes");
}

// ===========================================================================
// 36. Batch upsert edges
// ===========================================================================

#[test]
fn batch_upsert_edges_in_transaction() {
    let conn = initialize_database(":memory:").unwrap();
    let store = GraphStore::from_connection(conn);

    // Create nodes first
    let nodes: Vec<codegraph::types::CodeNode> = (0..10)
        .map(|i| codegraph::types::CodeNode {
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
    store.upsert_nodes(&nodes).unwrap();

    // Create edges between consecutive nodes
    let edges: Vec<codegraph::types::CodeEdge> = (0..9)
        .map(|i| codegraph::types::CodeEdge {
            source: format!("function:test.ts:fn{}:{}", i, i),
            target: format!("function:test.ts:fn{}:{}", i + 1, i + 1),
            kind: codegraph::types::EdgeKind::Calls,
            file_path: "test.ts".to_string(),
            line: i as u32,
            metadata: None,
        })
        .collect();
    store.upsert_edges(&edges).unwrap();

    let edge_count = store.get_edge_count().unwrap();
    assert_eq!(edge_count, 9, "Should have exactly 9 edges");
}

// ===========================================================================
// 37. Get node by ID
// ===========================================================================

#[test]
fn get_node_by_id() {
    let (_dir, store) = setup_with_code(&[("hello.ts", "export function hello() { return 1; }")]);

    let all = store.get_all_nodes().unwrap();
    assert!(!all.is_empty());

    let first = &all[0];
    let fetched = store.get_node(&first.id).unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().name, first.name);
}

#[test]
fn get_node_by_nonexistent_id_returns_none() {
    let (_dir, store) = setup_with_code(&[("hello.ts", "export function hello() { return 1; }")]);
    let fetched = store.get_node("nonexistent:id:here:99").unwrap();
    assert!(fetched.is_none());
}

// ===========================================================================
// 38. Multiple languages edge detection
// ===========================================================================

#[test]
fn python_function_calls_generate_edges() {
    let code = r#"
def caller():
    return helper()

def helper():
    return 42
"#;
    let (_dir, store) = setup_with_code(&[("app.py", code)]);
    let stats = store.get_stats().unwrap();
    assert!(stats.nodes >= 2, "Expected at least 2 Python functions");
    // Calls edge should be present
    assert!(
        stats.edges >= 1,
        "Expected call edges from caller to helper"
    );
}

// ===========================================================================
// 39. Eval project fixture (if available)
// ===========================================================================

#[test]
fn eval_fixture_indexes_when_present() {
    let fixture_path = PathBuf::from("tests/fixtures/eval-project");
    if !fixture_path.exists() {
        eprintln!("Skipping: eval-project fixture not found");
        return;
    }

    let conn = initialize_database(":memory:").unwrap();
    let store = GraphStore::from_connection(conn);
    let pipeline = IndexingPipeline::new(&store);
    let result = pipeline
        .index_directory(&IndexOptions {
            root_dir: fixture_path,
            incremental: false,
        })
        .unwrap();

    assert!(
        result.files_indexed >= 10,
        "Eval project should have >= 10 files"
    );
    assert!(
        result.nodes_created >= 30,
        "Eval project should have >= 30 nodes"
    );
}

// ===========================================================================
// 40. Gitignored files are skipped
// ===========================================================================

#[test]
fn gitignored_files_are_skipped() {
    let dir = TempDir::new().unwrap();

    // Initialize git repo so .gitignore is respected by the `ignore` crate
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .ok();

    // Create .gitignore
    std::fs::write(dir.path().join(".gitignore"), "node_modules/\n").unwrap();
    // Create an ignored file
    std::fs::create_dir_all(dir.path().join("node_modules")).unwrap();
    std::fs::write(
        dir.path().join("node_modules/dep.ts"),
        "export function dep() {}",
    )
    .unwrap();
    // Create a tracked file
    std::fs::write(dir.path().join("app.ts"), "export function app() {}").unwrap();

    let conn = initialize_database(":memory:").unwrap();
    let store = GraphStore::from_connection(conn);
    let pipeline = IndexingPipeline::new(&store);
    let _result = pipeline
        .index_directory(&IndexOptions {
            root_dir: dir.path().to_path_buf(),
            incremental: false,
        })
        .unwrap();

    // app.ts should be indexed
    let app_nodes = store.get_nodes_by_name("app").unwrap();
    assert!(!app_nodes.is_empty(), "app.ts should be indexed");

    // node_modules should be skipped (the `ignore` crate respects .gitignore)
    let dep_nodes = store.get_nodes_by_name("dep").unwrap();
    assert!(
        dep_nodes.is_empty(),
        "node_modules files should not be indexed"
    );
}

// ===========================================================================
// 41-50. Additional integration tests
// ===========================================================================

#[test]
fn index_javascript_module_exports() {
    let code = r#"
function privateHelper() {
    return 42;
}

module.exports = {
    getAnswer: function() {
        return privateHelper();
    }
};
"#;
    let (_dir, store) = setup_with_code(&[("lib.js", code)]);
    let stats = store.get_stats().unwrap();
    assert!(stats.nodes >= 1, "Should index JS file");
}

#[test]
fn index_typescript_enum() {
    let code = r#"
export enum Color {
    Red = "RED",
    Green = "GREEN",
    Blue = "BLUE",
}

export function getColor(c: Color): string {
    return c;
}
"#;
    let (_dir, store) = setup_with_code(&[("colors.ts", code)]);
    let stats = store.get_stats().unwrap();
    assert!(stats.nodes >= 2, "Should index enum and function");
}

#[test]
fn index_typescript_type_alias() {
    let code = r#"
export type UserId = string;
export type UserMap = Map<UserId, User>;
export interface User {
    id: UserId;
    name: string;
}
"#;
    let (_dir, store) = setup_with_code(&[("types.ts", code)]);
    let stats = store.get_stats().unwrap();
    assert!(stats.nodes >= 1, "Should index type aliases");
}

#[test]
fn index_python_class_with_decorators() {
    let code = r#"
class Meta(type):
    pass

class Base(metaclass=Meta):
    def base_method(self):
        pass

class Derived(Base):
    def derived_method(self):
        return self.base_method()
"#;
    let (_dir, store) = setup_with_code(&[("classes.py", code)]);
    let stats = store.get_stats().unwrap();
    assert!(stats.nodes >= 2, "Should index Python classes");
}

#[test]
fn index_go_interface() {
    let code = r#"
package main

type Writer interface {
    Write(data []byte) (int, error)
}

type FileWriter struct {
    path string
}

func (f *FileWriter) Write(data []byte) (int, error) {
    return len(data), nil
}
"#;
    let (_dir, store) = setup_with_code(&[("writer.go", code)]);
    let stats = store.get_stats().unwrap();
    assert!(stats.nodes >= 1, "Should index Go interfaces and structs");
}

#[test]
fn index_rust_enum_and_trait() {
    let code = r#"
pub enum Status {
    Active,
    Inactive,
    Pending,
}

pub trait Statusable {
    fn status(&self) -> Status;
    fn is_active(&self) -> bool {
        matches!(self.status(), Status::Active)
    }
}
"#;
    let (_dir, store) = setup_with_code(&[("status.rs", code)]);
    let stats = store.get_stats().unwrap();
    assert!(stats.nodes >= 1, "Should index Rust enum and trait");
}

#[test]
fn index_large_file_with_many_symbols() {
    // Generate a TypeScript file with many functions
    let mut code = String::new();
    for i in 0..50 {
        code.push_str(&format!("export function fn{}() {{ return {}; }}\n", i, i));
    }
    let (_dir, store) = setup_with_code(&[("many.ts", &code)]);
    let stats = store.get_stats().unwrap();
    assert!(
        stats.nodes >= 50,
        "Should index all 50 functions, got {}",
        stats.nodes
    );
}

#[test]
fn file_display_format() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("a.ts"), "export function a() {}").unwrap();

    let conn = initialize_database(":memory:").unwrap();
    let store = GraphStore::from_connection(conn);
    let pipeline = IndexingPipeline::new(&store);
    let result = pipeline
        .index_directory(&IndexOptions {
            root_dir: dir.path().to_path_buf(),
            incremental: false,
        })
        .unwrap();

    // Display trait should work
    let display = format!("{}", result);
    assert!(display.contains("Indexed"));
    assert!(display.contains("nodes"));
    assert!(display.contains("edges"));
}

#[test]
fn index_empty_source_file() {
    let (_dir, store) = setup_with_code(&[("empty.ts", "")]);
    let stats = store.get_stats().unwrap();
    // Empty file should produce 0 nodes
    assert_eq!(stats.nodes, 0);
}

#[test]
fn index_file_with_only_comments() {
    let code = r#"
// This file is intentionally left mostly empty
// It only has comments
/* Block comment */
"#;
    let (_dir, store) = setup_with_code(&[("comments.ts", code)]);
    let stats = store.get_stats().unwrap();
    // Comments-only file should produce 0 nodes
    assert_eq!(stats.nodes, 0);
}

// ===========================================================================
// 51-55. Graph operations
// ===========================================================================

#[test]
fn get_all_nodes_returns_all() {
    let code = r#"
export function a() {}
export function b() {}
export function c() {}
"#;
    let (_dir, store) = setup_with_code(&[("fns.ts", code)]);
    let all = store.get_all_nodes().unwrap();
    let names: Vec<&str> = all.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"a"));
    assert!(names.contains(&"b"));
    assert!(names.contains(&"c"));
}

#[test]
fn get_all_edges_returns_all() {
    let conn = initialize_database(":memory:").unwrap();
    let store = GraphStore::from_connection(conn);

    let nodes = vec![
        codegraph::types::CodeNode {
            id: "fn:t.ts:a:1".to_string(),
            name: "a".to_string(),
            qualified_name: None,
            kind: NodeKind::Function,
            file_path: "t.ts".to_string(),
            start_line: 1,
            end_line: 5,
            start_column: 0,
            end_column: 10,
            language: Language::TypeScript,
            body: None,
            documentation: None,
            exported: None,
        },
        codegraph::types::CodeNode {
            id: "fn:t.ts:b:10".to_string(),
            name: "b".to_string(),
            qualified_name: None,
            kind: NodeKind::Function,
            file_path: "t.ts".to_string(),
            start_line: 10,
            end_line: 15,
            start_column: 0,
            end_column: 10,
            language: Language::TypeScript,
            body: None,
            documentation: None,
            exported: None,
        },
    ];
    store.upsert_nodes(&nodes).unwrap();

    let edges = vec![codegraph::types::CodeEdge {
        source: "fn:t.ts:a:1".to_string(),
        target: "fn:t.ts:b:10".to_string(),
        kind: codegraph::types::EdgeKind::Calls,
        file_path: "t.ts".to_string(),
        line: 3,
        metadata: None,
    }];
    store.upsert_edges(&edges).unwrap();

    let all_edges = store.get_all_edges().unwrap();
    assert_eq!(all_edges.len(), 1);
    assert_eq!(all_edges[0].source, "fn:t.ts:a:1");
    assert_eq!(all_edges[0].target, "fn:t.ts:b:10");
}

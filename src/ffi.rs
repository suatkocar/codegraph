//! C FFI for CodeGraph — enables Node.js (napi-rs), Python (PyO3), and C bindings.
//!
//! All functions use `extern "C"` calling convention with opaque handles and
//! JSON-encoded return values. Every function catches panics to prevent
//! unwinding across the FFI boundary. Errors are reported via a thread-local
//! `last_error` string retrievable with [`codegraph_last_error`].
//!
//! # Lifecycle
//!
//! ```c
//! CodeGraphHandle db = codegraph_open("/path/to/.codegraph/graph.db");
//! if (!db) {
//!     char* err = codegraph_last_error();
//!     fprintf(stderr, "open failed: %s\n", err);
//!     codegraph_free_string(err);
//!     return 1;
//! }
//!
//! char* json = codegraph_query(db, "getUserById", 10);
//! // ... use json ...
//! codegraph_free_string(json);
//!
//! codegraph_close(db);
//! ```
//!
//! # Memory
//!
//! All `*mut c_char` return values are heap-allocated by Rust. The caller
//! **must** free them with [`codegraph_free_string`]. Passing any other
//! allocator's pointer to `codegraph_free_string` is undefined behavior.

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic;
use std::ptr;

use crate::graph::search::{HybridSearch, SearchOptions};
use crate::graph::store::GraphStore;
use crate::graph::traversal::GraphTraversal;

// ---------------------------------------------------------------------------
// Opaque handle
// ---------------------------------------------------------------------------

/// Opaque handle to a CodeGraph database. Internally a `Box<GraphStore>`.
pub type CodeGraphHandle = *mut std::ffi::c_void;

// ---------------------------------------------------------------------------
// Thread-local error
// ---------------------------------------------------------------------------

thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Set the thread-local error message.
fn set_last_error(msg: String) {
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = Some(msg);
    });
}

/// Clear the thread-local error.
fn clear_last_error() {
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a Rust `String` into a heap-allocated C string, or null on failure.
fn string_to_c(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(cs) => cs.into_raw(),
        Err(e) => {
            set_last_error(format!("CString conversion error: {e}"));
            ptr::null_mut()
        }
    }
}

/// Safely read a C string pointer into a Rust `&str`, returning `None` if null.
///
/// # Safety
///
/// The caller must ensure `ptr` points to a valid, null-terminated C string
/// (or is null).
unsafe fn c_str_to_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().ok()
}

/// Convert the `CodeGraphHandle` back to a `&GraphStore` reference.
///
/// # Safety
///
/// The handle must have been created by [`codegraph_open`] and not yet closed.
unsafe fn handle_to_store<'a>(handle: CodeGraphHandle) -> Option<&'a GraphStore> {
    if handle.is_null() {
        set_last_error("null handle".to_string());
        return None;
    }
    Some(unsafe { &*(handle as *const GraphStore) })
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Open a CodeGraph database at `db_path`.
///
/// Returns an opaque handle on success, or null on failure (check
/// [`codegraph_last_error`]).
///
/// # Safety
///
/// `db_path` must be a valid, null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn codegraph_open(db_path: *const c_char) -> CodeGraphHandle {
    clear_last_error();

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let path = match unsafe { c_str_to_str(db_path) } {
            Some(p) => p,
            None => {
                set_last_error("db_path is null or invalid UTF-8".to_string());
                return ptr::null_mut();
            }
        };

        match GraphStore::new(path) {
            Ok(store) => {
                let boxed = Box::new(store);
                Box::into_raw(boxed) as CodeGraphHandle
            }
            Err(e) => {
                set_last_error(format!("failed to open database: {e}"));
                ptr::null_mut()
            }
        }
    }));

    match result {
        Ok(handle) => handle,
        Err(_) => {
            set_last_error("panic in codegraph_open".to_string());
            ptr::null_mut()
        }
    }
}

/// Search the code graph with a hybrid (FTS5 + vector) query.
///
/// Returns a JSON array of search results, or null on error.
/// The caller must free the returned string with [`codegraph_free_string`].
///
/// # Safety
///
/// - `handle` must be a valid handle from [`codegraph_open`].
/// - `query` must be a valid, null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn codegraph_query(
    handle: CodeGraphHandle,
    query: *const c_char,
    limit: i32,
) -> *mut c_char {
    clear_last_error();

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let store = match unsafe { handle_to_store(handle) } {
            Some(s) => s,
            None => return ptr::null_mut(),
        };
        let q = match unsafe { c_str_to_str(query) } {
            Some(s) => s,
            None => {
                set_last_error("query is null or invalid UTF-8".to_string());
                return ptr::null_mut();
            }
        };

        let search = HybridSearch::new(&store.conn);
        let opts = SearchOptions {
            limit: Some(limit.max(1) as usize),
            ..Default::default()
        };

        match search.search(q, &opts) {
            Ok(results) => match serde_json::to_string(&results) {
                Ok(json) => string_to_c(json),
                Err(e) => {
                    set_last_error(format!("JSON serialization error: {e}"));
                    ptr::null_mut()
                }
            },
            Err(e) => {
                set_last_error(format!("search error: {e}"));
                ptr::null_mut()
            }
        }
    }));

    match result {
        Ok(ptr) => ptr,
        Err(_) => {
            set_last_error("panic in codegraph_query".to_string());
            ptr::null_mut()
        }
    }
}

/// Find callers (reverse call graph) of a symbol, up to `depth` levels.
///
/// `symbol` should be a node ID (e.g. "function:src/main.ts:hello:1").
/// Returns a JSON array of `{node, depth}` objects, or null on error.
///
/// # Safety
///
/// - `handle` must be a valid handle from [`codegraph_open`].
/// - `symbol` must be a valid, null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn codegraph_callers(
    handle: CodeGraphHandle,
    symbol: *const c_char,
    depth: i32,
) -> *mut c_char {
    clear_last_error();

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let store = match unsafe { handle_to_store(handle) } {
            Some(s) => s,
            None => return ptr::null_mut(),
        };
        let sym = match unsafe { c_str_to_str(symbol) } {
            Some(s) => s,
            None => {
                set_last_error("symbol is null or invalid UTF-8".to_string());
                return ptr::null_mut();
            }
        };

        let traversal = GraphTraversal::new(store);
        match traversal.find_callers(sym, depth.max(1) as u32) {
            Ok(results) => {
                // Serialize as [{node: {...}, depth: N}, ...]
                let serializable: Vec<serde_json::Value> = results
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "node": r.node,
                            "depth": r.depth,
                        })
                    })
                    .collect();
                match serde_json::to_string(&serializable) {
                    Ok(json) => string_to_c(json),
                    Err(e) => {
                        set_last_error(format!("JSON serialization error: {e}"));
                        ptr::null_mut()
                    }
                }
            }
            Err(e) => {
                set_last_error(format!("callers error: {e}"));
                ptr::null_mut()
            }
        }
    }));

    match result {
        Ok(ptr) => ptr,
        Err(_) => {
            set_last_error("panic in codegraph_callers".to_string());
            ptr::null_mut()
        }
    }
}

/// Find dependencies (forward traversal) of a symbol, up to `depth` levels.
///
/// Returns a JSON array of `{node, depth}` objects, or null on error.
///
/// # Safety
///
/// - `handle` must be a valid handle from [`codegraph_open`].
/// - `symbol` must be a valid, null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn codegraph_dependencies(
    handle: CodeGraphHandle,
    symbol: *const c_char,
    depth: i32,
) -> *mut c_char {
    clear_last_error();

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let store = match unsafe { handle_to_store(handle) } {
            Some(s) => s,
            None => return ptr::null_mut(),
        };
        let sym = match unsafe { c_str_to_str(symbol) } {
            Some(s) => s,
            None => {
                set_last_error("symbol is null or invalid UTF-8".to_string());
                return ptr::null_mut();
            }
        };

        let traversal = GraphTraversal::new(store);
        match traversal.find_dependencies(sym, depth.max(1) as u32) {
            Ok(results) => {
                let serializable: Vec<serde_json::Value> = results
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "node": r.node,
                            "depth": r.depth,
                        })
                    })
                    .collect();
                match serde_json::to_string(&serializable) {
                    Ok(json) => string_to_c(json),
                    Err(e) => {
                        set_last_error(format!("JSON serialization error: {e}"));
                        ptr::null_mut()
                    }
                }
            }
            Err(e) => {
                set_last_error(format!("dependencies error: {e}"));
                ptr::null_mut()
            }
        }
    }));

    match result {
        Ok(ptr) => ptr,
        Err(_) => {
            set_last_error("panic in codegraph_dependencies".to_string());
            ptr::null_mut()
        }
    }
}

/// Look up a single node by its ID.
///
/// Returns a JSON object representing the node, or null if not found / on error.
///
/// # Safety
///
/// - `handle` must be a valid handle from [`codegraph_open`].
/// - `symbol` must be a valid, null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn codegraph_node(
    handle: CodeGraphHandle,
    symbol: *const c_char,
) -> *mut c_char {
    clear_last_error();

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let store = match unsafe { handle_to_store(handle) } {
            Some(s) => s,
            None => return ptr::null_mut(),
        };
        let sym = match unsafe { c_str_to_str(symbol) } {
            Some(s) => s,
            None => {
                set_last_error("symbol is null or invalid UTF-8".to_string());
                return ptr::null_mut();
            }
        };

        match store.get_node(sym) {
            Ok(Some(node)) => match serde_json::to_string(&node) {
                Ok(json) => string_to_c(json),
                Err(e) => {
                    set_last_error(format!("JSON serialization error: {e}"));
                    ptr::null_mut()
                }
            },
            Ok(None) => {
                set_last_error(format!("node not found: {sym}"));
                ptr::null_mut()
            }
            Err(e) => {
                set_last_error(format!("node lookup error: {e}"));
                ptr::null_mut()
            }
        }
    }));

    match result {
        Ok(ptr) => ptr,
        Err(_) => {
            set_last_error("panic in codegraph_node".to_string());
            ptr::null_mut()
        }
    }
}

/// Free a string previously returned by any `codegraph_*` function.
///
/// Passing null is a safe no-op. Passing a pointer not allocated by
/// CodeGraph is **undefined behavior**.
///
/// # Safety
///
/// `s` must be null or a pointer previously returned by a `codegraph_*`
/// function and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn codegraph_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

/// Close a CodeGraph handle, releasing all resources.
///
/// Passing null is a safe no-op. After this call, the handle is invalid
/// and must not be used again.
///
/// # Safety
///
/// `handle` must be null or a handle from [`codegraph_open`] not yet closed.
#[no_mangle]
pub unsafe extern "C" fn codegraph_close(handle: CodeGraphHandle) {
    if !handle.is_null() {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            drop(unsafe { Box::from_raw(handle as *mut GraphStore) });
        }));
    }
}

/// Retrieve the last error message, or null if no error occurred.
///
/// The caller must free the returned string with [`codegraph_free_string`].
/// Returns null if no error is set.
///
/// # Safety
///
/// This function is always safe to call.
#[no_mangle]
pub extern "C" fn codegraph_last_error() -> *mut c_char {
    LAST_ERROR.with(|cell| {
        let borrowed = cell.borrow();
        match borrowed.as_ref() {
            Some(msg) => string_to_c(msg.clone()),
            None => ptr::null_mut(),
        }
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    // -- Helper: read a C string returned by FFI and free it --

    unsafe fn read_and_free(ptr: *mut c_char) -> Option<String> {
        if ptr.is_null() {
            return None;
        }
        let s = unsafe { CStr::from_ptr(ptr) }.to_str().ok()?.to_string();
        unsafe { codegraph_free_string(ptr) };
        Some(s)
    }

    unsafe fn get_last_error() -> Option<String> {
        let ptr = codegraph_last_error();
        unsafe { read_and_free(ptr) }
    }

    // -- codegraph_open / codegraph_close --

    #[test]
    fn open_in_memory_and_close() {
        let path = CString::new(":memory:").unwrap();
        let handle = unsafe { codegraph_open(path.as_ptr()) };
        assert!(!handle.is_null(), "should open in-memory DB successfully");
        unsafe { codegraph_close(handle) };
    }

    #[test]
    fn open_null_path_returns_null() {
        let handle = unsafe { codegraph_open(ptr::null()) };
        assert!(handle.is_null());
        let err = unsafe { get_last_error() };
        assert!(err.is_some());
        assert!(err.unwrap().contains("null"));
    }

    #[test]
    fn close_null_is_safe() {
        unsafe { codegraph_close(ptr::null_mut()) };
        // Should not crash
    }

    // -- codegraph_query --

    #[test]
    fn query_on_empty_db() {
        let path = CString::new(":memory:").unwrap();
        let handle = unsafe { codegraph_open(path.as_ptr()) };
        assert!(!handle.is_null());

        let query = CString::new("nonexistent").unwrap();
        let result = unsafe { codegraph_query(handle, query.as_ptr(), 10) };
        // Should return a valid JSON (empty array)
        let json = unsafe { read_and_free(result) };
        assert!(json.is_some());
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json.unwrap()).unwrap();
        assert!(parsed.is_empty());

        unsafe { codegraph_close(handle) };
    }

    #[test]
    fn query_null_handle_returns_null() {
        let query = CString::new("test").unwrap();
        let result = unsafe { codegraph_query(ptr::null_mut(), query.as_ptr(), 10) };
        assert!(result.is_null());
        let err = unsafe { get_last_error() };
        assert!(err.unwrap().contains("null handle"));
    }

    #[test]
    fn query_null_query_returns_null() {
        let path = CString::new(":memory:").unwrap();
        let handle = unsafe { codegraph_open(path.as_ptr()) };
        assert!(!handle.is_null());

        let result = unsafe { codegraph_query(handle, ptr::null(), 10) };
        assert!(result.is_null());

        unsafe { codegraph_close(handle) };
    }

    // -- codegraph_callers --

    #[test]
    fn callers_on_empty_db() {
        let path = CString::new(":memory:").unwrap();
        let handle = unsafe { codegraph_open(path.as_ptr()) };
        assert!(!handle.is_null());

        let sym = CString::new("nonexistent_node").unwrap();
        let result = unsafe { codegraph_callers(handle, sym.as_ptr(), 3) };
        let json = unsafe { read_and_free(result) };
        assert!(json.is_some());
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json.unwrap()).unwrap();
        assert!(parsed.is_empty());

        unsafe { codegraph_close(handle) };
    }

    #[test]
    fn callers_null_handle() {
        let sym = CString::new("test").unwrap();
        let result = unsafe { codegraph_callers(ptr::null_mut(), sym.as_ptr(), 3) };
        assert!(result.is_null());
    }

    #[test]
    fn callers_null_symbol() {
        let path = CString::new(":memory:").unwrap();
        let handle = unsafe { codegraph_open(path.as_ptr()) };
        let result = unsafe { codegraph_callers(handle, ptr::null(), 3) };
        assert!(result.is_null());
        unsafe { codegraph_close(handle) };
    }

    // -- codegraph_dependencies --

    #[test]
    fn dependencies_on_empty_db() {
        let path = CString::new(":memory:").unwrap();
        let handle = unsafe { codegraph_open(path.as_ptr()) };
        assert!(!handle.is_null());

        let sym = CString::new("nonexistent_node").unwrap();
        let result = unsafe { codegraph_dependencies(handle, sym.as_ptr(), 3) };
        let json = unsafe { read_and_free(result) };
        assert!(json.is_some());
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json.unwrap()).unwrap();
        assert!(parsed.is_empty());

        unsafe { codegraph_close(handle) };
    }

    #[test]
    fn dependencies_null_handle() {
        let sym = CString::new("test").unwrap();
        let result = unsafe { codegraph_dependencies(ptr::null_mut(), sym.as_ptr(), 3) };
        assert!(result.is_null());
    }

    // -- codegraph_node --

    #[test]
    fn node_not_found_returns_null() {
        let path = CString::new(":memory:").unwrap();
        let handle = unsafe { codegraph_open(path.as_ptr()) };
        assert!(!handle.is_null());

        let sym = CString::new("nonexistent").unwrap();
        let result = unsafe { codegraph_node(handle, sym.as_ptr()) };
        assert!(result.is_null());
        let err = unsafe { get_last_error() };
        assert!(err.unwrap().contains("not found"));

        unsafe { codegraph_close(handle) };
    }

    #[test]
    fn node_null_handle() {
        let sym = CString::new("test").unwrap();
        let result = unsafe { codegraph_node(ptr::null_mut(), sym.as_ptr()) };
        assert!(result.is_null());
    }

    #[test]
    fn node_null_symbol() {
        let path = CString::new(":memory:").unwrap();
        let handle = unsafe { codegraph_open(path.as_ptr()) };
        let result = unsafe { codegraph_node(handle, ptr::null()) };
        assert!(result.is_null());
        unsafe { codegraph_close(handle) };
    }

    // -- codegraph_free_string --

    #[test]
    fn free_string_null_is_safe() {
        unsafe { codegraph_free_string(ptr::null_mut()) };
    }

    // -- codegraph_last_error --

    #[test]
    fn last_error_none_initially() {
        clear_last_error();
        let err = codegraph_last_error();
        assert!(err.is_null());
    }

    #[test]
    fn last_error_round_trip() {
        set_last_error("test error".to_string());
        let err = unsafe { get_last_error() };
        assert_eq!(err.as_deref(), Some("test error"));
    }

    // -- Integration: insert data then query via FFI --

    #[test]
    fn query_finds_inserted_node() {
        use crate::db::schema::initialize_database;
        use crate::graph::store::GraphStore;
        use crate::types::{CodeNode, Language, NodeKind};

        // Build a store in-memory and insert a node
        let conn = initialize_database(":memory:").unwrap();
        let store = GraphStore::from_connection(conn);
        store
            .upsert_node(&CodeNode {
                id: "fn:app.ts:greet:1".to_string(),
                name: "greet".to_string(),
                qualified_name: None,
                kind: NodeKind::Function,
                file_path: "app.ts".to_string(),
                start_line: 1,
                end_line: 5,
                start_column: 0,
                end_column: 1,
                language: Language::TypeScript,
                body: Some("function greet() {}".to_string()),
                documentation: Some("Say hello".to_string()),
                exported: Some(true),
            })
            .unwrap();

        // Convert to handle
        let handle = Box::into_raw(Box::new(store)) as CodeGraphHandle;

        let query = CString::new("greet").unwrap();
        let result = unsafe { codegraph_query(handle, query.as_ptr(), 10) };
        assert!(!result.is_null(), "should find the inserted node");

        let json = unsafe { read_and_free(result) }.unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["name"], "greet");

        unsafe { codegraph_close(handle) };
    }

    #[test]
    fn node_lookup_finds_inserted_node() {
        use crate::db::schema::initialize_database;
        use crate::graph::store::GraphStore;
        use crate::types::{CodeNode, Language, NodeKind};

        let conn = initialize_database(":memory:").unwrap();
        let store = GraphStore::from_connection(conn);
        store
            .upsert_node(&CodeNode {
                id: "fn:app.ts:hello:1".to_string(),
                name: "hello".to_string(),
                qualified_name: None,
                kind: NodeKind::Function,
                file_path: "app.ts".to_string(),
                start_line: 1,
                end_line: 5,
                start_column: 0,
                end_column: 1,
                language: Language::TypeScript,
                body: None,
                documentation: None,
                exported: None,
            })
            .unwrap();

        let handle = Box::into_raw(Box::new(store)) as CodeGraphHandle;

        let sym = CString::new("fn:app.ts:hello:1").unwrap();
        let result = unsafe { codegraph_node(handle, sym.as_ptr()) };
        assert!(!result.is_null());

        let json = unsafe { read_and_free(result) }.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["name"], "hello");
        assert_eq!(parsed["kind"], "function");

        unsafe { codegraph_close(handle) };
    }

    #[test]
    fn callers_finds_call_edge() {
        use crate::db::schema::initialize_database;
        use crate::graph::store::GraphStore;
        use crate::types::{CodeEdge, CodeNode, EdgeKind, Language, NodeKind};

        let conn = initialize_database(":memory:").unwrap();
        let store = GraphStore::from_connection(conn);

        let caller = CodeNode {
            id: "fn:a.ts:main:1".to_string(),
            name: "main".to_string(),
            qualified_name: None,
            kind: NodeKind::Function,
            file_path: "a.ts".to_string(),
            start_line: 1,
            end_line: 10,
            start_column: 0,
            end_column: 1,
            language: Language::TypeScript,
            body: None,
            documentation: None,
            exported: None,
        };
        let callee = CodeNode {
            id: "fn:a.ts:helper:20".to_string(),
            name: "helper".to_string(),
            qualified_name: None,
            kind: NodeKind::Function,
            file_path: "a.ts".to_string(),
            start_line: 20,
            end_line: 25,
            start_column: 0,
            end_column: 1,
            language: Language::TypeScript,
            body: None,
            documentation: None,
            exported: None,
        };
        store.upsert_nodes(&[caller, callee]).unwrap();
        store
            .upsert_edge(&CodeEdge {
                source: "fn:a.ts:main:1".to_string(),
                target: "fn:a.ts:helper:20".to_string(),
                kind: EdgeKind::Calls,
                file_path: "a.ts".to_string(),
                line: 5,
                metadata: None,
            })
            .unwrap();

        let handle = Box::into_raw(Box::new(store)) as CodeGraphHandle;

        let sym = CString::new("fn:a.ts:helper:20").unwrap();
        let result = unsafe { codegraph_callers(handle, sym.as_ptr(), 3) };
        assert!(!result.is_null());

        let json = unsafe { read_and_free(result) }.unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["node"]["name"], "main");
        assert_eq!(parsed[0]["depth"], 1);

        unsafe { codegraph_close(handle) };
    }

    #[test]
    fn dependencies_finds_outgoing_edges() {
        use crate::db::schema::initialize_database;
        use crate::graph::store::GraphStore;
        use crate::types::{CodeEdge, CodeNode, EdgeKind, Language, NodeKind};

        let conn = initialize_database(":memory:").unwrap();
        let store = GraphStore::from_connection(conn);

        let src = CodeNode {
            id: "fn:a.ts:app:1".to_string(),
            name: "app".to_string(),
            qualified_name: None,
            kind: NodeKind::Function,
            file_path: "a.ts".to_string(),
            start_line: 1,
            end_line: 10,
            start_column: 0,
            end_column: 1,
            language: Language::TypeScript,
            body: None,
            documentation: None,
            exported: None,
        };
        let dep = CodeNode {
            id: "fn:b.ts:utils:1".to_string(),
            name: "utils".to_string(),
            qualified_name: None,
            kind: NodeKind::Function,
            file_path: "b.ts".to_string(),
            start_line: 1,
            end_line: 5,
            start_column: 0,
            end_column: 1,
            language: Language::TypeScript,
            body: None,
            documentation: None,
            exported: None,
        };
        store.upsert_nodes(&[src, dep]).unwrap();
        store
            .upsert_edge(&CodeEdge {
                source: "fn:a.ts:app:1".to_string(),
                target: "fn:b.ts:utils:1".to_string(),
                kind: EdgeKind::Imports,
                file_path: "a.ts".to_string(),
                line: 1,
                metadata: None,
            })
            .unwrap();

        let handle = Box::into_raw(Box::new(store)) as CodeGraphHandle;

        let sym = CString::new("fn:a.ts:app:1").unwrap();
        let result = unsafe { codegraph_dependencies(handle, sym.as_ptr(), 3) };
        assert!(!result.is_null());

        let json = unsafe { read_and_free(result) }.unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["node"]["name"], "utils");

        unsafe { codegraph_close(handle) };
    }
}

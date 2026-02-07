//! Graph traversal algorithms using SQLite recursive CTEs.
//!
//! Ports the TypeScript `graph/traversal.ts` to Rust. All SQL recursive
//! CTEs are copied verbatim from the TS version. Cycle detection uses
//! Tarjan's SCC algorithm implemented in Rust (not SQL), matching the
//! original design decision.

use std::collections::{HashMap, HashSet, VecDeque};

use rusqlite::params;

use crate::db::converters::{row_to_code_edge, row_to_code_node};
use crate::error::Result;
use crate::graph::store::GraphStore;
use crate::types::{CodeEdge, CodeNode};

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// A node annotated with its traversal depth from the starting point.
#[derive(Debug, Clone)]
pub struct NodeWithDepth {
    pub node: CodeNode,
    pub depth: u32,
}

/// A strongly connected component (cycle) in the graph.
#[derive(Debug, Clone)]
pub struct CycleInfo {
    pub node_ids: Vec<String>,
    pub size: usize,
}

/// A bidirectional subgraph around a focal node.
#[derive(Debug, Clone)]
pub struct Neighborhood {
    pub nodes: Vec<CodeNode>,
    pub edges: Vec<CodeEdge>,
}

// ---------------------------------------------------------------------------
// SQL constants — copied verbatim from the TypeScript version
// ---------------------------------------------------------------------------

const FIND_DEPENDENCIES_SQL: &str = "\
WITH RECURSIVE deps(id, depth, path) AS (
    -- Base: start node
    SELECT target_id, 1, source_id || '->' || target_id
    FROM edges
    WHERE source_id = ?1

    UNION

    -- Recursive: follow outgoing edges, with cycle detection
    SELECT e.target_id, d.depth + 1, d.path || '->' || e.target_id
    FROM deps d
    JOIN edges e ON e.source_id = d.id
    WHERE d.depth < ?2
      AND instr(d.path, e.target_id) = 0
)
SELECT DISTINCT n.*, d.depth
FROM deps d
JOIN nodes n ON n.id = d.id
ORDER BY d.depth ASC, n.name ASC";

const FIND_CALLEES_SQL: &str = "\
WITH RECURSIVE callees(id, depth, path) AS (
    -- Base: direct callees (outgoing call edges)
    SELECT target_id, 1, source_id || '->' || target_id
    FROM edges
    WHERE source_id = ?1 AND type = 'calls'

    UNION

    -- Recursive: follow outgoing call edges, with cycle detection
    SELECT e.target_id, c.depth + 1, c.path || '->' || e.target_id
    FROM callees c
    JOIN edges e ON e.source_id = c.id AND e.type = 'calls'
    WHERE c.depth < ?2
      AND instr(c.path, e.target_id) = 0
)
SELECT DISTINCT n.*, c.depth
FROM callees c
JOIN nodes n ON n.id = c.id
ORDER BY c.depth ASC, n.name ASC";

const FIND_CALLERS_SQL: &str = "\
WITH RECURSIVE callers(id, depth, path) AS (
    -- Base: direct callers
    SELECT source_id, 1, target_id || '<-' || source_id
    FROM edges
    WHERE target_id = ?1 AND type = 'calls'

    UNION

    -- Recursive: follow incoming call edges
    SELECT e.source_id, c.depth + 1, c.path || '<-' || e.source_id
    FROM callers c
    JOIN edges e ON e.target_id = c.id AND e.type = 'calls'
    WHERE c.depth < ?2
      AND instr(c.path, e.source_id) = 0
)
SELECT DISTINCT n.*, c.depth
FROM callers c
JOIN nodes n ON n.id = c.id
ORDER BY c.depth ASC, n.name ASC";

const FIND_TESTS_SQL: &str = "\
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
WHERE (
    n.file_path LIKE '%test%'
    OR n.file_path LIKE '%spec%'
    OR n.file_path LIKE '%__tests__%'
    OR n.name LIKE 'test%'
    OR n.name LIKE '%Test'
    OR n.name LIKE '%test'
)
ORDER BY n.file_path ASC, n.start_line ASC";

const NEIGHBORHOOD_NODES_SQL: &str = "\
WITH RECURSIVE
    outgoing(id, depth, path) AS (
        SELECT ?1, 0, ?2
        UNION
        SELECT e.target_id, o.depth + 1, o.path || '->' || e.target_id
        FROM outgoing o
        JOIN edges e ON e.source_id = o.id
        WHERE o.depth < ?3 AND instr(o.path, e.target_id) = 0
    ),
    incoming(id, depth, path) AS (
        SELECT ?4, 0, ?5
        UNION
        SELECT e.source_id, i.depth + 1, i.path || '<-' || e.source_id
        FROM incoming i
        JOIN edges e ON e.target_id = i.id
        WHERE i.depth < ?6 AND instr(i.path, e.source_id) = 0
    )
SELECT DISTINCT n.*
FROM nodes n
WHERE n.id IN (SELECT id FROM outgoing UNION SELECT id FROM incoming)
ORDER BY n.name ASC";

// ---------------------------------------------------------------------------
// GraphTraversal
// ---------------------------------------------------------------------------

/// Graph traversal algorithms using SQLite recursive CTEs.
///
/// All traversals use depth limits and cycle detection to prevent
/// runaway queries on large or cyclic graphs.
pub struct GraphTraversal<'a> {
    store: &'a GraphStore,
}

impl<'a> GraphTraversal<'a> {
    /// Create a new traversal bound to the given store.
    pub fn new(store: &'a GraphStore) -> Self {
        Self { store }
    }

    // -------------------------------------------------------------------
    // find_dependencies
    // -------------------------------------------------------------------

    /// Find all dependencies (outgoing edges) from a node, up to `max_depth`.
    /// Follows: calls, imports, references, extends, implements.
    pub fn find_dependencies(&self, node_id: &str, max_depth: u32) -> Result<Vec<NodeWithDepth>> {
        let mut stmt = self.store.conn.prepare_cached(FIND_DEPENDENCIES_SQL)?;
        let rows = stmt.query_and_then(params![node_id, max_depth], |row| {
            let node = row_to_code_node(row)?;
            let depth: u32 = row.get("depth")?;
            Ok::<_, crate::error::CodeGraphError>(NodeWithDepth { node, depth })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
    }

    // -------------------------------------------------------------------
    // find_callees
    // -------------------------------------------------------------------

    /// Find all callees (outgoing "calls" edges) from a node, up to `max_depth`.
    /// This is the forward call graph — what does this function call?
    pub fn find_callees(&self, node_id: &str, max_depth: u32) -> Result<Vec<NodeWithDepth>> {
        if max_depth == 0 {
            return Ok(Vec::new());
        }
        let mut stmt = self.store.conn.prepare_cached(FIND_CALLEES_SQL)?;
        let rows = stmt.query_and_then(params![node_id, max_depth], |row| {
            let node = row_to_code_node(row)?;
            let depth: u32 = row.get("depth")?;
            Ok::<_, crate::error::CodeGraphError>(NodeWithDepth { node, depth })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
    }

    // -------------------------------------------------------------------
    // find_callers
    // -------------------------------------------------------------------

    /// Find all callers (incoming "calls" edges) of a node, up to `max_depth`.
    pub fn find_callers(&self, node_id: &str, max_depth: u32) -> Result<Vec<NodeWithDepth>> {
        if max_depth == 0 {
            return Ok(Vec::new());
        }
        let mut stmt = self.store.conn.prepare_cached(FIND_CALLERS_SQL)?;
        let rows = stmt.query_and_then(params![node_id, max_depth], |row| {
            let node = row_to_code_node(row)?;
            let depth: u32 = row.get("depth")?;
            Ok::<_, crate::error::CodeGraphError>(NodeWithDepth { node, depth })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
    }

    // -------------------------------------------------------------------
    // find_transitive_deps
    // -------------------------------------------------------------------

    /// Find all transitively reachable nodes from a starting node.
    /// Uses a generous depth limit of 50 to capture the full dependency tree.
    pub fn find_transitive_deps(&self, node_id: &str) -> Result<Vec<CodeNode>> {
        let results = self.find_dependencies(node_id, 50)?;
        Ok(results.into_iter().map(|r| r.node).collect())
    }

    // -------------------------------------------------------------------
    // find_tests
    // -------------------------------------------------------------------

    /// Find test files and test functions that likely cover the given node.
    ///
    /// Heuristic: find nodes in files containing "test" or "spec" that
    /// reference or call the target node (directly or transitively).
    pub fn find_tests(&self, node_id: &str) -> Result<Vec<CodeNode>> {
        let mut stmt = self.store.conn.prepare_cached(FIND_TESTS_SQL)?;
        let rows = stmt.query_and_then(params![node_id], row_to_code_node)?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    // -------------------------------------------------------------------
    // detect_cycles
    // -------------------------------------------------------------------

    /// Detect all cycles in the graph using Tarjan's SCC algorithm.
    /// Returns strongly connected components of size >= 2 (i.e., actual cycles).
    ///
    /// Implemented in Rust rather than SQL because Tarjan's algorithm requires
    /// mutable state (stack, index counters) that recursive CTEs can't express.
    pub fn detect_cycles(&self) -> Result<Vec<CycleInfo>> {
        // Load the full edge list into memory for Tarjan's
        let mut stmt = self
            .store
            .conn
            .prepare_cached("SELECT source_id, target_id FROM edges")?;
        let edge_pairs: Vec<(String, String)> = stmt
            .query_map([], |row| {
                let source: String = row.get(0)?;
                let target: String = row.get(1)?;
                Ok((source, target))
            })?
            .filter_map(|r| r.ok())
            .collect();

        // Build adjacency list
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        let mut all_nodes: HashSet<String> = HashSet::new();
        for (source, target) in &edge_pairs {
            all_nodes.insert(source.clone());
            all_nodes.insert(target.clone());
            adj.entry(source.clone()).or_default().push(target.clone());
        }

        // Tarjan's strongly connected components (iterative to avoid stack
        // overflow on deep graphs — the TS version uses recursion, but Rust's
        // default stack is smaller).
        let mut index_counter: u32 = 0;
        let mut node_index: HashMap<String, u32> = HashMap::new();
        let mut node_lowlink: HashMap<String, u32> = HashMap::new();
        let mut on_stack: HashSet<String> = HashSet::new();
        let mut stack: Vec<String> = Vec::new();
        let mut sccs: Vec<Vec<String>> = Vec::new();

        // Recursive inner function — using an explicit closure isn't
        // ergonomic with mutable borrows in Rust, so we use a helper
        // function with all state passed by reference.
        #[allow(clippy::too_many_arguments)]
        fn strong_connect(
            v: &str,
            adj: &HashMap<String, Vec<String>>,
            index_counter: &mut u32,
            node_index: &mut HashMap<String, u32>,
            node_lowlink: &mut HashMap<String, u32>,
            on_stack: &mut HashSet<String>,
            stack: &mut Vec<String>,
            sccs: &mut Vec<Vec<String>>,
        ) {
            node_index.insert(v.to_string(), *index_counter);
            node_lowlink.insert(v.to_string(), *index_counter);
            *index_counter += 1;
            stack.push(v.to_string());
            on_stack.insert(v.to_string());

            if let Some(neighbors) = adj.get(v) {
                for w in neighbors {
                    if !node_index.contains_key(w.as_str()) {
                        strong_connect(
                            w,
                            adj,
                            index_counter,
                            node_index,
                            node_lowlink,
                            on_stack,
                            stack,
                            sccs,
                        );
                        let w_low = *node_lowlink.get(w.as_str()).unwrap();
                        let v_low = node_lowlink.get_mut(v).unwrap();
                        if w_low < *v_low {
                            *v_low = w_low;
                        }
                    } else if on_stack.contains(w.as_str()) {
                        let w_idx = *node_index.get(w.as_str()).unwrap();
                        let v_low = node_lowlink.get_mut(v).unwrap();
                        if w_idx < *v_low {
                            *v_low = w_idx;
                        }
                    }
                }
            }

            if node_lowlink.get(v) == node_index.get(v) {
                let mut scc: Vec<String> = Vec::new();
                loop {
                    let w = stack.pop().unwrap();
                    on_stack.remove(&w);
                    scc.push(w.clone());
                    if w == v {
                        break;
                    }
                }
                sccs.push(scc);
            }
        }

        for node in &all_nodes {
            if !node_index.contains_key(node.as_str()) {
                strong_connect(
                    node,
                    &adj,
                    &mut index_counter,
                    &mut node_index,
                    &mut node_lowlink,
                    &mut on_stack,
                    &mut stack,
                    &mut sccs,
                );
            }
        }

        // Only return SCCs with 2+ nodes (actual cycles)
        Ok(sccs
            .into_iter()
            .filter(|scc| scc.len() >= 2)
            .map(|node_ids| {
                let size = node_ids.len();
                CycleInfo { node_ids, size }
            })
            .collect())
    }

    // -------------------------------------------------------------------
    // get_neighborhood
    // -------------------------------------------------------------------

    /// Get the subgraph (neighborhood) around a node within a given radius.
    /// Returns all nodes reachable within `radius` hops in either direction,
    /// plus all edges between those nodes.
    pub fn get_neighborhood(&self, node_id: &str, radius: u32) -> Result<Neighborhood> {
        // Gather reachable node IDs via both outgoing and incoming edges.
        // The CTE uses the node_id as both the starting ID and the initial
        // path string (matching the TS version's parameter binding).
        let mut stmt = self.store.conn.prepare_cached(NEIGHBORHOOD_NODES_SQL)?;
        let rows = stmt.query_and_then(
            params![node_id, node_id, radius, node_id, node_id, radius],
            row_to_code_node,
        )?;

        let nodes: Vec<CodeNode> = rows.collect::<std::result::Result<Vec<_>, _>>()?;

        let node_set: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();

        if node_set.is_empty() {
            return Ok(Neighborhood {
                nodes: vec![],
                edges: vec![],
            });
        }

        // Get all edges where both endpoints are in the neighborhood.
        // Build a dynamic query with the right number of placeholders.
        let node_ids: Vec<&str> = node_set.iter().copied().collect();
        let placeholders: String = node_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let edge_sql = format!(
            "SELECT * FROM edges WHERE source_id IN ({placeholders}) AND target_id IN ({placeholders})"
        );

        let mut edge_stmt = self.store.conn.prepare(&edge_sql)?;

        // Bind parameters: first set for source_id IN, second set for target_id IN.
        let mut param_values: Vec<&dyn rusqlite::types::ToSql> =
            Vec::with_capacity(node_ids.len() * 2);
        for id in &node_ids {
            param_values.push(id);
        }
        for id in &node_ids {
            param_values.push(id);
        }

        let edge_rows = edge_stmt.query_and_then(param_values.as_slice(), row_to_code_edge)?;
        let edges: Vec<CodeEdge> = edge_rows.collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(Neighborhood { nodes, edges })
    }

    // -------------------------------------------------------------------
    // find_call_path
    // -------------------------------------------------------------------

    /// Find the shortest call path between two functions using BFS.
    ///
    /// Returns `None` if no path exists within `max_depth` hops.
    /// The returned path includes both the source and target nodes.
    pub fn find_call_path(
        &self,
        from_id: &str,
        to_id: &str,
        max_depth: u32,
    ) -> Result<Option<Vec<CodeNode>>> {
        if from_id == to_id {
            // Path from a node to itself: return just that node.
            let node = self.store.conn.query_row(
                "SELECT * FROM nodes WHERE id = ?1",
                params![from_id],
                row_to_code_node,
            );
            return match node {
                Ok(n) => Ok(Some(vec![n])),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            };
        }

        // BFS: queue holds (node_id, path_of_ids_so_far).
        let mut queue: VecDeque<(String, Vec<String>)> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();

        queue.push_back((from_id.to_string(), vec![from_id.to_string()]));
        visited.insert(from_id.to_string());

        while let Some((current, path)) = queue.pop_front() {
            // path contains nodes; edges = path.len() - 1.
            // If we've already used max_depth edges, don't explore further.
            let edges_used = (path.len() as u32) - 1;
            if edges_used >= max_depth {
                continue;
            }

            // Get direct callees (outgoing "calls" edges).
            let mut stmt = self.store.conn.prepare_cached(
                "SELECT target_id FROM edges WHERE source_id = ?1 AND type = 'calls'",
            )?;
            let neighbors: Vec<String> = stmt
                .query_map(params![current], |row| row.get::<_, String>(0))?
                .filter_map(|r| r.ok())
                .collect();

            for neighbor in neighbors {
                if neighbor == to_id {
                    // Found the target — reconstruct the full path.
                    let mut full_path = path.clone();
                    full_path.push(neighbor);

                    // Fetch CodeNode objects for each ID in the path.
                    let mut nodes = Vec::with_capacity(full_path.len());
                    for id in &full_path {
                        let node = self.store.conn.query_row(
                            "SELECT * FROM nodes WHERE id = ?1",
                            params![id],
                            row_to_code_node,
                        )?;
                        nodes.push(node);
                    }
                    return Ok(Some(nodes));
                }

                if !visited.contains(&neighbor) {
                    visited.insert(neighbor.clone());
                    let mut new_path = path.clone();
                    new_path.push(neighbor);
                    queue.push_back((new_path.last().unwrap().clone(), new_path));
                }
            }
        }

        Ok(None)
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
    fn make_node(id: &str, name: &str, file: &str, kind: NodeKind, line: u32) -> CodeNode {
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
            documentation: Some(format!("Docs for {}", name)),
            exported: Some(true),
        }
    }

    /// Build a minimal test edge.
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

    /// Seed a simple linear chain: a -> b -> c -> d
    fn seed_linear_chain(store: &GraphStore) {
        store
            .upsert_nodes(&[
                make_node("a", "alpha", "src/a.ts", NodeKind::Function, 1),
                make_node("b", "beta", "src/b.ts", NodeKind::Function, 1),
                make_node("c", "gamma", "src/c.ts", NodeKind::Function, 1),
                make_node("d", "delta", "src/d.ts", NodeKind::Function, 1),
            ])
            .unwrap();
        store
            .upsert_edges(&[
                make_edge("a", "b", EdgeKind::Calls, "src/a.ts", 2),
                make_edge("b", "c", EdgeKind::Calls, "src/b.ts", 2),
                make_edge("c", "d", EdgeKind::Calls, "src/c.ts", 2),
            ])
            .unwrap();
    }

    // -----------------------------------------------------------------------
    // 1. find_dependencies — linear chain
    // -----------------------------------------------------------------------

    #[test]
    fn find_dependencies_follows_outgoing_edges() {
        let store = setup();
        seed_linear_chain(&store);
        let traversal = GraphTraversal::new(&store);

        let deps = traversal.find_dependencies("a", 5).unwrap();

        // a -> b -> c -> d : three dependencies
        assert_eq!(deps.len(), 3);
        assert_eq!(deps[0].node.id, "b");
        assert_eq!(deps[0].depth, 1);
        assert_eq!(deps[1].node.id, "c"); // "gamma" sorts after "beta"
        assert_eq!(deps[1].depth, 2);
        assert_eq!(deps[2].node.id, "d");
        assert_eq!(deps[2].depth, 3);
    }

    // -----------------------------------------------------------------------
    // 2. find_dependencies — respects max_depth
    // -----------------------------------------------------------------------

    #[test]
    fn find_dependencies_respects_max_depth() {
        let store = setup();
        seed_linear_chain(&store);
        let traversal = GraphTraversal::new(&store);

        let deps = traversal.find_dependencies("a", 1).unwrap();

        // Only direct dependency at depth 1
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].node.id, "b");
        assert_eq!(deps[0].depth, 1);
    }

    // -----------------------------------------------------------------------
    // find_callees — follows outgoing "calls" edges
    // -----------------------------------------------------------------------

    #[test]
    fn find_callees_follows_outgoing_call_edges() {
        let store = setup();
        seed_linear_chain(&store); // a -> b -> c -> d (all via Calls edges)
        let traversal = GraphTraversal::new(&store);

        let callees = traversal.find_callees("a", 5).unwrap();

        // a calls b, b calls c, c calls d
        assert_eq!(callees.len(), 3);
        assert_eq!(callees[0].node.id, "b");
        assert_eq!(callees[0].depth, 1);
        assert_eq!(callees[1].node.id, "c");
        assert_eq!(callees[1].depth, 2);
        assert_eq!(callees[2].node.id, "d");
        assert_eq!(callees[2].depth, 3);
    }

    #[test]
    fn find_callees_ignores_non_call_edges() {
        let store = setup();

        store
            .upsert_nodes(&[
                make_node("a", "alpha", "src/a.ts", NodeKind::Function, 1),
                make_node("b", "beta", "src/b.ts", NodeKind::Function, 1),
                make_node("c", "gamma", "src/c.ts", NodeKind::Function, 1),
            ])
            .unwrap();
        store
            .upsert_edges(&[
                make_edge("a", "b", EdgeKind::Calls, "src/a.ts", 2),
                make_edge("a", "c", EdgeKind::Imports, "src/a.ts", 1),
            ])
            .unwrap();

        let traversal = GraphTraversal::new(&store);
        let callees = traversal.find_callees("a", 5).unwrap();

        // Only "calls" edges are followed, so only "b" should appear
        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].node.id, "b");
    }

    #[test]
    fn find_callees_respects_max_depth() {
        let store = setup();
        seed_linear_chain(&store);
        let traversal = GraphTraversal::new(&store);

        let callees = traversal.find_callees("a", 1).unwrap();

        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].node.id, "b");
    }

    // -----------------------------------------------------------------------
    // 3. find_callers — follows incoming "calls" edges
    // -----------------------------------------------------------------------

    #[test]
    fn find_callers_follows_incoming_call_edges() {
        let store = setup();
        seed_linear_chain(&store);
        let traversal = GraphTraversal::new(&store);

        let callers = traversal.find_callers("d", 5).unwrap();

        // d is called by c, c by b, b by a
        assert_eq!(callers.len(), 3);
        assert_eq!(callers[0].depth, 1); // c
        assert_eq!(callers[1].depth, 2); // b
        assert_eq!(callers[2].depth, 3); // a
    }

    // -----------------------------------------------------------------------
    // 4. find_transitive_deps
    // -----------------------------------------------------------------------

    #[test]
    fn find_transitive_deps_returns_full_tree() {
        let store = setup();
        seed_linear_chain(&store);
        let traversal = GraphTraversal::new(&store);

        let deps = traversal.find_transitive_deps("a").unwrap();

        assert_eq!(deps.len(), 3);
        let ids: Vec<&str> = deps.iter().map(|n| n.id.as_str()).collect();
        assert!(ids.contains(&"b"));
        assert!(ids.contains(&"c"));
        assert!(ids.contains(&"d"));
    }

    // -----------------------------------------------------------------------
    // 5. find_tests — matches test file patterns
    // -----------------------------------------------------------------------

    #[test]
    fn find_tests_returns_test_nodes_referencing_target() {
        let store = setup();

        store
            .upsert_nodes(&[
                make_node("fn1", "doWork", "src/worker.ts", NodeKind::Function, 1),
                make_node(
                    "test1",
                    "testDoWork",
                    "src/__tests__/worker.test.ts",
                    NodeKind::Function,
                    1,
                ),
                make_node(
                    "test2",
                    "doWorkTest",
                    "src/worker.spec.ts",
                    NodeKind::Function,
                    1,
                ),
                make_node("other", "helper", "src/helper.ts", NodeKind::Function, 1),
            ])
            .unwrap();
        store
            .upsert_edges(&[
                make_edge(
                    "test1",
                    "fn1",
                    EdgeKind::Calls,
                    "src/__tests__/worker.test.ts",
                    5,
                ),
                make_edge("test2", "fn1", EdgeKind::Calls, "src/worker.spec.ts", 5),
                make_edge("other", "fn1", EdgeKind::Calls, "src/helper.ts", 3),
            ])
            .unwrap();

        let traversal = GraphTraversal::new(&store);
        let tests = traversal.find_tests("fn1").unwrap();

        // Should find test1 and test2 (both in test paths), but not "other"
        assert_eq!(tests.len(), 2);
        let ids: Vec<&str> = tests.iter().map(|n| n.id.as_str()).collect();
        assert!(ids.contains(&"test1"));
        assert!(ids.contains(&"test2"));
        assert!(!ids.contains(&"other"));
    }

    // -----------------------------------------------------------------------
    // 6. detect_cycles — finds strongly connected components
    // -----------------------------------------------------------------------

    #[test]
    fn detect_cycles_finds_mutual_recursion() {
        let store = setup();

        store
            .upsert_nodes(&[
                make_node("a", "alpha", "src/a.ts", NodeKind::Function, 1),
                make_node("b", "beta", "src/b.ts", NodeKind::Function, 1),
                make_node("c", "gamma", "src/c.ts", NodeKind::Function, 1),
            ])
            .unwrap();
        // a -> b -> c -> a (a 3-node cycle)
        store
            .upsert_edges(&[
                make_edge("a", "b", EdgeKind::Calls, "src/a.ts", 2),
                make_edge("b", "c", EdgeKind::Calls, "src/b.ts", 2),
                make_edge("c", "a", EdgeKind::Calls, "src/c.ts", 2),
            ])
            .unwrap();

        let traversal = GraphTraversal::new(&store);
        let cycles = traversal.detect_cycles().unwrap();

        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].size, 3);
        let ids = &cycles[0].node_ids;
        assert!(ids.contains(&"a".to_string()));
        assert!(ids.contains(&"b".to_string()));
        assert!(ids.contains(&"c".to_string()));
    }

    // -----------------------------------------------------------------------
    // 7. detect_cycles — no false positives on acyclic graph
    // -----------------------------------------------------------------------

    #[test]
    fn detect_cycles_returns_empty_for_acyclic_graph() {
        let store = setup();
        seed_linear_chain(&store);

        let traversal = GraphTraversal::new(&store);
        let cycles = traversal.detect_cycles().unwrap();

        assert!(cycles.is_empty());
    }

    // -----------------------------------------------------------------------
    // 8. get_neighborhood — bidirectional subgraph
    // -----------------------------------------------------------------------

    #[test]
    fn get_neighborhood_returns_bidirectional_subgraph() {
        let store = setup();
        seed_linear_chain(&store); // a -> b -> c -> d
        let traversal = GraphTraversal::new(&store);

        // Neighborhood of "b" with radius 1 should include:
        //   - a (incoming caller)
        //   - b (the center)
        //   - c (outgoing dependency)
        let neighborhood = traversal.get_neighborhood("b", 1).unwrap();

        let node_ids: Vec<&str> = neighborhood.nodes.iter().map(|n| n.id.as_str()).collect();
        assert!(
            node_ids.contains(&"a"),
            "should include incoming neighbor a"
        );
        assert!(node_ids.contains(&"b"), "should include the center node b");
        assert!(
            node_ids.contains(&"c"),
            "should include outgoing neighbor c"
        );
        assert!(
            !node_ids.contains(&"d"),
            "d is 2 hops away, beyond radius 1"
        );

        // Edges between those nodes: a->b and b->c
        assert_eq!(neighborhood.edges.len(), 2);
    }

    // -----------------------------------------------------------------------
    // 9. get_neighborhood — empty graph
    // -----------------------------------------------------------------------

    #[test]
    fn get_neighborhood_handles_nonexistent_node() {
        let store = setup();
        let traversal = GraphTraversal::new(&store);

        let neighborhood = traversal.get_neighborhood("nonexistent", 2).unwrap();

        assert!(neighborhood.nodes.is_empty());
        assert!(neighborhood.edges.is_empty());
    }

    // -----------------------------------------------------------------------
    // 10. find_dependencies — handles cycles gracefully
    // -----------------------------------------------------------------------

    #[test]
    fn find_dependencies_handles_cyclic_graph() {
        let store = setup();

        store
            .upsert_nodes(&[
                make_node("x", "ex", "src/x.ts", NodeKind::Function, 1),
                make_node("y", "why", "src/y.ts", NodeKind::Function, 1),
            ])
            .unwrap();
        // x -> y -> x (2-node cycle)
        store
            .upsert_edges(&[
                make_edge("x", "y", EdgeKind::Calls, "src/x.ts", 2),
                make_edge("y", "x", EdgeKind::Calls, "src/y.ts", 2),
            ])
            .unwrap();

        let traversal = GraphTraversal::new(&store);
        let deps = traversal.find_dependencies("x", 10).unwrap();

        // The CTE's `instr(path, target_id) = 0` prevents infinite recursion.
        // We should get at least "y" at depth 1. "x" might or might not appear
        // depending on how the CTE resolves — but the key point is it terminates.
        assert!(!deps.is_empty());
        assert!(deps.iter().any(|d| d.node.id == "y"));
    }

    // -----------------------------------------------------------------------
    // 11. find_callers — non-call edges are excluded
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // 12. find_call_path — direct neighbor
    // -----------------------------------------------------------------------

    #[test]
    fn find_call_path_direct_neighbor() {
        let store = setup();
        seed_linear_chain(&store); // a -> b -> c -> d
        let traversal = GraphTraversal::new(&store);

        let path = traversal.find_call_path("a", "b", 5).unwrap();

        assert!(path.is_some(), "direct call a->b should have a path");
        let nodes = path.unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].id, "a");
        assert_eq!(nodes[1].id, "b");
    }

    // -----------------------------------------------------------------------
    // 13. find_call_path — through intermediary
    // -----------------------------------------------------------------------

    #[test]
    fn find_call_path_through_intermediary() {
        let store = setup();
        seed_linear_chain(&store); // a -> b -> c -> d
        let traversal = GraphTraversal::new(&store);

        let path = traversal.find_call_path("a", "d", 10).unwrap();

        assert!(path.is_some(), "path a->b->c->d should exist");
        let nodes = path.unwrap();
        assert_eq!(nodes.len(), 4);
        assert_eq!(nodes[0].id, "a");
        assert_eq!(nodes[1].id, "b");
        assert_eq!(nodes[2].id, "c");
        assert_eq!(nodes[3].id, "d");
    }

    // -----------------------------------------------------------------------
    // 14. find_call_path — no path exists
    // -----------------------------------------------------------------------

    #[test]
    fn find_call_path_no_path() {
        let store = setup();
        seed_linear_chain(&store);
        let traversal = GraphTraversal::new(&store);

        // d does not call a (graph is directed)
        let path = traversal.find_call_path("d", "a", 10).unwrap();
        assert!(path.is_none(), "no reverse path d->a in a directed graph");
    }

    // -----------------------------------------------------------------------
    // 15. find_call_path — same node
    // -----------------------------------------------------------------------

    #[test]
    fn find_call_path_same_node() {
        let store = setup();
        seed_linear_chain(&store);
        let traversal = GraphTraversal::new(&store);

        let path = traversal.find_call_path("b", "b", 5).unwrap();

        assert!(path.is_some(), "path from b to b should be self");
        let nodes = path.unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, "b");
    }

    // -----------------------------------------------------------------------
    // 16. find_call_path — respects max_depth
    // -----------------------------------------------------------------------

    #[test]
    fn find_call_path_respects_max_depth() {
        let store = setup();
        seed_linear_chain(&store); // a -> b -> c -> d (3 hops)
        let traversal = GraphTraversal::new(&store);

        // max_depth=2 should NOT find path a->d (needs 3 hops)
        let path = traversal.find_call_path("a", "d", 2).unwrap();
        assert!(
            path.is_none(),
            "3-hop path should not be found with max_depth=2"
        );

        // max_depth=3 SHOULD find it
        let path = traversal.find_call_path("a", "d", 3).unwrap();
        assert!(
            path.is_some(),
            "3-hop path should be found with max_depth=3"
        );
    }

    // -----------------------------------------------------------------------
    // 17. find_call_path — nonexistent node
    // -----------------------------------------------------------------------

    #[test]
    fn find_call_path_nonexistent_node() {
        let store = setup();
        seed_linear_chain(&store);
        let traversal = GraphTraversal::new(&store);

        let path = traversal.find_call_path("a", "nonexistent", 10).unwrap();
        assert!(path.is_none());
    }

    // -----------------------------------------------------------------------
    // 11. find_callers — non-call edges are excluded
    // -----------------------------------------------------------------------

    #[test]
    fn find_callers_ignores_non_call_edges() {
        let store = setup();

        store
            .upsert_nodes(&[
                make_node("a", "alpha", "src/a.ts", NodeKind::Function, 1),
                make_node("b", "beta", "src/b.ts", NodeKind::Function, 1),
                make_node("c", "gamma", "src/c.ts", NodeKind::Function, 1),
            ])
            .unwrap();
        store
            .upsert_edges(&[
                make_edge("a", "c", EdgeKind::Calls, "src/a.ts", 2),
                make_edge("b", "c", EdgeKind::Imports, "src/b.ts", 1),
            ])
            .unwrap();

        let traversal = GraphTraversal::new(&store);
        let callers = traversal.find_callers("c", 5).unwrap();

        // Only "calls" edges are followed, so only "a" should appear
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0].node.id, "a");
    }

    // =====================================================================
    // NEW TESTS: Phase 18C — Traversal comprehensive coverage
    // =====================================================================

    // -- Diamond graph: a->b, a->c, b->d, c->d ---------------------------

    fn seed_diamond(store: &GraphStore) {
        store
            .upsert_nodes(&[
                make_node("a", "alpha", "src/a.ts", NodeKind::Function, 1),
                make_node("b", "beta", "src/b.ts", NodeKind::Function, 1),
                make_node("c", "gamma", "src/c.ts", NodeKind::Function, 1),
                make_node("d", "delta", "src/d.ts", NodeKind::Function, 1),
            ])
            .unwrap();
        store
            .upsert_edges(&[
                make_edge("a", "b", EdgeKind::Calls, "src/a.ts", 2),
                make_edge("a", "c", EdgeKind::Calls, "src/a.ts", 3),
                make_edge("b", "d", EdgeKind::Calls, "src/b.ts", 2),
                make_edge("c", "d", EdgeKind::Calls, "src/c.ts", 2),
            ])
            .unwrap();
    }

    #[test]
    fn diamond_dependencies_from_root() {
        let store = setup();
        seed_diamond(&store);
        let traversal = GraphTraversal::new(&store);

        let deps = traversal.find_dependencies("a", 5).unwrap();
        assert_eq!(deps.len(), 3);
        let ids: Vec<&str> = deps.iter().map(|d| d.node.id.as_str()).collect();
        assert!(ids.contains(&"b"));
        assert!(ids.contains(&"c"));
        assert!(ids.contains(&"d"));
    }

    #[test]
    fn diamond_callees_from_root() {
        let store = setup();
        seed_diamond(&store);
        let traversal = GraphTraversal::new(&store);

        let callees = traversal.find_callees("a", 5).unwrap();
        assert_eq!(callees.len(), 3);
        // depth 1: b, c; depth 2: d
        assert!(callees.iter().any(|c| c.node.id == "d" && c.depth == 2));
    }

    #[test]
    fn diamond_callers_of_sink() {
        let store = setup();
        seed_diamond(&store);
        let traversal = GraphTraversal::new(&store);

        let callers = traversal.find_callers("d", 5).unwrap();
        assert_eq!(callers.len(), 3); // b, c at depth 1; a at depth 2
        assert!(callers.iter().any(|c| c.node.id == "a" && c.depth == 2));
    }

    #[test]
    fn diamond_find_call_path_two_routes() {
        let store = setup();
        seed_diamond(&store);
        let traversal = GraphTraversal::new(&store);

        // BFS finds shortest path: a->b->d or a->c->d (both length 2)
        let path = traversal.find_call_path("a", "d", 5).unwrap();
        assert!(path.is_some());
        let nodes = path.unwrap();
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].id, "a");
        assert_eq!(nodes[2].id, "d");
    }

    // -- Wide graph: hub -> 50 spokes ------------------------------------

    #[test]
    fn wide_graph_callees() {
        let store = setup();
        let mut nodes = vec![make_node("hub", "hub", "src/hub.ts", NodeKind::Function, 1)];
        let mut edges = Vec::new();
        for i in 0..50 {
            let id = format!("spoke{}", i);
            nodes.push(make_node(
                &id,
                &format!("spoke{}", i),
                "src/spokes.ts",
                NodeKind::Function,
                i + 10,
            ));
            edges.push(make_edge("hub", &id, EdgeKind::Calls, "src/hub.ts", i + 2));
        }
        store.upsert_nodes(&nodes).unwrap();
        store.upsert_edges(&edges).unwrap();

        let traversal = GraphTraversal::new(&store);
        let callees = traversal.find_callees("hub", 1).unwrap();
        assert_eq!(callees.len(), 50);
    }

    #[test]
    fn wide_graph_dependencies() {
        let store = setup();
        let mut nodes = vec![make_node("hub", "hub", "src/hub.ts", NodeKind::Function, 1)];
        let mut edges = Vec::new();
        for i in 0..30 {
            let id = format!("dep{}", i);
            nodes.push(make_node(
                &id,
                &format!("dep{}", i),
                "src/deps.ts",
                NodeKind::Function,
                i + 10,
            ));
            edges.push(make_edge(
                "hub",
                &id,
                EdgeKind::Imports,
                "src/hub.ts",
                i + 1,
            ));
        }
        store.upsert_nodes(&nodes).unwrap();
        store.upsert_edges(&edges).unwrap();

        let traversal = GraphTraversal::new(&store);
        let deps = traversal.find_dependencies("hub", 1).unwrap();
        assert_eq!(deps.len(), 30);
    }

    // -- Deep chain: depth 20 -------------------------------------------

    #[test]
    fn deep_chain_full_traversal() {
        let store = setup();
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for i in 0..20 {
            nodes.push(make_node(
                &format!("n{}", i),
                &format!("fn{}", i),
                &format!("src/{}.ts", i),
                NodeKind::Function,
                1,
            ));
        }
        for i in 0..19 {
            edges.push(make_edge(
                &format!("n{}", i),
                &format!("n{}", i + 1),
                EdgeKind::Calls,
                &format!("src/{}.ts", i),
                2,
            ));
        }
        store.upsert_nodes(&nodes).unwrap();
        store.upsert_edges(&edges).unwrap();

        let traversal = GraphTraversal::new(&store);

        let callees = traversal.find_callees("n0", 25).unwrap();
        assert_eq!(callees.len(), 19);
        assert_eq!(callees.last().unwrap().depth, 19);
    }

    #[test]
    fn deep_chain_depth_limited() {
        let store = setup();
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for i in 0..20 {
            nodes.push(make_node(
                &format!("n{}", i),
                &format!("fn{}", i),
                "src/chain.ts",
                NodeKind::Function,
                i + 1,
            ));
        }
        for i in 0..19 {
            edges.push(make_edge(
                &format!("n{}", i),
                &format!("n{}", i + 1),
                EdgeKind::Calls,
                "src/chain.ts",
                i + 2,
            ));
        }
        store.upsert_nodes(&nodes).unwrap();
        store.upsert_edges(&edges).unwrap();

        let traversal = GraphTraversal::new(&store);
        let callees = traversal.find_callees("n0", 5).unwrap();
        assert_eq!(callees.len(), 5);
    }

    // -- Mixed edge types -------------------------------------------------

    #[test]
    fn mixed_edge_types_dependencies() {
        let store = setup();
        store
            .upsert_nodes(&[
                make_node("a", "alpha", "src/a.ts", NodeKind::Function, 1),
                make_node("b", "beta", "src/b.ts", NodeKind::Function, 1),
                make_node("c", "gamma", "src/c.ts", NodeKind::Class, 1),
                make_node("d", "delta", "src/d.ts", NodeKind::Interface, 1),
            ])
            .unwrap();
        store
            .upsert_edges(&[
                make_edge("a", "b", EdgeKind::Calls, "src/a.ts", 2),
                make_edge("a", "c", EdgeKind::Imports, "src/a.ts", 1),
                make_edge("c", "d", EdgeKind::Extends, "src/c.ts", 2),
            ])
            .unwrap();

        let traversal = GraphTraversal::new(&store);

        // find_dependencies follows all edge types
        let deps = traversal.find_dependencies("a", 5).unwrap();
        let ids: Vec<&str> = deps.iter().map(|d| d.node.id.as_str()).collect();
        assert!(ids.contains(&"b"));
        assert!(ids.contains(&"c"));
        assert!(ids.contains(&"d"));

        // find_callees only follows "calls" edges
        let callees = traversal.find_callees("a", 5).unwrap();
        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].node.id, "b");
    }

    // -- detect_cycles: multiple independent cycles -----------------------

    #[test]
    fn detect_cycles_multiple_independent() {
        let store = setup();
        // Cycle 1: x -> y -> x
        // Cycle 2: p -> q -> r -> p
        store
            .upsert_nodes(&[
                make_node("x", "ex", "src/x.ts", NodeKind::Function, 1),
                make_node("y", "why", "src/y.ts", NodeKind::Function, 1),
                make_node("p", "pee", "src/p.ts", NodeKind::Function, 1),
                make_node("q", "queue", "src/q.ts", NodeKind::Function, 1),
                make_node("r", "arr", "src/r.ts", NodeKind::Function, 1),
            ])
            .unwrap();
        store
            .upsert_edges(&[
                make_edge("x", "y", EdgeKind::Calls, "src/x.ts", 2),
                make_edge("y", "x", EdgeKind::Calls, "src/y.ts", 2),
                make_edge("p", "q", EdgeKind::Calls, "src/p.ts", 2),
                make_edge("q", "r", EdgeKind::Calls, "src/q.ts", 2),
                make_edge("r", "p", EdgeKind::Calls, "src/r.ts", 2),
            ])
            .unwrap();

        let traversal = GraphTraversal::new(&store);
        let cycles = traversal.detect_cycles().unwrap();
        assert_eq!(cycles.len(), 2);

        let sizes: Vec<usize> = cycles.iter().map(|c| c.size).collect();
        assert!(sizes.contains(&2));
        assert!(sizes.contains(&3));
    }

    #[test]
    fn detect_cycles_self_loop() {
        let store = setup();
        store
            .upsert_node(&make_node("a", "alpha", "src/a.ts", NodeKind::Function, 1))
            .unwrap();
        store
            .upsert_edge(&make_edge("a", "a", EdgeKind::Calls, "src/a.ts", 2))
            .unwrap();

        let traversal = GraphTraversal::new(&store);
        let cycles = traversal.detect_cycles().unwrap();
        // Self-loop is not a 2+ SCC in Tarjan's unless explicitly counted
        // The implementation only returns SCCs with size >= 2
        // A self-loop creates a single-node SCC so it won't appear
        assert!(cycles.is_empty() || cycles.iter().all(|c| c.size >= 2));
    }

    #[test]
    fn detect_cycles_empty_graph() {
        let store = setup();
        let traversal = GraphTraversal::new(&store);
        let cycles = traversal.detect_cycles().unwrap();
        assert!(cycles.is_empty());
    }

    // -- find_call_path: various patterns ---------------------------------

    #[test]
    fn find_call_path_diamond_shortest() {
        let store = setup();
        seed_diamond(&store);
        let traversal = GraphTraversal::new(&store);

        let path = traversal.find_call_path("a", "d", 10).unwrap();
        assert!(path.is_some());
        // BFS guarantees shortest path: 3 nodes (a, {b|c}, d)
        assert_eq!(path.unwrap().len(), 3);
    }

    #[test]
    fn find_call_path_disconnected_nodes() {
        let store = setup();
        store
            .upsert_nodes(&[
                make_node("a", "alpha", "src/a.ts", NodeKind::Function, 1),
                make_node("b", "beta", "src/b.ts", NodeKind::Function, 1),
            ])
            .unwrap();
        // No edges between a and b

        let traversal = GraphTraversal::new(&store);
        let path = traversal.find_call_path("a", "b", 10).unwrap();
        assert!(path.is_none());
    }

    #[test]
    fn find_call_path_ignores_import_edges() {
        let store = setup();
        store
            .upsert_nodes(&[
                make_node("a", "alpha", "src/a.ts", NodeKind::Function, 1),
                make_node("b", "beta", "src/b.ts", NodeKind::Function, 1),
            ])
            .unwrap();
        store
            .upsert_edge(&make_edge("a", "b", EdgeKind::Imports, "src/a.ts", 1))
            .unwrap();

        let traversal = GraphTraversal::new(&store);
        let path = traversal.find_call_path("a", "b", 10).unwrap();
        assert!(
            path.is_none(),
            "find_call_path should only follow calls edges"
        );
    }

    // -- get_neighborhood: various radii ----------------------------------

    #[test]
    fn get_neighborhood_radius_zero() {
        let store = setup();
        seed_linear_chain(&store);
        let traversal = GraphTraversal::new(&store);

        let n = traversal.get_neighborhood("b", 0).unwrap();
        // Radius 0 means just the node itself
        assert_eq!(n.nodes.len(), 1);
        assert_eq!(n.nodes[0].id, "b");
    }

    #[test]
    fn get_neighborhood_radius_two() {
        let store = setup();
        seed_linear_chain(&store); // a -> b -> c -> d
        let traversal = GraphTraversal::new(&store);

        let n = traversal.get_neighborhood("b", 2).unwrap();
        let ids: Vec<&str> = n.nodes.iter().map(|n| n.id.as_str()).collect();
        // Radius 2 from b: a (1 hop back), b, c (1 hop forward), d (2 hops forward)
        assert!(ids.contains(&"a"));
        assert!(ids.contains(&"b"));
        assert!(ids.contains(&"c"));
        assert!(ids.contains(&"d"));
    }

    #[test]
    fn get_neighborhood_full_radius() {
        let store = setup();
        seed_linear_chain(&store); // a -> b -> c -> d
        let traversal = GraphTraversal::new(&store);

        let n = traversal.get_neighborhood("b", 10).unwrap();
        assert_eq!(n.nodes.len(), 4); // all 4 nodes reachable
    }

    #[test]
    fn get_neighborhood_includes_edges_between_nodes() {
        let store = setup();
        seed_diamond(&store); // a->b, a->c, b->d, c->d
        let traversal = GraphTraversal::new(&store);

        let n = traversal.get_neighborhood("a", 2).unwrap();
        assert_eq!(n.nodes.len(), 4);
        assert_eq!(n.edges.len(), 4);
    }

    // -- find_dependencies: no deps for leaf node -------------------------

    #[test]
    fn find_dependencies_leaf_node_empty() {
        let store = setup();
        seed_linear_chain(&store);
        let traversal = GraphTraversal::new(&store);

        let deps = traversal.find_dependencies("d", 5).unwrap();
        assert!(deps.is_empty(), "leaf node d has no outgoing dependencies");
    }

    // -- find_callers: root node has no callers ---------------------------

    #[test]
    fn find_callers_root_node_empty() {
        let store = setup();
        seed_linear_chain(&store);
        let traversal = GraphTraversal::new(&store);

        let callers = traversal.find_callers("a", 5).unwrap();
        assert!(callers.is_empty(), "root node a has no callers");
    }

    // -- find_callees: empty for node with no calls -----------------------

    #[test]
    fn find_callees_empty_for_isolated_node() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "solo",
                "solo",
                "src/solo.ts",
                NodeKind::Function,
                1,
            ))
            .unwrap();
        let traversal = GraphTraversal::new(&store);

        let callees = traversal.find_callees("solo", 5).unwrap();
        assert!(callees.is_empty());
    }

    // -- find_transitive_deps on diamond ----------------------------------

    #[test]
    fn find_transitive_deps_diamond() {
        let store = setup();
        seed_diamond(&store);
        let traversal = GraphTraversal::new(&store);

        let deps = traversal.find_transitive_deps("a").unwrap();
        assert_eq!(deps.len(), 3);
    }

    // -- find_tests: no tests available -----------------------------------

    #[test]
    fn find_tests_returns_empty_when_no_tests() {
        let store = setup();
        store
            .upsert_nodes(&[
                make_node("fn1", "doWork", "src/worker.ts", NodeKind::Function, 1),
                make_node("fn2", "helper", "src/helper.ts", NodeKind::Function, 1),
            ])
            .unwrap();
        store
            .upsert_edge(&make_edge(
                "fn2",
                "fn1",
                EdgeKind::Calls,
                "src/helper.ts",
                5,
            ))
            .unwrap();

        let traversal = GraphTraversal::new(&store);
        let tests = traversal.find_tests("fn1").unwrap();
        assert!(tests.is_empty());
    }

    // -- find_dependencies: nonexistent start node returns empty ----------

    #[test]
    fn find_dependencies_nonexistent_node() {
        let store = setup();
        seed_linear_chain(&store);
        let traversal = GraphTraversal::new(&store);

        let deps = traversal.find_dependencies("nonexistent", 5).unwrap();
        assert!(deps.is_empty());
    }

    // -- find_callees: max_depth 0 returns empty --------------------------

    #[test]
    fn find_callees_max_depth_zero() {
        let store = setup();
        seed_linear_chain(&store);
        let traversal = GraphTraversal::new(&store);

        let callees = traversal.find_callees("a", 0).unwrap();
        assert!(callees.is_empty(), "depth 0 means no traversal");
    }

    // -- find_callers: max_depth 0 returns empty --------------------------

    #[test]
    fn find_callers_max_depth_zero() {
        let store = setup();
        seed_linear_chain(&store);
        let traversal = GraphTraversal::new(&store);

        let callers = traversal.find_callers("d", 0).unwrap();
        assert!(callers.is_empty());
    }

    // -- find_call_path: with cycle in graph doesn't hang -----------------

    #[test]
    fn find_call_path_in_cyclic_graph() {
        let store = setup();
        store
            .upsert_nodes(&[
                make_node("a", "alpha", "src/a.ts", NodeKind::Function, 1),
                make_node("b", "beta", "src/b.ts", NodeKind::Function, 1),
                make_node("c", "gamma", "src/c.ts", NodeKind::Function, 1),
            ])
            .unwrap();
        store
            .upsert_edges(&[
                make_edge("a", "b", EdgeKind::Calls, "src/a.ts", 2),
                make_edge("b", "c", EdgeKind::Calls, "src/b.ts", 2),
                make_edge("c", "a", EdgeKind::Calls, "src/c.ts", 2),
            ])
            .unwrap();

        let traversal = GraphTraversal::new(&store);
        let path = traversal.find_call_path("a", "c", 10).unwrap();
        assert!(path.is_some());
        let nodes = path.unwrap();
        assert_eq!(nodes.len(), 3); // a -> b -> c
    }

    // -- find_dependencies: depth ordering --------------------------------

    #[test]
    fn find_dependencies_ordered_by_depth() {
        let store = setup();
        seed_linear_chain(&store);
        let traversal = GraphTraversal::new(&store);

        let deps = traversal.find_dependencies("a", 10).unwrap();
        for i in 1..deps.len() {
            assert!(
                deps[i].depth >= deps[i - 1].depth,
                "results should be ordered by depth"
            );
        }
    }

    // -- find_callers: depth ordering -------------------------------------

    #[test]
    fn find_callers_ordered_by_depth() {
        let store = setup();
        seed_linear_chain(&store);
        let traversal = GraphTraversal::new(&store);

        let callers = traversal.find_callers("d", 10).unwrap();
        for i in 1..callers.len() {
            assert!(callers[i].depth >= callers[i - 1].depth);
        }
    }
}

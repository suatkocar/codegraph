//! Interactive web visualization server for the CodeGraph.
//!
//! Serves a D3.js force-directed graph of the code graph with search,
//! filtering, and node detail inspection.

mod assets;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    response::{Html, IntoResponse},
    routing::get,
    Json, Router,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::db::schema::initialize_database;
use crate::graph::search::{HybridSearch, SearchOptions};
use crate::graph::store::GraphStore;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

struct VizState {
    store: Mutex<GraphStore>,
}

// ---------------------------------------------------------------------------
// JSON response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct NodeJson {
    id: String,
    name: String,
    kind: String,
    file_path: String,
    start_line: u32,
    end_line: u32,
    language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    documentation: Option<String>,
}

#[derive(Serialize)]
struct EdgeJson {
    source: String,
    target: String,
    kind: String,
}

#[derive(Serialize)]
struct SearchResultJson {
    node_id: String,
    name: String,
    kind: String,
    file_path: String,
    score: f64,
}

#[derive(Serialize)]
struct NodeDetailJson {
    node: NodeJson,
    callers: Vec<RefNodeJson>,
    callees: Vec<RefNodeJson>,
}

#[derive(Serialize)]
struct RefNodeJson {
    id: String,
    name: String,
    kind: String,
    file_path: String,
}

#[derive(Serialize)]
struct StatsJson {
    nodes: usize,
    edges: usize,
    files: usize,
    languages: Vec<String>,
}

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct NodesQuery {
    limit: Option<usize>,
    kind: Option<String>,
    language: Option<String>,
}

#[derive(Deserialize)]
struct EdgesQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct SearchQuery {
    q: Option<String>,
    limit: Option<usize>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn index_page() -> Html<&'static str> {
    Html(assets::INDEX_HTML)
}

async fn get_nodes(
    State(state): State<Arc<VizState>>,
    Query(params): Query<NodesQuery>,
) -> Json<Vec<NodeJson>> {
    let limit = params.limit.unwrap_or(200).min(1000);
    let store = state.store.lock().await;

    // Build a query that selects top nodes by in-degree (a proxy for importance)
    // with optional kind and language filters.
    let mut sql = String::from(
        "SELECT n.id, n.name, n.type, n.file_path, n.start_line, n.end_line, n.language \
         FROM nodes n \
         LEFT JOIN edges e ON e.target_id = n.id \
         WHERE 1=1",
    );
    let mut bind_values: Vec<String> = Vec::new();

    if let Some(ref kind) = params.kind {
        bind_values.push(kind.clone());
        sql.push_str(&format!(" AND n.type = ?{}", bind_values.len()));
    }
    if let Some(ref language) = params.language {
        bind_values.push(language.clone());
        sql.push_str(&format!(" AND n.language = ?{}", bind_values.len()));
    }

    sql.push_str(" GROUP BY n.id ORDER BY COUNT(e.source_id) DESC");
    bind_values.push(limit.to_string());
    sql.push_str(&format!(" LIMIT ?{}", bind_values.len()));

    let mut stmt = match store.conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return Json(Vec::new()),
    };

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = bind_values
        .iter()
        .map(|v| v as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(NodeJson {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                file_path: row.get(3)?,
                start_line: row.get(4)?,
                end_line: row.get(5)?,
                language: row.get(6)?,
                body: None,
                documentation: None,
            })
        })
        .ok();

    let nodes: Vec<NodeJson> = rows
        .map(|r| r.filter_map(|x| x.ok()).collect())
        .unwrap_or_default();

    Json(nodes)
}

async fn get_edges(
    State(state): State<Arc<VizState>>,
    Query(params): Query<EdgesQuery>,
) -> Json<Vec<EdgeJson>> {
    let limit = params.limit.unwrap_or(500).min(5000);
    let store = state.store.lock().await;

    let mut stmt = match store
        .conn
        .prepare("SELECT source_id, target_id, type FROM edges LIMIT ?1")
    {
        Ok(s) => s,
        Err(_) => return Json(Vec::new()),
    };

    let rows = stmt
        .query_map(params![limit as i64], |row| {
            Ok(EdgeJson {
                source: row.get(0)?,
                target: row.get(1)?,
                kind: row.get(2)?,
            })
        })
        .ok();

    let edges: Vec<EdgeJson> = rows
        .map(|r| r.filter_map(|x| x.ok()).collect())
        .unwrap_or_default();

    Json(edges)
}

async fn search_nodes(
    State(state): State<Arc<VizState>>,
    Query(params): Query<SearchQuery>,
) -> Json<Vec<SearchResultJson>> {
    let query = match params.q {
        Some(q) if !q.is_empty() => q,
        _ => return Json(Vec::new()),
    };
    let limit = params.limit.unwrap_or(20).min(100);
    let store = state.store.lock().await;

    let search = HybridSearch::new(&store.conn);
    let opts = SearchOptions {
        limit: Some(limit),
        ..Default::default()
    };

    match search.search(&query, &opts) {
        Ok(results) => Json(
            results
                .into_iter()
                .map(|r| SearchResultJson {
                    node_id: r.node_id,
                    name: r.name,
                    kind: r.kind,
                    file_path: r.file_path,
                    score: r.score,
                })
                .collect(),
        ),
        Err(_) => Json(Vec::new()),
    }
}

async fn get_node_detail(
    State(state): State<Arc<VizState>>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    let store = state.store.lock().await;

    let node = match store.get_node(&node_id) {
        Ok(Some(n)) => n,
        _ => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "node not found"})),
            )
                .into_response()
        }
    };

    // Callers: incoming "calls" edges
    let callers: Vec<RefNodeJson> = store
        .get_in_edges(&node_id, Some("calls"))
        .unwrap_or_default()
        .iter()
        .filter_map(|e| {
            store
                .get_node(&e.source)
                .ok()
                .flatten()
                .map(|n| RefNodeJson {
                    id: n.id,
                    name: n.name,
                    kind: n.kind.as_str().to_string(),
                    file_path: n.file_path,
                })
        })
        .collect();

    // Callees: outgoing "calls" edges
    let callees: Vec<RefNodeJson> = store
        .get_out_edges(&node_id, Some("calls"))
        .unwrap_or_default()
        .iter()
        .filter_map(|e| {
            store
                .get_node(&e.target)
                .ok()
                .flatten()
                .map(|n| RefNodeJson {
                    id: n.id,
                    name: n.name,
                    kind: n.kind.as_str().to_string(),
                    file_path: n.file_path,
                })
        })
        .collect();

    // Truncate body for display
    let body = node.body.map(|b| {
        if b.len() > 2000 {
            b[..b.floor_char_boundary(2000)].to_string()
        } else {
            b
        }
    });

    let detail = NodeDetailJson {
        node: NodeJson {
            id: node.id,
            name: node.name,
            kind: node.kind.as_str().to_string(),
            file_path: node.file_path,
            start_line: node.start_line,
            end_line: node.end_line,
            language: node.language.as_str().to_string(),
            body,
            documentation: node.documentation,
        },
        callers,
        callees,
    };

    Json(detail).into_response()
}

async fn get_stats(State(state): State<Arc<VizState>>) -> Json<StatsJson> {
    let store = state.store.lock().await;

    let stats = store
        .get_stats()
        .unwrap_or(crate::graph::store::GraphStats {
            nodes: 0,
            edges: 0,
            files: 0,
        });

    // Get distinct languages
    let languages: Vec<String> = store
        .conn
        .prepare("SELECT DISTINCT language FROM nodes ORDER BY language")
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map([], |row| row.get(0))
                .ok()
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
        })
        .unwrap_or_default();

    Json(StatsJson {
        nodes: stats.nodes,
        edges: stats.edges,
        files: stats.files,
        languages,
    })
}

// ---------------------------------------------------------------------------
// Server entry point
// ---------------------------------------------------------------------------

/// Build the viz Router (extracted for testability).
fn build_router(state: Arc<VizState>) -> Router {
    Router::new()
        .route("/", get(index_page))
        .route("/api/nodes", get(get_nodes))
        .route("/api/edges", get(get_edges))
        .route("/api/search", get(search_nodes))
        .route("/api/node/{id}", get(get_node_detail))
        .route("/api/stats", get(get_stats))
        .with_state(state)
}

/// Start the visualization web server.
///
/// Opens the graph database at `db_path` and serves an interactive D3.js
/// visualization on the given socket address.
pub async fn run_viz_server(
    db_path: &str,
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = initialize_database(db_path)?;
    let store = GraphStore::from_connection(conn);
    let state = Arc::new(VizState {
        store: Mutex::new(store),
    });

    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("CodeGraph visualization: http://{}", addr);
    eprintln!("CodeGraph visualization: http://{}", addr);
    eprintln!("Open in your browser to explore the code graph.");

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("Shutting down visualization server");
            eprintln!("\nShutting down visualization server");
        })
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::initialize_database;
    use crate::types::{CodeEdge, CodeNode, EdgeKind, Language, NodeKind};

    fn test_state() -> Arc<VizState> {
        let conn = initialize_database(":memory:").unwrap();
        let store = GraphStore::from_connection(conn);

        store
            .upsert_node(&CodeNode {
                id: "fn:app.ts:greet:1".into(),
                name: "greet".into(),
                qualified_name: None,
                kind: NodeKind::Function,
                file_path: "app.ts".into(),
                start_line: 1,
                end_line: 5,
                start_column: 0,
                end_column: 1,
                language: Language::TypeScript,
                body: Some("function greet() { return 'hello'; }".into()),
                documentation: Some("Greeting function".into()),
                exported: Some(true),
            })
            .unwrap();

        store
            .upsert_node(&CodeNode {
                id: "fn:app.ts:farewell:7".into(),
                name: "farewell".into(),
                qualified_name: None,
                kind: NodeKind::Function,
                file_path: "app.ts".into(),
                start_line: 7,
                end_line: 10,
                start_column: 0,
                end_column: 1,
                language: Language::TypeScript,
                body: Some("function farewell() {}".into()),
                documentation: None,
                exported: Some(false),
            })
            .unwrap();

        store
            .upsert_edge(&CodeEdge {
                source: "fn:app.ts:greet:1".into(),
                target: "fn:app.ts:farewell:7".into(),
                kind: EdgeKind::Calls,
                file_path: "app.ts".into(),
                line: 3,
                metadata: None,
            })
            .unwrap();

        Arc::new(VizState {
            store: Mutex::new(store),
        })
    }

    #[tokio::test]
    async fn index_page_returns_html() {
        let html = index_page().await;
        assert!(
            html.0.contains("CodeGraph"),
            "should contain CodeGraph title"
        );
        assert!(html.0.contains("<html"), "should be HTML");
    }

    #[tokio::test]
    async fn get_nodes_returns_inserted_nodes() {
        let state = test_state();
        let params = NodesQuery {
            limit: None,
            kind: None,
            language: None,
        };
        let Json(nodes) = get_nodes(State(state), Query(params)).await;
        assert_eq!(nodes.len(), 2);
    }

    #[tokio::test]
    async fn get_nodes_filters_by_kind() {
        let state = test_state();
        let params = NodesQuery {
            limit: None,
            kind: Some("function".into()),
            language: None,
        };
        let Json(nodes) = get_nodes(State(state), Query(params)).await;
        assert_eq!(nodes.len(), 2);
    }

    #[tokio::test]
    async fn get_nodes_respects_limit() {
        let state = test_state();
        let params = NodesQuery {
            limit: Some(1),
            kind: None,
            language: None,
        };
        let Json(nodes) = get_nodes(State(state), Query(params)).await;
        assert_eq!(nodes.len(), 1);
    }

    #[tokio::test]
    async fn get_edges_returns_inserted_edge() {
        let state = test_state();
        let params = EdgesQuery { limit: None };
        let Json(edges) = get_edges(State(state), Query(params)).await;
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source, "fn:app.ts:greet:1");
        assert_eq!(edges[0].target, "fn:app.ts:farewell:7");
        assert_eq!(edges[0].kind, "calls");
    }

    #[tokio::test]
    async fn get_stats_reflects_data() {
        let state = test_state();
        let Json(stats) = get_stats(State(state)).await;
        assert_eq!(stats.nodes, 2);
        assert_eq!(stats.edges, 1);
        assert_eq!(stats.files, 1);
        assert!(stats.languages.contains(&"typescript".to_string()));
    }

    #[tokio::test]
    async fn search_empty_query_returns_empty() {
        let state = test_state();
        let params = SearchQuery {
            q: None,
            limit: None,
        };
        let Json(results) = search_nodes(State(state), Query(params)).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn search_finds_node_by_name() {
        let state = test_state();
        let params = SearchQuery {
            q: Some("greet".into()),
            limit: None,
        };
        let Json(results) = search_nodes(State(state), Query(params)).await;
        assert!(!results.is_empty(), "should find 'greet' node");
        assert!(results.iter().any(|r| r.name == "greet"));
    }

    #[tokio::test]
    async fn get_nodes_empty_db() {
        let conn = initialize_database(":memory:").unwrap();
        let store = GraphStore::from_connection(conn);
        let state = Arc::new(VizState {
            store: Mutex::new(store),
        });
        let params = NodesQuery {
            limit: None,
            kind: None,
            language: None,
        };
        let Json(nodes) = get_nodes(State(state), Query(params)).await;
        assert!(nodes.is_empty());
    }

    #[tokio::test]
    async fn build_router_creates_valid_router() {
        let state = test_state();
        let _router = build_router(state);
        // Router builds without panic — routes are valid
    }
}

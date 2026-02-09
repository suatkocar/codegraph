//! HTTP transport for the MCP server using rmcp's StreamableHttpService.
//!
//! Enables remote MCP clients (Gemini CLI, Cursor, Copilot, remote agents)
//! to connect to CodeGraph over HTTP instead of stdio.
//!
//! Usage: `codegraph serve --http 0.0.0.0:8080`

use std::path::PathBuf;

use crate::config::loader::load_config;
use crate::graph::store::GraphStore;

use super::server::CodeGraphServer;

/// Start the MCP server over HTTP on the given address.
///
/// The server exposes a single `/mcp` endpoint that handles the MCP
/// streamable HTTP protocol (POST for requests, SSE for server-initiated
/// messages). Each client gets its own session.
pub async fn run_http_server(
    store: GraphStore,
    addr: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpService,
    };

    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let config = load_config(None, Some(&project_root)).unwrap_or_default();
    let server = CodeGraphServer::with_config(store, project_root, config);

    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        LocalSessionManager::default().into(),
        Default::default(),
    );

    let router = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("CodeGraph MCP server listening on http://{}/mcp", addr);
    eprintln!("CodeGraph MCP server listening on http://{}/mcp", addr);

    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("Shutting down HTTP server");
        })
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_can_be_cloned_for_http_factory() {
        // StreamableHttpService requires a Clone factory. Verify the full
        // CodeGraphServer construction pipeline works with an in-memory DB.
        let conn = crate::db::schema::initialize_database(":memory:").unwrap();
        let store = GraphStore::from_connection(conn);
        let project_root = PathBuf::from(".");
        let config = load_config(None, Some(&project_root)).unwrap_or_default();
        let server = CodeGraphServer::with_config(store, project_root, config);
        let _cloned = server.clone();
    }
}

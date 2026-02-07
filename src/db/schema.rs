//! SQLite schema initialization for CodeGraph.
//!
//! Faithfully ports the TypeScript `db/schema.ts` to Rust, producing an
//! identical on-disk schema so that databases are interchangeable between
//! the TS and Rust implementations.

use rusqlite::Connection;

// ---------------------------------------------------------------------------
// DDL constants — kept as separate strings so each statement can be executed
// individually (rusqlite's `execute_batch` handles multiple statements, but
// separating them makes error reporting clearer).
// ---------------------------------------------------------------------------

const CREATE_NODES: &str = "\
CREATE TABLE IF NOT EXISTS nodes (
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL,
  name TEXT NOT NULL,
  qualified_name TEXT,
  file_path TEXT NOT NULL,
  start_line INTEGER NOT NULL,
  end_line INTEGER NOT NULL,
  start_column INTEGER DEFAULT 0,
  end_column INTEGER DEFAULT 0,
  language TEXT NOT NULL,
  signature TEXT,
  doc_comment TEXT,
  source_hash TEXT,
  metadata TEXT
)";

const CREATE_EDGES: &str = "\
CREATE TABLE IF NOT EXISTS edges (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  source_id TEXT NOT NULL,
  target_id TEXT NOT NULL,
  type TEXT NOT NULL,
  properties TEXT,
  FOREIGN KEY (source_id) REFERENCES nodes(id) ON DELETE CASCADE,
  FOREIGN KEY (target_id) REFERENCES nodes(id) ON DELETE CASCADE
)";

const CREATE_FILE_HASHES: &str = "\
CREATE TABLE IF NOT EXISTS file_hashes (
  file_path TEXT PRIMARY KEY,
  content_hash TEXT NOT NULL,
  language TEXT NOT NULL,
  indexed_at INTEGER DEFAULT (strftime('%s','now'))
)";

const CREATE_EMBEDDING_CACHE: &str = "\
CREATE TABLE IF NOT EXISTS embedding_cache (
  node_id TEXT PRIMARY KEY,
  embedding BLOB NOT NULL,
  model_version TEXT NOT NULL DEFAULT 'jina-embeddings-v2-base-code',
  FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
)";

const CREATE_UNRESOLVED_REFS: &str = "\
CREATE TABLE IF NOT EXISTS unresolved_refs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  source_id TEXT NOT NULL,
  specifier TEXT NOT NULL,
  ref_type TEXT NOT NULL DEFAULT 'import',
  file_path TEXT NOT NULL,
  line INTEGER DEFAULT 0,
  FOREIGN KEY (source_id) REFERENCES nodes(id) ON DELETE CASCADE
)";

// Indexes ----------------------------------------------------------------

const CREATE_INDEXES: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS idx_nodes_file ON nodes(file_path)",
    "CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(type)",
    "CREATE INDEX IF NOT EXISTS idx_nodes_name ON nodes(name)",
    "CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_id)",
    "CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_id)",
    "CREATE INDEX IF NOT EXISTS idx_edges_type ON edges(type)",
    "CREATE INDEX IF NOT EXISTS idx_unresolved_file ON unresolved_refs(file_path)",
];

// FTS5 -------------------------------------------------------------------

const CREATE_FTS: &str = "\
CREATE VIRTUAL TABLE IF NOT EXISTS fts_nodes USING fts5(
  name, qualified_name, signature, doc_comment, file_path,
  content='nodes', content_rowid='rowid'
)";

const CREATE_FTS_TRIGGERS: &[&str] = &[
    "\
CREATE TRIGGER IF NOT EXISTS nodes_ai AFTER INSERT ON nodes BEGIN
  INSERT INTO fts_nodes(rowid, name, qualified_name, signature, doc_comment, file_path)
  VALUES (new.rowid, new.name, new.qualified_name, new.signature, new.doc_comment, new.file_path);
END",
    "\
CREATE TRIGGER IF NOT EXISTS nodes_ad AFTER DELETE ON nodes BEGIN
  INSERT INTO fts_nodes(fts_nodes, rowid, name, qualified_name, signature, doc_comment, file_path)
  VALUES ('delete', old.rowid, old.name, old.qualified_name, old.signature, old.doc_comment, old.file_path);
END",
    "\
CREATE TRIGGER IF NOT EXISTS nodes_au AFTER UPDATE ON nodes BEGIN
  INSERT INTO fts_nodes(fts_nodes, rowid, name, qualified_name, signature, doc_comment, file_path)
  VALUES ('delete', old.rowid, old.name, old.qualified_name, old.signature, old.doc_comment, old.file_path);
  INSERT INTO fts_nodes(rowid, name, qualified_name, signature, doc_comment, file_path)
  VALUES (new.rowid, new.name, new.qualified_name, new.signature, new.doc_comment, new.file_path);
END",
];

// sqlite-vec -------------------------------------------------------------

const CREATE_VEC_EMBEDDINGS: &str = "\
CREATE VIRTUAL TABLE IF NOT EXISTS vec_embeddings USING vec0(
  node_id TEXT PRIMARY KEY,
  embedding float[768]
)";

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Load the `sqlite-vec` extension via `sqlite3_auto_extension`.
///
/// This **must** be called before any connection is opened so that every
/// new connection automatically has vec0 available.  The call is idempotent
/// — calling it more than once is harmless.
#[allow(clippy::missing_transmute_annotations)]
fn load_sqlite_vec_extension() {
    use rusqlite::ffi::sqlite3_auto_extension;
    use sqlite_vec::sqlite3_vec_init;

    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    }
}

/// Create the `vec_embeddings` virtual table.
///
/// Separated into its own function because it depends on the `sqlite-vec`
/// extension being loaded.  If the extension is unavailable the error is
/// logged as a warning and execution continues — the rest of the schema is
/// fully functional without vector search.
pub fn create_vec_table(conn: &Connection) {
    if let Err(e) = conn.execute_batch(CREATE_VEC_EMBEDDINGS) {
        tracing::warn!(
            "could not create vec_embeddings table \
             (sqlite-vec may not be loaded): {e}"
        );
    }
}

/// Open (or create) the SQLite database at `db_path` and apply the full
/// CodeGraph schema.
///
/// The returned connection has WAL mode, foreign keys, and synchronous
/// NORMAL already configured.
///
/// # Errors
///
/// Returns a `rusqlite::Error` if the database cannot be opened or any DDL
/// statement fails (excluding the optional `vec_embeddings` table).
pub fn initialize_database(db_path: &str) -> rusqlite::Result<Connection> {
    // Register the sqlite-vec auto-extension *before* opening the connection.
    load_sqlite_vec_extension();

    let conn = Connection::open(db_path)?;

    // -- Pragmas ----------------------------------------------------------
    conn.pragma_update(None, "journal_mode", "WAL")?;
    // FK enforcement is OFF (matching TS better-sqlite3 default behavior).
    // Import/type-ref edges often target conceptual IDs (e.g. "module:./path")
    // that aren't actual rows in the nodes table.
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;

    // -- Core tables ------------------------------------------------------
    conn.execute_batch(CREATE_NODES)?;
    conn.execute_batch(CREATE_EDGES)?;
    conn.execute_batch(CREATE_FILE_HASHES)?;
    conn.execute_batch(CREATE_EMBEDDING_CACHE)?;
    conn.execute_batch(CREATE_UNRESOLVED_REFS)?;

    // -- Indexes ----------------------------------------------------------
    for ddl in CREATE_INDEXES {
        conn.execute_batch(ddl)?;
    }

    // -- FTS5 -------------------------------------------------------------
    conn.execute_batch(CREATE_FTS)?;
    for trigger in CREATE_FTS_TRIGGERS {
        conn.execute_batch(trigger)?;
    }

    // -- sqlite-vec -------------------------------------------------------
    create_vec_table(&conn);

    Ok(conn)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: initialize an in-memory database and return the connection.
    fn setup() -> Connection {
        initialize_database(":memory:").expect("schema creation should succeed on :memory:")
    }

    /// Helper: query sqlite_master for a given type and name.
    fn object_exists(conn: &Connection, obj_type: &str, obj_name: &str) -> bool {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
                rusqlite::params![obj_type, obj_name],
                |row| row.get(0),
            )
            .unwrap();
        count > 0
    }

    #[test]
    fn schema_creation_succeeds() {
        let _conn = setup();
        // If we get here without panicking, the schema was applied.
    }

    #[test]
    fn core_tables_exist() {
        let conn = setup();
        for table in &[
            "nodes",
            "edges",
            "file_hashes",
            "embedding_cache",
            "unresolved_refs",
        ] {
            assert!(
                object_exists(&conn, "table", table),
                "table '{table}' should exist"
            );
        }
    }

    #[test]
    fn fts5_table_exists() {
        let conn = setup();
        assert!(
            object_exists(&conn, "table", "fts_nodes"),
            "FTS5 virtual table 'fts_nodes' should exist"
        );
    }

    #[test]
    fn indexes_exist() {
        let conn = setup();
        let expected = [
            "idx_nodes_file",
            "idx_nodes_type",
            "idx_nodes_name",
            "idx_edges_source",
            "idx_edges_target",
            "idx_edges_type",
        ];
        for idx in &expected {
            assert!(
                object_exists(&conn, "index", idx),
                "index '{idx}' should exist"
            );
        }
    }

    #[test]
    fn triggers_exist() {
        let conn = setup();
        for trigger in &["nodes_ai", "nodes_ad", "nodes_au"] {
            assert!(
                object_exists(&conn, "trigger", trigger),
                "trigger '{trigger}' should exist"
            );
        }
    }

    #[test]
    fn vec_embeddings_table_exists() {
        let conn = setup();
        assert!(
            object_exists(&conn, "table", "vec_embeddings"),
            "vec_embeddings virtual table should exist"
        );
    }

    #[test]
    fn pragmas_are_set() {
        let conn = setup();

        let journal_mode: String = conn
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        // In-memory databases may report "memory" instead of "wal", so we
        // accept both.
        assert!(
            journal_mode == "wal" || journal_mode == "memory",
            "journal_mode should be 'wal' or 'memory', got '{journal_mode}'"
        );

        let fk: i64 = conn
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert_eq!(fk, 0, "foreign_keys should be OFF (matching TS behavior)");

        let sync: i64 = conn
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .unwrap();
        // NORMAL = 1
        assert_eq!(sync, 1, "synchronous should be NORMAL (1)");
    }

    #[test]
    fn fts_triggers_fire_on_insert() {
        let conn = setup();

        conn.execute(
            "INSERT INTO nodes (id, type, name, file_path, start_line, end_line, language, source_hash)
             VALUES ('n1', 'function', 'hello', 'src/main.ts', 1, 5, 'typescript', 'abc123')",
            [],
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fts_nodes WHERE fts_nodes MATCH 'hello'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "FTS should contain the inserted node");
    }

    #[test]
    fn fts_triggers_fire_on_delete() {
        let conn = setup();

        conn.execute(
            "INSERT INTO nodes (id, type, name, file_path, start_line, end_line, language, source_hash)
             VALUES ('n1', 'function', 'hello', 'src/main.ts', 1, 5, 'typescript', 'abc123')",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM nodes WHERE id = 'n1'", [])
            .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fts_nodes WHERE fts_nodes MATCH 'hello'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "FTS entry should be removed after node deletion");
    }

    #[test]
    fn nodes_table_has_expected_columns() {
        let conn = setup();

        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(nodes)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        let expected = [
            "id",
            "type",
            "name",
            "qualified_name",
            "file_path",
            "start_line",
            "end_line",
            "start_column",
            "end_column",
            "language",
            "signature",
            "doc_comment",
            "source_hash",
            "metadata",
        ];
        for col in &expected {
            assert!(
                columns.contains(&col.to_string()),
                "nodes table should have column '{col}', found: {columns:?}"
            );
        }
    }

    // -- Additional schema tests (Phase 18D) ----------------------------------

    #[test]
    fn edges_table_has_expected_columns() {
        let conn = setup();
        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(edges)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        for col in &["id", "source_id", "target_id", "type", "properties"] {
            assert!(
                columns.contains(&col.to_string()),
                "edges table should have column '{col}', found: {columns:?}"
            );
        }
    }

    #[test]
    fn file_hashes_table_has_expected_columns() {
        let conn = setup();
        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(file_hashes)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        for col in &["file_path", "content_hash", "language", "indexed_at"] {
            assert!(
                columns.contains(&col.to_string()),
                "file_hashes table should have column '{col}', found: {columns:?}"
            );
        }
    }

    #[test]
    fn embedding_cache_table_has_expected_columns() {
        let conn = setup();
        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(embedding_cache)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        for col in &["node_id", "embedding", "model_version"] {
            assert!(
                columns.contains(&col.to_string()),
                "embedding_cache table should have column '{col}', found: {columns:?}"
            );
        }
    }

    #[test]
    fn unresolved_refs_table_has_expected_columns() {
        let conn = setup();
        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(unresolved_refs)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        for col in &[
            "id",
            "source_id",
            "specifier",
            "ref_type",
            "file_path",
            "line",
        ] {
            assert!(
                columns.contains(&col.to_string()),
                "unresolved_refs table should have column '{col}', found: {columns:?}"
            );
        }
    }

    #[test]
    fn fts_search_works_with_qualified_name() {
        let conn = setup();
        conn.execute(
            "INSERT INTO nodes (id, type, name, qualified_name, file_path, start_line, end_line, language)
             VALUES ('n1', 'method', 'findUser', 'UserService.findUser', 'src/service.ts', 10, 20, 'typescript')",
            [],
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fts_nodes WHERE fts_nodes MATCH 'UserService'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "FTS should match on qualified_name");
    }

    #[test]
    fn fts_search_works_with_doc_comment() {
        let conn = setup();
        conn.execute(
            "INSERT INTO nodes (id, type, name, file_path, start_line, end_line, language, doc_comment)
             VALUES ('n1', 'function', 'process', 'src/app.ts', 1, 5, 'typescript', 'Processes incoming requests from the API gateway')",
            [],
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fts_nodes WHERE fts_nodes MATCH 'gateway'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "FTS should match on doc_comment");
    }

    #[test]
    fn fts_triggers_fire_on_update() {
        let conn = setup();
        conn.execute(
            "INSERT INTO nodes (id, type, name, file_path, start_line, end_line, language)
             VALUES ('n1', 'function', 'oldName', 'src/app.ts', 1, 5, 'typescript')",
            [],
        )
        .unwrap();

        // Verify old name is in FTS
        let count_old: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fts_nodes WHERE fts_nodes MATCH 'oldName'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count_old, 1);

        // Update the name
        conn.execute("UPDATE nodes SET name = 'newName' WHERE id = 'n1'", [])
            .unwrap();

        // Old name should be gone
        let count_old_after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fts_nodes WHERE fts_nodes MATCH 'oldName'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count_old_after, 0,
            "Old name should be removed from FTS after update"
        );

        // New name should be present
        let count_new: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fts_nodes WHERE fts_nodes MATCH 'newName'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count_new, 1, "New name should be in FTS after update");
    }

    #[test]
    fn multiple_nodes_in_fts() {
        let conn = setup();
        for i in 0..10 {
            conn.execute(
                "INSERT INTO nodes (id, type, name, file_path, start_line, end_line, language)
                 VALUES (?1, 'function', ?2, 'src/app.ts', ?3, ?4, 'typescript')",
                rusqlite::params![format!("n{}", i), format!("function{}", i), i, i + 5],
            )
            .unwrap();
        }

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fts_nodes WHERE fts_nodes MATCH 'function*'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 10,
            "FTS should find all 10 functions with prefix match"
        );
    }

    #[test]
    fn schema_handles_null_optional_columns() {
        let conn = setup();
        // Insert with minimal columns (NULLs for optional)
        conn.execute(
            "INSERT INTO nodes (id, type, name, file_path, start_line, end_line, language)
             VALUES ('n1', 'function', 'minimal', 'src/app.ts', 1, 5, 'typescript')",
            [],
        )
        .unwrap();

        let (qn, sig, doc, meta): (Option<String>, Option<String>, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT qualified_name, signature, doc_comment, metadata FROM nodes WHERE id = 'n1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert!(qn.is_none());
        assert!(sig.is_none());
        assert!(doc.is_none());
        assert!(meta.is_none());
    }

    #[test]
    fn edges_autoincrement_id() {
        let conn = setup();
        // Insert a node first
        conn.execute(
            "INSERT INTO nodes (id, type, name, file_path, start_line, end_line, language)
             VALUES ('n1', 'function', 'fn1', 'src/a.ts', 1, 5, 'typescript')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes (id, type, name, file_path, start_line, end_line, language)
             VALUES ('n2', 'function', 'fn2', 'src/b.ts', 1, 5, 'typescript')",
            [],
        )
        .unwrap();

        // Insert edges
        conn.execute(
            "INSERT INTO edges (source_id, target_id, type) VALUES ('n1', 'n2', 'calls')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO edges (source_id, target_id, type) VALUES ('n2', 'n1', 'calls')",
            [],
        )
        .unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);

        // IDs should be auto-incrementing
        let ids: Vec<i64> = conn
            .prepare("SELECT id FROM edges ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(ids.len(), 2);
        assert!(ids[1] > ids[0], "Edge IDs should auto-increment");
    }

    #[test]
    fn file_hashes_default_indexed_at() {
        let conn = setup();
        conn.execute(
            "INSERT INTO file_hashes (file_path, content_hash, language) VALUES ('src/app.ts', 'abc123', 'typescript')",
            [],
        )
        .unwrap();

        let indexed_at: i64 = conn
            .query_row(
                "SELECT indexed_at FROM file_hashes WHERE file_path = 'src/app.ts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        // Should be a reasonable Unix timestamp (after 2020)
        assert!(
            indexed_at > 1577836800,
            "indexed_at should be a valid timestamp, got {}",
            indexed_at
        );
    }

    #[test]
    fn unresolved_refs_index_exists() {
        let conn = setup();
        assert!(
            object_exists(&conn, "index", "idx_unresolved_file"),
            "unresolved_refs file index should exist"
        );
    }

    #[test]
    fn insert_and_query_unresolved_ref() {
        let conn = setup();
        conn.execute(
            "INSERT INTO unresolved_refs (source_id, specifier, ref_type, file_path, line) \
             VALUES ('n1', './missing', 'import', 'src/app.ts', 3)",
            [],
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM unresolved_refs WHERE file_path = 'src/app.ts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn embedding_cache_model_version_default() {
        let conn = setup();
        // Need a node for the FK
        conn.execute(
            "INSERT INTO nodes (id, type, name, file_path, start_line, end_line, language)
             VALUES ('n1', 'function', 'fn1', 'src/a.ts', 1, 5, 'typescript')",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO embedding_cache (node_id, embedding) VALUES ('n1', X'00')",
            [],
        )
        .unwrap();

        let version: String = conn
            .query_row(
                "SELECT model_version FROM embedding_cache WHERE node_id = 'n1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            version, "jina-embeddings-v2-base-code",
            "Default model version should be jina-embeddings-v2-base-code"
        );
    }

    #[test]
    fn node_default_column_values() {
        let conn = setup();
        conn.execute(
            "INSERT INTO nodes (id, type, name, file_path, start_line, end_line, language)
             VALUES ('n1', 'function', 'fn1', 'src/a.ts', 1, 5, 'typescript')",
            [],
        )
        .unwrap();

        let (start_col, end_col): (i64, i64) = conn
            .query_row(
                "SELECT start_column, end_column FROM nodes WHERE id = 'n1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(start_col, 0, "start_column default should be 0");
        assert_eq!(end_col, 0, "end_column default should be 0");
    }

    #[test]
    fn nodes_primary_key_prevents_duplicates() {
        let conn = setup();
        conn.execute(
            "INSERT INTO nodes (id, type, name, file_path, start_line, end_line, language)
             VALUES ('n1', 'function', 'fn1', 'src/a.ts', 1, 5, 'typescript')",
            [],
        )
        .unwrap();

        // Inserting with same ID should fail (without ON CONFLICT)
        let result = conn.execute(
            "INSERT INTO nodes (id, type, name, file_path, start_line, end_line, language)
             VALUES ('n1', 'class', 'different', 'src/b.ts', 10, 20, 'typescript')",
            [],
        );
        assert!(result.is_err(), "Duplicate primary key should fail");
    }

    #[test]
    fn file_hashes_primary_key_prevents_duplicates() {
        let conn = setup();
        conn.execute(
            "INSERT INTO file_hashes (file_path, content_hash, language)
             VALUES ('src/a.ts', 'hash1', 'typescript')",
            [],
        )
        .unwrap();

        let result = conn.execute(
            "INSERT INTO file_hashes (file_path, content_hash, language)
             VALUES ('src/a.ts', 'hash2', 'typescript')",
            [],
        );
        assert!(result.is_err(), "Duplicate file_path should fail");
    }

    #[test]
    fn create_vec_table_is_idempotent() {
        let conn = setup();
        // Call create_vec_table again — should not error
        create_vec_table(&conn);
        assert!(
            object_exists(&conn, "table", "vec_embeddings"),
            "vec_embeddings should still exist after double create"
        );
    }

    #[test]
    fn all_table_count() {
        let conn = setup();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'fts_nodes_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        // nodes, edges, file_hashes, embedding_cache, unresolved_refs, fts_nodes, vec_embeddings
        assert!(count >= 7, "Should have at least 7 tables, got {}", count);
    }
}

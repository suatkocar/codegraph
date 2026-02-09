//! Hybrid search engine for the CodeGraph.
//!
//! Combines SQLite FTS5 keyword search with vector cosine similarity
//! (via sqlite-vec / fastembed Jina v2 Base Code 768-dim), merging
//! results through Reciprocal Rank Fusion (RRF, k=60).
//!
//! Supports query intent detection to dynamically adjust FTS5/vector
//! blending weights, and file-level search for grouped results.

use std::collections::HashMap;

use rusqlite::{params, Connection};

use crate::error::Result;
use crate::graph::expansion::expand_query;

// ---------------------------------------------------------------------------
// Query intent detection
// ---------------------------------------------------------------------------

/// Detected intent of a search query, used to adjust RRF blending weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryIntent {
    /// Looks like a code symbol: camelCase, snake_case, PascalCase, dots, `::`.
    SymbolLookup,
    /// Natural language with spaces, common English words.
    SemanticSearch,
    /// Mixed signals or ambiguous — keep default weights.
    Hybrid,
}

/// Detect whether a query is a symbol lookup, a semantic/natural-language
/// search, or an ambiguous hybrid.
///
/// Uses lightweight heuristics — no allocations beyond the regex check:
///
/// **SymbolLookup indicators**: contains `_`, `.`, `::`, has camelCase
/// transitions (lower->upper), matches PascalCase pattern, has no spaces.
///
/// **SemanticSearch indicators**: has spaces, contains common natural-language
/// words ("the", "how", "what", "find", "all", etc.), more than 3 words.
pub fn detect_query_intent(query: &str) -> QueryIntent {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return QueryIntent::Hybrid;
    }

    let has_spaces = trimmed.contains(' ');
    let word_count = trimmed.split_whitespace().count();

    let mut symbol_signals: u32 = 0;
    let mut semantic_signals: u32 = 0;

    // --- Symbol signals ---

    // Underscores (snake_case / SCREAMING_SNAKE)
    if trimmed.contains('_') {
        symbol_signals += 2;
    }

    // Dot notation (obj.method)
    if trimmed.contains('.') {
        symbol_signals += 2;
    }

    // Rust/C++ path separator
    if trimmed.contains("::") {
        symbol_signals += 2;
    }

    // camelCase transition: a lowercase letter directly followed by an uppercase letter
    let chars: Vec<char> = trimmed.chars().collect();
    let has_camel = chars
        .windows(2)
        .any(|w| w[0].is_lowercase() && w[1].is_uppercase());
    if has_camel {
        symbol_signals += 2;
    }

    // PascalCase: starts with uppercase, has at least one lowercase, no spaces
    if !has_spaces
        && chars[0].is_uppercase()
        && chars.iter().any(|c| c.is_lowercase())
        && chars.len() > 1
    {
        symbol_signals += 1;
    }

    // No spaces at all — likely a single symbol token
    if !has_spaces {
        symbol_signals += 1;
    }

    // --- Semantic signals ---

    const SEMANTIC_WORDS: &[&str] = &[
        "the",
        "a",
        "an",
        "how",
        "what",
        "which",
        "where",
        "when",
        "why",
        "who",
        "find",
        "get",
        "all",
        "that",
        "this",
        "with",
        "from",
        "for",
        "into",
        "does",
        "show",
        "list",
        "is",
        "are",
        "can",
        "should",
        "function",
        "method",
        "class",
        "file",
        "functions",
        "methods",
        "classes",
        "files",
    ];

    if has_spaces {
        semantic_signals += 1;
    }

    if word_count > 3 {
        semantic_signals += 2;
    }

    let lower = trimmed.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    let semantic_word_count = words.iter().filter(|w| SEMANTIC_WORDS.contains(w)).count();
    if semantic_word_count >= 1 {
        semantic_signals += 1;
    }
    if semantic_word_count >= 2 {
        semantic_signals += 2;
    }

    // --- Decision ---

    if symbol_signals >= 2 && semantic_signals == 0 {
        QueryIntent::SymbolLookup
    } else if semantic_signals >= 2 && symbol_signals == 0 {
        QueryIntent::SemanticSearch
    } else if symbol_signals > semantic_signals + 1 {
        QueryIntent::SymbolLookup
    } else if semantic_signals > symbol_signals + 1 {
        QueryIntent::SemanticSearch
    } else {
        QueryIntent::Hybrid
    }
}

/// RRF blending weights for FTS5 and vector based on query intent.
#[derive(Debug, Clone, Copy)]
pub struct BlendWeights {
    pub fts_weight: f64,
    pub vec_weight: f64,
}

impl From<QueryIntent> for BlendWeights {
    fn from(intent: QueryIntent) -> Self {
        match intent {
            QueryIntent::SymbolLookup => BlendWeights {
                fts_weight: 0.8,
                vec_weight: 0.2,
            },
            QueryIntent::SemanticSearch => BlendWeights {
                fts_weight: 0.3,
                vec_weight: 0.7,
            },
            QueryIntent::Hybrid => BlendWeights {
                fts_weight: 1.0,
                vec_weight: 1.0,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// File-level search result
// ---------------------------------------------------------------------------

/// A search result aggregated at the file level.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileSearchResult {
    /// Path to the source file.
    pub file_path: String,
    /// Number of symbols in this file that matched the query.
    pub matched_symbols: usize,
    /// Names of the top-scoring symbols (up to 5).
    pub top_symbols: Vec<String>,
    /// Aggregate relevance score (sum of individual BM25 scores).
    pub relevance_score: f64,
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single search result with composite scoring.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    /// The unique node ID from the `nodes` table.
    pub node_id: String,
    /// Human-readable symbol name.
    pub name: String,
    /// Node kind/type (e.g. "function", "class").
    pub kind: String,
    /// Path to the source file containing this symbol.
    pub file_path: String,
    /// Combined RRF score (higher is better).
    pub score: f64,
    /// Raw FTS5 BM25 score (inverted so higher = better), if present.
    pub fts_score: Option<f64>,
    /// Vector cosine similarity score (0..1), if present.
    pub vec_score: Option<f64>,
    /// Short display snippet derived from docs or signature.
    pub snippet: Option<String>,
}

/// Options that control search behaviour.
#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    /// Maximum results to return (default 20).
    pub limit: Option<usize>,
    /// Filter to a specific programming language.
    pub language: Option<String>,
    /// Filter to a specific node type/kind.
    pub node_type: Option<String>,
    /// Discard results below this RRF score (default 0).
    pub min_score: Option<f64>,
}

// ---------------------------------------------------------------------------
// Internal row shapes
// ---------------------------------------------------------------------------

/// A row returned by the FTS5 keyword query.
struct FtsRow {
    id: String,
    name: String,
    kind: String,
    file_path: String,
    rank: f64,
    #[allow(dead_code)]
    language: String,
    signature: Option<String>,
    doc_comment: Option<String>,
}

// ---------------------------------------------------------------------------
// SQL constants
// ---------------------------------------------------------------------------

const FTS_SEARCH_SQL: &str = "\
SELECT n.id, n.name, n.type, n.file_path, n.language,
       n.signature, n.doc_comment,
       bm25(fts_nodes, 10.0, 8.0, 5.0, 3.0, 1.0, 7.0) AS rank
FROM fts_nodes fts
JOIN nodes n ON n.rowid = fts.rowid
WHERE fts_nodes MATCH ?1
ORDER BY rank
LIMIT ?2";

const GET_NODE_LANGUAGE_SQL: &str = "\
SELECT language FROM nodes WHERE id = ?1";

// ---------------------------------------------------------------------------
// Hybrid search engine
// ---------------------------------------------------------------------------

/// HybridSearch combines SQLite FTS5 keyword search with sqlite-vec
/// cosine similarity to deliver results that are both lexically precise
/// and semantically rich.
///
/// Results from each system are merged using Reciprocal Rank Fusion
/// (RRF), a rank-aggregation method that doesn't require score
/// normalization and gracefully handles result lists of different
/// lengths.
pub struct HybridSearch<'a> {
    conn: &'a Connection,
}

impl<'a> HybridSearch<'a> {
    /// Create a new search engine backed by `conn`.
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Execute a hybrid search: FTS5 keyword + vector similarity, fused
    /// via RRF.
    ///
    /// Automatically detects query intent (symbol lookup vs semantic
    /// search) and adjusts FTS5/vector blending weights accordingly.
    pub fn search(&self, query: &str, options: &SearchOptions) -> Result<Vec<SearchResult>> {
        let limit = options.limit.unwrap_or(20);
        // Fetch more candidates than needed so fusion has room to merge.
        let fetch_limit = limit * 3;

        let fts_results = self.search_by_keyword(query, fetch_limit)?;
        let vec_results = self.search_by_similarity(query, fetch_limit);

        // Query expansion: generate alternative search terms and run
        // them through FTS5.  Expanded results are fused at 0.5x
        // weight (giving the original query 2x relative weight).
        let expansions = expand_query(query);
        let expansion_fts = if expansions.len() > 1 {
            // Build an OR query from all expanded terms (skip index 0
            // which is the original query — already searched above).
            let expanded_query = expansions[1..].join(" OR ");
            let safe = sanitize_fts_query(&expanded_query);
            if safe.is_empty() {
                Vec::new()
            } else {
                self.search_by_keyword(&safe, fetch_limit)
                    .unwrap_or_default()
            }
        } else {
            Vec::new()
        };

        // Detect intent and adjust blending weights.
        let weights: BlendWeights = detect_query_intent(query).into();

        let mut fused =
            fuse_results_weighted(&fts_results, &vec_results, &expansion_fts, 60, weights);

        // Apply optional filters.
        if let Some(ref lang) = options.language {
            fused.retain(|r| self.get_node_language(&r.node_id).as_deref() == Some(lang.as_str()));
        }
        if let Some(ref node_type) = options.node_type {
            fused.retain(|r| r.kind == *node_type);
        }
        if let Some(min_score) = options.min_score {
            if min_score > 0.0 {
                fused.retain(|r| r.score >= min_score);
            }
        }

        fused.truncate(limit);
        Ok(fused)
    }

    /// FTS5 keyword search on the `fts_nodes` virtual table.
    ///
    /// Uses the built-in BM25 ranking (exposed as `rank`). Queries are
    /// sanitized: special FTS5 syntax characters are quoted to prevent
    /// user input from breaking the query.
    pub fn search_by_keyword(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let safe_query = sanitize_fts_query(query);
        if safe_query.is_empty() {
            return Ok(Vec::new());
        }

        let mut stmt = self.conn.prepare_cached(FTS_SEARCH_SQL)?;
        let rows = stmt.query_map(params![safe_query, limit as i64], |row| {
            Ok(FtsRow {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                file_path: row.get(3)?,
                language: row.get(4)?,
                signature: row.get(5)?,
                doc_comment: row.get(6)?,
                rank: row.get(7)?,
            })
        })?;

        let mut results = Vec::new();
        for row_result in rows {
            let row = row_result?;
            let snippet = build_snippet(
                &row.name,
                row.signature.as_deref(),
                row.doc_comment.as_deref(),
            );
            results.push(SearchResult {
                node_id: row.id,
                name: row.name,
                kind: row.kind,
                file_path: row.file_path,
                score: 0.0,                 // will be set by fusion
                fts_score: Some(-row.rank), // FTS5 rank is negative; invert for display
                vec_score: None,
                snippet: Some(snippet),
            });
        }

        Ok(results)
    }

    /// Vector similarity search via sqlite-vec.
    ///
    /// Embeds the query text, finds nearest neighbors by cosine distance
    /// in the `vec_embeddings` virtual table, and decorates each result
    /// with node metadata.
    ///
    /// Returns an empty `Vec` if no embedder is provided or if the
    /// `vec_embeddings` table has no data.
    pub fn search_by_similarity(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        #[cfg(feature = "embedding")]
        {
            // Try to get embedder; if unavailable, return empty
            let embedder = match crate::indexer::embedder::EmbeddingEngine::try_new() {
                Ok(e) => e,
                Err(_) => return Vec::new(),
            };

            let query_vec = match embedder.embed(query) {
                Ok(v) => v,
                Err(_) => return Vec::new(),
            };

            // Convert to JSON array for sqlite-vec MATCH
            let vec_json = match serde_json::to_string(&query_vec) {
                Ok(j) => j,
                Err(_) => return Vec::new(),
            };

            // Query vec_embeddings for nearest neighbors
            let sql = "SELECT v.node_id, v.distance, n.name, n.type, n.file_path
                        FROM vec_embeddings v
                        JOIN nodes n ON n.id = v.node_id
                        WHERE v.embedding MATCH ?1
                        ORDER BY v.distance
                        LIMIT ?2";

            let mut stmt = match self.conn.prepare_cached(sql) {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };

            let rows = match stmt.query_map(params![vec_json, limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            }) {
                Ok(r) => r,
                Err(_) => return Vec::new(),
            };

            let mut results = Vec::new();
            for row in rows.flatten() {
                let (node_id, distance, name, kind, file_path) = row;
                // Convert distance to similarity score (1.0 - distance for cosine)
                let similarity = 1.0 - distance;
                results.push(SearchResult {
                    node_id,
                    name: name.clone(),
                    kind,
                    file_path,
                    score: 0.0, // Will be set by fusion
                    fts_score: None,
                    vec_score: Some(similarity),
                    snippet: Some(name),
                });
            }
            results
        }

        #[cfg(not(feature = "embedding"))]
        {
            let _ = (query, limit);
            Vec::new()
        }
    }

    // -------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------

    /// Look up the language for a node (used for post-fusion filtering).
    fn get_node_language(&self, node_id: &str) -> Option<String> {
        let mut stmt = self.conn.prepare_cached(GET_NODE_LANGUAGE_SQL).ok()?;
        stmt.query_row(params![node_id], |row| row.get::<_, String>(0))
            .ok()
    }

    /// Search and return results grouped by file.
    ///
    /// Runs an FTS5 keyword search, groups matches by `file_path`,
    /// aggregates BM25 scores, and returns the top `limit` files
    /// ranked by aggregate relevance.
    pub fn search_files(&self, query: &str, limit: usize) -> Result<Vec<FileSearchResult>> {
        let safe_query = sanitize_fts_query(query);
        if safe_query.is_empty() {
            return Ok(Vec::new());
        }

        // Fetch a generous number of symbol-level results so grouping
        // has enough data.  We want at most `limit` files, but each
        // file may have many symbols.
        let fetch_limit = limit * 10;

        let sql = "\
            SELECT n.file_path, n.name,
                   bm25(fts_nodes, 10.0, 8.0, 5.0, 3.0, 1.0, 7.0) AS rank
            FROM fts_nodes fts
            JOIN nodes n ON n.rowid = fts.rowid
            WHERE fts_nodes MATCH ?1
            ORDER BY rank
            LIMIT ?2";

        let mut stmt = self.conn.prepare_cached(sql)?;
        let rows = stmt.query_map(params![safe_query, fetch_limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?, // file_path
                row.get::<_, String>(1)?, // name
                row.get::<_, f64>(2)?,    // rank (negative BM25)
            ))
        })?;

        // Accumulate per-file: count, top symbol names, total score.
        #[allow(clippy::type_complexity)]
        let mut file_map: HashMap<String, (usize, Vec<(String, f64)>, f64)> = HashMap::new();

        for row_result in rows {
            let (file_path, name, rank) = row_result?;
            let score = -rank; // BM25 rank is negative; invert
            let entry = file_map
                .entry(file_path)
                .or_insert_with(|| (0, Vec::new(), 0.0));
            entry.0 += 1;
            entry.1.push((name, score));
            entry.2 += score;
        }

        let mut results: Vec<FileSearchResult> = file_map
            .into_iter()
            .map(|(file_path, (count, mut symbols, total_score))| {
                // Sort symbols by score descending, keep top 5 names.
                symbols.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                let top_symbols: Vec<String> =
                    symbols.into_iter().take(5).map(|(n, _)| n).collect();
                FileSearchResult {
                    file_path,
                    matched_symbols: count,
                    top_symbols,
                    relevance_score: total_score,
                }
            })
            .collect();

        // Sort by relevance descending.
        results.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);

        Ok(results)
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Reciprocal Rank Fusion (RRF).
///
/// Merges two ranked result lists into a single list using:
///
///   `score(d) = SUM( 1 / (k + rank_i(d)) )`
///
/// where `k` (default 60) is the standard constant that prevents
/// top-ranked items from dominating. This is a score-agnostic fusion
/// method -- it only cares about rank position, so heterogeneous
/// scoring functions (BM25 vs cosine distance) work naturally.
pub fn fuse_results(
    fts_results: &[SearchResult],
    vec_results: &[SearchResult],
    k: u32,
) -> Vec<SearchResult> {
    let k = k as f64;
    let mut score_map: HashMap<String, (SearchResult, f64)> = HashMap::new();

    // Score from FTS rankings (0-indexed internally, 1-indexed for RRF).
    for (rank, r) in fts_results.iter().enumerate() {
        let rrf_score = 1.0 / (k + (rank as f64) + 1.0);
        match score_map.get_mut(&r.node_id) {
            Some((existing, total)) => {
                *total += rrf_score;
                existing.fts_score = r.fts_score;
            }
            None => {
                score_map.insert(r.node_id.clone(), (r.clone(), rrf_score));
            }
        }
    }

    // Score from vector rankings.
    for (rank, r) in vec_results.iter().enumerate() {
        let rrf_score = 1.0 / (k + (rank as f64) + 1.0);
        match score_map.get_mut(&r.node_id) {
            Some((existing, total)) => {
                *total += rrf_score;
                existing.vec_score = r.vec_score;
            }
            None => {
                score_map.insert(r.node_id.clone(), (r.clone(), rrf_score));
            }
        }
    }

    // Top-rank bonus: reward results that ranked highly in either list.
    // #1 in any list gets +0.05, #2-3 get +0.02. This stabilizes exact
    // keyword matches that score #1 in FTS but could get diluted in fusion.
    for (rank, r) in fts_results.iter().enumerate() {
        if let Some((_, total)) = score_map.get_mut(&r.node_id) {
            match rank {
                0 => *total += 0.05,
                1 | 2 => *total += 0.02,
                _ => {}
            }
        }
    }
    for (rank, r) in vec_results.iter().enumerate() {
        if let Some((_, total)) = score_map.get_mut(&r.node_id) {
            match rank {
                0 => *total += 0.05,
                1 | 2 => *total += 0.02,
                _ => {}
            }
        }
    }

    // Sort by combined RRF score descending.
    let mut fused: Vec<(SearchResult, f64)> = score_map.into_values().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    fused
        .into_iter()
        .map(|(mut result, score)| {
            result.score = score;
            result
        })
        .collect()
}

/// RRF fusion with query-expansion support.
///
/// Like [`fuse_results`] but accepts an additional `expansion_results`
/// list whose RRF contributions are halved (×0.5).  This gives the
/// original FTS results effectively 2× the weight of expansion hits,
/// ensuring that exact keyword matches dominate while expansions
/// surface related symbols that would otherwise be missed.
pub fn fuse_results_with_expansion(
    fts_results: &[SearchResult],
    vec_results: &[SearchResult],
    expansion_results: &[SearchResult],
    k: u32,
) -> Vec<SearchResult> {
    let k_f = k as f64;
    let mut score_map: HashMap<String, (SearchResult, f64)> = HashMap::new();

    // Helper: accumulate RRF score for a ranked list with a weight multiplier.
    let mut accumulate = |results: &[SearchResult], weight: f64, is_fts: bool| {
        for (rank, r) in results.iter().enumerate() {
            let rrf_score = weight / (k_f + (rank as f64) + 1.0);
            match score_map.get_mut(&r.node_id) {
                Some((existing, total)) => {
                    *total += rrf_score;
                    if is_fts {
                        existing.fts_score = r.fts_score;
                    } else {
                        existing.vec_score = r.vec_score;
                    }
                }
                None => {
                    score_map.insert(r.node_id.clone(), (r.clone(), rrf_score));
                }
            }
        }
    };

    // Original FTS: full weight (1.0).
    accumulate(fts_results, 1.0, true);
    // Vector results: full weight (1.0).
    accumulate(vec_results, 1.0, false);
    // Expansion FTS: half weight (0.5) → original is 2× relative.
    accumulate(expansion_results, 0.5, true);

    // Top-rank bonus (same as fuse_results).
    for (rank, r) in fts_results.iter().enumerate() {
        if let Some((_, total)) = score_map.get_mut(&r.node_id) {
            match rank {
                0 => *total += 0.05,
                1 | 2 => *total += 0.02,
                _ => {}
            }
        }
    }
    for (rank, r) in vec_results.iter().enumerate() {
        if let Some((_, total)) = score_map.get_mut(&r.node_id) {
            match rank {
                0 => *total += 0.05,
                1 | 2 => *total += 0.02,
                _ => {}
            }
        }
    }

    // Sort by combined RRF score descending.
    let mut fused: Vec<(SearchResult, f64)> = score_map.into_values().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    fused
        .into_iter()
        .map(|(mut result, score)| {
            result.score = score;
            result
        })
        .collect()
}

/// RRF fusion with query-expansion support and intent-based blending.
///
/// Like [`fuse_results_with_expansion`] but applies [`BlendWeights`]
/// to scale the FTS5 and vector RRF contributions.  When
/// `weights.fts_weight == 0.8` and `weights.vec_weight == 0.2`, FTS5
/// results receive 4x the influence of vector results, which is ideal
/// for symbol lookups.
pub fn fuse_results_weighted(
    fts_results: &[SearchResult],
    vec_results: &[SearchResult],
    expansion_results: &[SearchResult],
    k: u32,
    weights: BlendWeights,
) -> Vec<SearchResult> {
    let k_f = k as f64;
    let mut score_map: HashMap<String, (SearchResult, f64)> = HashMap::new();

    let mut accumulate = |results: &[SearchResult], weight: f64, is_fts: bool| {
        for (rank, r) in results.iter().enumerate() {
            let rrf_score = weight / (k_f + (rank as f64) + 1.0);
            match score_map.get_mut(&r.node_id) {
                Some((existing, total)) => {
                    *total += rrf_score;
                    if is_fts {
                        existing.fts_score = r.fts_score;
                    } else {
                        existing.vec_score = r.vec_score;
                    }
                }
                None => {
                    score_map.insert(r.node_id.clone(), (r.clone(), rrf_score));
                }
            }
        }
    };

    // Original FTS: weighted by intent.
    accumulate(fts_results, weights.fts_weight, true);
    // Vector results: weighted by intent.
    accumulate(vec_results, weights.vec_weight, false);
    // Expansion FTS: half of the FTS weight.
    accumulate(expansion_results, weights.fts_weight * 0.5, true);

    // Top-rank bonus (same as fuse_results).
    for (rank, r) in fts_results.iter().enumerate() {
        if let Some((_, total)) = score_map.get_mut(&r.node_id) {
            match rank {
                0 => *total += 0.05,
                1 | 2 => *total += 0.02,
                _ => {}
            }
        }
    }
    for (rank, r) in vec_results.iter().enumerate() {
        if let Some((_, total)) = score_map.get_mut(&r.node_id) {
            match rank {
                0 => *total += 0.05,
                1 | 2 => *total += 0.02,
                _ => {}
            }
        }
    }

    let mut fused: Vec<(SearchResult, f64)> = score_map.into_values().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    fused
        .into_iter()
        .map(|(mut result, score)| {
            result.score = score;
            result
        })
        .collect()
}

/// Sanitize a user query for FTS5 MATCH syntax.
///
/// FTS5 has its own query grammar where characters like `*`, `"`, `-`,
/// `(`, `)` carry meaning. We strip those special characters from each
/// token and wrap it in double quotes for exact matching, then join
/// tokens with `OR` for broadest recall. RRF will rank appropriately.
pub fn sanitize_fts_query(query: &str) -> String {
    let tokens: Vec<String> = query
        .split_whitespace()
        .filter_map(|token| {
            let clean: String = token
                .chars()
                .filter(|c| {
                    !matches!(
                        c,
                        '*' | '"' | '(' | ')' | '{' | '}' | '[' | ']' | '^' | '~' | ':'
                    )
                })
                .collect();
            if clean.is_empty() {
                None
            } else {
                Some(format!("\"{}\"", clean))
            }
        })
        .collect();

    if tokens.is_empty() {
        return String::new();
    }

    tokens.join(" OR ")
}

/// Build a short display snippet from a node's name, signature, and
/// doc comment.
///
/// Prefers the first line of documentation. Falls back to a compacted
/// signature (truncated at 120 chars). As a last resort, returns the
/// bare name.
pub fn build_snippet(name: &str, signature: Option<&str>, doc_comment: Option<&str>) -> String {
    if let Some(doc) = doc_comment {
        let first_line = doc.lines().next().unwrap_or("").trim();
        if !first_line.is_empty() {
            return first_line.to_string();
        }
    }
    if let Some(sig) = signature {
        // Show a compacted signature, truncated at 120 characters.
        let compacted: String = sig
            .chars()
            .take(120)
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if sig.len() > 120 {
            return format!("{}...", compacted);
        }
        return compacted;
    }
    name.to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::initialize_database;
    use crate::graph::store::GraphStore;
    use crate::types::{CodeNode, Language, NodeKind};

    /// Spin up an in-memory store with the full schema applied.
    fn setup() -> GraphStore {
        let conn = initialize_database(":memory:").expect("schema init should succeed on :memory:");
        GraphStore::from_connection(conn)
    }

    /// Build a minimal test node.
    fn make_node(
        id: &str,
        name: &str,
        file: &str,
        kind: NodeKind,
        line: u32,
        sig: Option<&str>,
        doc: Option<&str>,
    ) -> CodeNode {
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
            body: sig.map(|s| s.to_string()),
            documentation: doc.map(|d| d.to_string()),
            exported: Some(true),
        }
    }

    // -- sanitize_fts_query ------------------------------------------------

    #[test]
    fn sanitize_fts_query_basic_tokens() {
        let result = sanitize_fts_query("hello world");
        assert_eq!(result, r#""hello" OR "world""#);
    }

    #[test]
    fn sanitize_fts_query_strips_special_chars() {
        let result = sanitize_fts_query("foo* (bar) baz:qux");
        assert_eq!(result, r#""foo" OR "bar" OR "bazqux""#);
    }

    #[test]
    fn sanitize_fts_query_empty_input() {
        assert_eq!(sanitize_fts_query(""), "");
        assert_eq!(sanitize_fts_query("   "), "");
    }

    #[test]
    fn sanitize_fts_query_all_special_chars() {
        // When every character is a special char, the result should be empty.
        assert_eq!(sanitize_fts_query("*** \"\" ()"), "");
    }

    #[test]
    fn sanitize_fts_query_single_token() {
        assert_eq!(sanitize_fts_query("search"), r#""search""#);
    }

    // -- build_snippet -----------------------------------------------------

    #[test]
    fn build_snippet_prefers_doc_comment() {
        let snippet = build_snippet(
            "foo",
            Some("fn foo(x: i32) -> bool"),
            Some("Check something.\nMore details."),
        );
        assert_eq!(snippet, "Check something.");
    }

    #[test]
    fn build_snippet_falls_back_to_signature() {
        let snippet = build_snippet("foo", Some("fn foo(x: i32) -> bool"), None);
        assert_eq!(snippet, "fn foo(x: i32) -> bool");
    }

    #[test]
    fn build_snippet_truncates_long_signature() {
        let long_sig = "a".repeat(200);
        let snippet = build_snippet("foo", Some(&long_sig), None);
        // Should be 120 chars + "..."
        assert!(snippet.ends_with("..."));
        assert_eq!(snippet.len(), 123); // 120 'a' chars + 3 dots
    }

    #[test]
    fn build_snippet_falls_back_to_name() {
        let snippet = build_snippet("myFunction", None, None);
        assert_eq!(snippet, "myFunction");
    }

    #[test]
    fn build_snippet_skips_empty_doc_comment() {
        // A doc comment that's just whitespace should fall through.
        let snippet = build_snippet("bar", Some("fn bar()"), Some("  \n  "));
        assert_eq!(snippet, "fn bar()");
    }

    // -- fuse_results (RRF math) -------------------------------------------

    #[test]
    fn fuse_results_combines_scores_from_both_lists() {
        let fts = vec![
            SearchResult {
                node_id: "a".to_string(),
                name: "alpha".to_string(),
                kind: "function".to_string(),
                file_path: "a.ts".to_string(),
                score: 0.0,
                fts_score: Some(5.0),
                vec_score: None,
                snippet: None,
            },
            SearchResult {
                node_id: "b".to_string(),
                name: "beta".to_string(),
                kind: "class".to_string(),
                file_path: "b.ts".to_string(),
                score: 0.0,
                fts_score: Some(3.0),
                vec_score: None,
                snippet: None,
            },
        ];
        let vec_results = vec![
            SearchResult {
                node_id: "a".to_string(),
                name: "alpha".to_string(),
                kind: "function".to_string(),
                file_path: "a.ts".to_string(),
                score: 0.0,
                fts_score: None,
                vec_score: Some(0.95),
                snippet: None,
            },
            SearchResult {
                node_id: "c".to_string(),
                name: "gamma".to_string(),
                kind: "variable".to_string(),
                file_path: "c.ts".to_string(),
                score: 0.0,
                fts_score: None,
                vec_score: Some(0.80),
                snippet: None,
            },
        ];

        let fused = fuse_results(&fts, &vec_results, 60);

        // "a" appears in both lists so it should have the highest score.
        assert_eq!(fused[0].node_id, "a");
        // Verify the RRF math:
        //   FTS rank 0 -> 1/(60+1) = 1/61   + top-rank bonus 0.05
        //   Vec rank 0 -> 1/(60+1) = 1/61   + top-rank bonus 0.05
        //   Combined  -> 2/61 + 0.10
        let expected_a_score = 2.0 / 61.0 + 0.10;
        assert!(
            (fused[0].score - expected_a_score).abs() < 1e-10,
            "expected {}, got {}",
            expected_a_score,
            fused[0].score,
        );

        // "a" should carry both fts_score and vec_score.
        assert!(fused[0].fts_score.is_some());
        assert!(fused[0].vec_score.is_some());

        // Total results: 3 unique node IDs.
        assert_eq!(fused.len(), 3);
    }

    #[test]
    fn fuse_results_empty_inputs() {
        let fused = fuse_results(&[], &[], 60);
        assert!(fused.is_empty());
    }

    #[test]
    fn fuse_results_single_list_only() {
        let fts = vec![SearchResult {
            node_id: "x".to_string(),
            name: "x".to_string(),
            kind: "function".to_string(),
            file_path: "x.ts".to_string(),
            score: 0.0,
            fts_score: Some(1.0),
            vec_score: None,
            snippet: None,
        }];
        let fused = fuse_results(&fts, &[], 60);
        assert_eq!(fused.len(), 1);
        assert_eq!(fused[0].node_id, "x");
        // 1/(60+1) + top-rank bonus 0.05 for being #1 in FTS
        let expected = 1.0 / 61.0 + 0.05;
        assert!((fused[0].score - expected).abs() < 1e-10);
    }

    #[test]
    fn fuse_results_preserves_rank_ordering() {
        // Three items in FTS, none in vec. Their order should be preserved.
        let fts: Vec<SearchResult> = (0..3)
            .map(|i| SearchResult {
                node_id: format!("n{}", i),
                name: format!("name{}", i),
                kind: "function".to_string(),
                file_path: "f.ts".to_string(),
                score: 0.0,
                fts_score: Some((3 - i) as f64),
                vec_score: None,
                snippet: None,
            })
            .collect();

        let fused = fuse_results(&fts, &[], 60);
        assert_eq!(fused[0].node_id, "n0");
        assert_eq!(fused[1].node_id, "n1");
        assert_eq!(fused[2].node_id, "n2");
        // Scores must be strictly decreasing.
        assert!(fused[0].score > fused[1].score);
        assert!(fused[1].score > fused[2].score);
    }

    // -- keyword search (integration with FTS5) ----------------------------

    #[test]
    fn keyword_search_finds_matching_nodes() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:greet:1",
                "greet",
                "a.ts",
                NodeKind::Function,
                1,
                Some("function greet(name: string)"),
                Some("Say hello to someone."),
            ))
            .unwrap();
        store
            .upsert_node(&make_node(
                "fn:a.ts:farewell:10",
                "farewell",
                "a.ts",
                NodeKind::Function,
                10,
                Some("function farewell()"),
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let results = search.search_by_keyword("greet", 10).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node_id, "fn:a.ts:greet:1");
        assert_eq!(results[0].name, "greet");
        assert_eq!(results[0].kind, "function");
        assert_eq!(results[0].snippet.as_deref(), Some("Say hello to someone."));
    }

    #[test]
    fn keyword_search_returns_empty_for_no_match() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:greet:1",
                "greet",
                "a.ts",
                NodeKind::Function,
                1,
                None,
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let results = search.search_by_keyword("nonexistent", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn keyword_search_respects_limit() {
        let store = setup();
        for i in 0..10 {
            store
                .upsert_node(&make_node(
                    &format!("fn:a.ts:func{}:{}", i, i),
                    &format!("func{}", i),
                    "a.ts",
                    NodeKind::Function,
                    i,
                    Some(&format!("function func{}()", i)),
                    None,
                ))
                .unwrap();
        }

        let search = HybridSearch::new(&store.conn);
        // All nodes have "func" in their name; ask for at most 3.
        let results = search.search_by_keyword("func", 3).unwrap();
        assert!(results.len() <= 3);
    }

    #[test]
    fn keyword_search_with_special_chars_in_query() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:create:1",
                "create",
                "a.ts",
                NodeKind::Function,
                1,
                None,
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        // Special chars should be stripped, leaving just "create".
        let results = search.search_by_keyword("*create*", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "create");
    }

    // -- hybrid search (integration) ---------------------------------------

    #[test]
    fn hybrid_search_applies_node_type_filter() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:hello:1",
                "hello",
                "a.ts",
                NodeKind::Function,
                1,
                None,
                None,
            ))
            .unwrap();
        store
            .upsert_node(&make_node(
                "cls:a.ts:Hello:10",
                "Hello",
                "a.ts",
                NodeKind::Class,
                10,
                None,
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let opts = SearchOptions {
            node_type: Some("class".to_string()),
            ..Default::default()
        };
        let results = search.search("Hello", &opts).unwrap();
        assert!(results.iter().all(|r| r.kind == "class"));
    }

    #[test]
    fn hybrid_search_applies_language_filter() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:compute:1",
                "compute",
                "a.ts",
                NodeKind::Function,
                1,
                None,
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let opts = SearchOptions {
            language: Some("python".to_string()),
            ..Default::default()
        };
        // The node is TypeScript; filtering by Python should exclude it.
        let results = search.search("compute", &opts).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn hybrid_search_applies_min_score_filter() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:tiny:1",
                "tiny",
                "a.ts",
                NodeKind::Function,
                1,
                None,
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let opts = SearchOptions {
            min_score: Some(1.0), // impossibly high for a single RRF contribution
            ..Default::default()
        };
        let results = search.search("tiny", &opts).unwrap();
        // Max single-list RRF for rank 0 is 1/61 ~ 0.016, well below 1.0.
        assert!(results.is_empty());
    }

    // =====================================================================
    // NEW TESTS: Phase 18C — Search comprehensive coverage
    // =====================================================================

    #[test]
    fn keyword_search_multiple_matches() {
        let store = setup();
        // Insert 5 nodes that all share the exact word "dispatch" in their name.
        for i in 0..5 {
            store
                .upsert_node(&make_node(
                    &format!("fn:a.ts:dispatch{}:{}", i, i),
                    "dispatch",
                    &format!("src/file{}.ts", i),
                    NodeKind::Function,
                    i * 10,
                    Some(&format!("function dispatch(arg{})", i)),
                    None,
                ))
                .unwrap();
        }

        let search = HybridSearch::new(&store.conn);
        let results = search.search_by_keyword("dispatch", 10).unwrap();
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn keyword_search_empty_query_returns_empty() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:greet:1",
                "greet",
                "a.ts",
                NodeKind::Function,
                1,
                None,
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let results = search.search_by_keyword("", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn keyword_search_whitespace_query_returns_empty() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:greet:1",
                "greet",
                "a.ts",
                NodeKind::Function,
                1,
                None,
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let results = search.search_by_keyword("   ", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn fts_search_matches_doc_comment() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:calc:1",
                "calc",
                "a.ts",
                NodeKind::Function,
                1,
                Some("function calc(x: number)"),
                Some("Calculate the fibonacci number"),
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let results = search.search_by_keyword("fibonacci", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "calc");
    }

    #[test]
    fn fts_search_matches_signature() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:processOrder:1",
                "processOrder",
                "a.ts",
                NodeKind::Function,
                1,
                Some("function processOrder(order: Order): Promise<void>"),
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let results = search.search_by_keyword("processOrder", 10).unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn hybrid_search_default_limit() {
        let store = setup();
        for i in 0..30 {
            store
                .upsert_node(&make_node(
                    &format!("fn:a.ts:item{}:{}", i, i),
                    &format!("item{}", i),
                    "a.ts",
                    NodeKind::Function,
                    i,
                    Some(&format!("function item{}()", i)),
                    None,
                ))
                .unwrap();
        }

        let search = HybridSearch::new(&store.conn);
        let opts = SearchOptions::default();
        let results = search.search("item", &opts).unwrap();
        // Default limit is 20
        assert!(results.len() <= 20);
    }

    #[test]
    fn hybrid_search_applies_limit() {
        let store = setup();
        for i in 0..10 {
            store
                .upsert_node(&make_node(
                    &format!("fn:a.ts:data{}:{}", i, i),
                    &format!("data{}", i),
                    "a.ts",
                    NodeKind::Function,
                    i,
                    Some(&format!("function data{}()", i)),
                    None,
                ))
                .unwrap();
        }

        let search = HybridSearch::new(&store.conn);
        let opts = SearchOptions {
            limit: Some(3),
            ..Default::default()
        };
        let results = search.search("data", &opts).unwrap();
        assert!(results.len() <= 3);
    }

    #[test]
    fn fuse_results_with_k_parameter() {
        let fts = vec![SearchResult {
            node_id: "a".to_string(),
            name: "a".to_string(),
            kind: "function".to_string(),
            file_path: "a.ts".to_string(),
            score: 0.0,
            fts_score: Some(5.0),
            vec_score: None,
            snippet: None,
        }];

        // With k=10, rank 0 -> 1/(10+1) = 1/11 + top-rank bonus 0.05
        let fused10 = fuse_results(&fts, &[], 10);
        let expected10 = 1.0 / 11.0 + 0.05;
        assert!((fused10[0].score - expected10).abs() < 1e-10);

        // With k=100, rank 0 -> 1/(100+1) = 1/101 + top-rank bonus 0.05
        let fused100 = fuse_results(&fts, &[], 100);
        let expected100 = 1.0 / 101.0 + 0.05;
        assert!((fused100[0].score - expected100).abs() < 1e-10);
    }

    #[test]
    fn sanitize_fts_query_preserves_alphanumeric() {
        let result = sanitize_fts_query("abc123");
        assert_eq!(result, r#""abc123""#);
    }

    #[test]
    fn sanitize_fts_query_preserves_hyphens() {
        let result = sanitize_fts_query("my-function");
        assert_eq!(result, r#""my-function""#);
    }

    #[test]
    fn sanitize_fts_query_multiple_special_chars() {
        let result = sanitize_fts_query("[test] (value) {object}");
        assert_eq!(result, r#""test" OR "value" OR "object""#);
    }

    #[test]
    fn build_snippet_empty_doc_with_signature() {
        let snippet = build_snippet("foo", Some("fn foo()"), Some(""));
        // Empty doc string falls through to signature
        assert_eq!(snippet, "fn foo()");
    }

    #[test]
    fn search_result_scores_descending() {
        let fts: Vec<SearchResult> = (0..5)
            .map(|i| SearchResult {
                node_id: format!("n{}", i),
                name: format!("name{}", i),
                kind: "function".to_string(),
                file_path: "f.ts".to_string(),
                score: 0.0,
                fts_score: Some((5 - i) as f64),
                vec_score: None,
                snippet: None,
            })
            .collect();

        let fused = fuse_results(&fts, &[], 60);
        for i in 1..fused.len() {
            assert!(
                fused[i].score <= fused[i - 1].score,
                "scores should be descending"
            );
        }
    }

    #[test]
    fn hybrid_search_combined_filters() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:compute:1",
                "compute",
                "a.ts",
                NodeKind::Function,
                1,
                Some("function compute()"),
                None,
            ))
            .unwrap();
        store
            .upsert_node(&make_node(
                "cls:b.ts:Computer:1",
                "Computer",
                "b.ts",
                NodeKind::Class,
                1,
                Some("class Computer"),
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let opts = SearchOptions {
            node_type: Some("function".to_string()),
            language: Some("typescript".to_string()),
            ..Default::default()
        };
        let results = search.search("comput", &opts).unwrap();
        assert!(results.iter().all(|r| r.kind == "function"));
    }

    #[test]
    fn search_by_similarity_returns_empty_without_embeddings() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:greet:1",
                "greet",
                "a.ts",
                NodeKind::Function,
                1,
                None,
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let results = search.search_by_similarity("hello", 10);
        // Without the embedding feature, this always returns empty
        assert!(results.is_empty());
    }

    #[test]
    fn hybrid_search_min_score_zero_keeps_all() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:foo:1",
                "foo",
                "a.ts",
                NodeKind::Function,
                1,
                None,
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let opts = SearchOptions {
            min_score: Some(0.0),
            ..Default::default()
        };
        let results = search.search("foo", &opts).unwrap();
        assert_eq!(results.len(), 1);
    }

    // =====================================================================
    // Query intent detection tests
    // =====================================================================

    #[test]
    fn intent_camel_case_is_symbol() {
        assert_eq!(
            detect_query_intent("getUserById"),
            QueryIntent::SymbolLookup
        );
    }

    #[test]
    fn intent_snake_case_is_symbol() {
        assert_eq!(
            detect_query_intent("get_user_by_id"),
            QueryIntent::SymbolLookup
        );
    }

    #[test]
    fn intent_pascal_case_is_symbol() {
        assert_eq!(
            detect_query_intent("PaymentService"),
            QueryIntent::SymbolLookup
        );
    }

    #[test]
    fn intent_dot_notation_is_symbol() {
        assert_eq!(
            detect_query_intent("user.getName"),
            QueryIntent::SymbolLookup
        );
    }

    #[test]
    fn intent_rust_path_is_symbol() {
        assert_eq!(
            detect_query_intent("std::collections::HashMap"),
            QueryIntent::SymbolLookup
        );
    }

    #[test]
    fn intent_natural_language_is_semantic() {
        assert_eq!(
            detect_query_intent("how does the authentication work"),
            QueryIntent::SemanticSearch
        );
    }

    #[test]
    fn intent_question_with_common_words_is_semantic() {
        assert_eq!(
            detect_query_intent("find all functions that handle errors"),
            QueryIntent::SemanticSearch
        );
    }

    #[test]
    fn intent_multi_word_question_is_semantic() {
        assert_eq!(
            detect_query_intent("what is the main entry point for this application"),
            QueryIntent::SemanticSearch
        );
    }

    #[test]
    fn intent_empty_is_hybrid() {
        assert_eq!(detect_query_intent(""), QueryIntent::Hybrid);
    }

    #[test]
    fn intent_whitespace_is_hybrid() {
        assert_eq!(detect_query_intent("   "), QueryIntent::Hybrid);
    }

    #[test]
    fn intent_single_word_is_hybrid() {
        // Single lowercase word without symbol indicators — ambiguous
        let intent = detect_query_intent("initialize");
        assert_eq!(intent, QueryIntent::Hybrid);
    }

    #[test]
    fn intent_screaming_snake_is_symbol() {
        assert_eq!(
            detect_query_intent("MAX_RETRY_COUNT"),
            QueryIntent::SymbolLookup
        );
    }

    #[test]
    fn intent_mixed_symbol_and_words() {
        // "getUserById method" — has camelCase but also a space
        let intent = detect_query_intent("getUserById method");
        // camelCase signals are strong, should still lean symbol or hybrid
        assert!(intent == QueryIntent::SymbolLookup || intent == QueryIntent::Hybrid);
    }

    #[test]
    fn intent_two_plain_words_is_hybrid() {
        // "user service" — has spaces but also short, no strong semantic signal
        let intent = detect_query_intent("user service");
        assert!(intent == QueryIntent::Hybrid || intent == QueryIntent::SemanticSearch);
    }

    #[test]
    fn intent_blend_weights_symbol() {
        let w: BlendWeights = QueryIntent::SymbolLookup.into();
        assert!((w.fts_weight - 0.8).abs() < f64::EPSILON);
        assert!((w.vec_weight - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn intent_blend_weights_semantic() {
        let w: BlendWeights = QueryIntent::SemanticSearch.into();
        assert!((w.fts_weight - 0.3).abs() < f64::EPSILON);
        assert!((w.vec_weight - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn intent_blend_weights_hybrid() {
        let w: BlendWeights = QueryIntent::Hybrid.into();
        assert!((w.fts_weight - 1.0).abs() < f64::EPSILON);
        assert!((w.vec_weight - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn intent_dotted_path_no_spaces() {
        assert_eq!(
            detect_query_intent("config.database.host"),
            QueryIntent::SymbolLookup
        );
    }

    #[test]
    fn intent_with_where_keyword_is_semantic() {
        assert_eq!(
            detect_query_intent("where is the database connection created"),
            QueryIntent::SemanticSearch
        );
    }

    // =====================================================================
    // Weighted fusion tests
    // =====================================================================

    #[test]
    fn fuse_weighted_symbol_boosts_fts() {
        let fts = vec![SearchResult {
            node_id: "fts_only".to_string(),
            name: "ftsOnly".to_string(),
            kind: "function".to_string(),
            file_path: "a.ts".to_string(),
            score: 0.0,
            fts_score: Some(5.0),
            vec_score: None,
            snippet: None,
        }];
        let vec_r = vec![SearchResult {
            node_id: "vec_only".to_string(),
            name: "vecOnly".to_string(),
            kind: "function".to_string(),
            file_path: "b.ts".to_string(),
            score: 0.0,
            fts_score: None,
            vec_score: Some(0.9),
            snippet: None,
        }];

        let weights = BlendWeights {
            fts_weight: 0.8,
            vec_weight: 0.2,
        };
        let fused = fuse_results_weighted(&fts, &vec_r, &[], 60, weights);
        assert_eq!(fused.len(), 2);
        // FTS-only result should score higher with symbol weights
        let fts_item = fused.iter().find(|r| r.node_id == "fts_only").unwrap();
        let vec_item = fused.iter().find(|r| r.node_id == "vec_only").unwrap();
        assert!(
            fts_item.score > vec_item.score,
            "FTS result should rank higher with SymbolLookup weights"
        );
    }

    #[test]
    fn fuse_weighted_semantic_boosts_vec() {
        let fts = vec![SearchResult {
            node_id: "fts_only".to_string(),
            name: "ftsOnly".to_string(),
            kind: "function".to_string(),
            file_path: "a.ts".to_string(),
            score: 0.0,
            fts_score: Some(5.0),
            vec_score: None,
            snippet: None,
        }];
        let vec_r = vec![SearchResult {
            node_id: "vec_only".to_string(),
            name: "vecOnly".to_string(),
            kind: "function".to_string(),
            file_path: "b.ts".to_string(),
            score: 0.0,
            fts_score: None,
            vec_score: Some(0.9),
            snippet: None,
        }];

        let weights = BlendWeights {
            fts_weight: 0.3,
            vec_weight: 0.7,
        };
        let fused = fuse_results_weighted(&fts, &vec_r, &[], 60, weights);
        let fts_item = fused.iter().find(|r| r.node_id == "fts_only").unwrap();
        let vec_item = fused.iter().find(|r| r.node_id == "vec_only").unwrap();
        assert!(
            vec_item.score > fts_item.score,
            "Vec result should rank higher with SemanticSearch weights"
        );
    }

    #[test]
    fn fuse_weighted_expansion_uses_half_fts_weight() {
        let expansion = vec![SearchResult {
            node_id: "exp".to_string(),
            name: "expanded".to_string(),
            kind: "function".to_string(),
            file_path: "e.ts".to_string(),
            score: 0.0,
            fts_score: Some(3.0),
            vec_score: None,
            snippet: None,
        }];

        let w = BlendWeights {
            fts_weight: 0.8,
            vec_weight: 0.2,
        };
        let fused = fuse_results_weighted(&[], &[], &expansion, 60, w);
        assert_eq!(fused.len(), 1);
        // Expansion weight = 0.8 * 0.5 = 0.4; rank 0 -> 0.4 / 61
        let expected = 0.4 / 61.0;
        assert!(
            (fused[0].score - expected).abs() < 1e-10,
            "expected {}, got {}",
            expected,
            fused[0].score,
        );
    }

    // =====================================================================
    // File-level search tests
    // =====================================================================

    #[test]
    fn search_files_groups_by_file() {
        let store = setup();
        // Two files, each with nodes containing "dispatch"
        store
            .upsert_node(&make_node(
                "fn:a.ts:dispatch:1",
                "dispatch",
                "src/a.ts",
                NodeKind::Function,
                1,
                Some("function dispatch()"),
                None,
            ))
            .unwrap();
        store
            .upsert_node(&make_node(
                "fn:a.ts:dispatchEvent:10",
                "dispatchEvent",
                "src/a.ts",
                NodeKind::Function,
                10,
                Some("function dispatchEvent()"),
                None,
            ))
            .unwrap();
        store
            .upsert_node(&make_node(
                "fn:b.ts:dispatch:1",
                "dispatch",
                "src/b.ts",
                NodeKind::Function,
                1,
                Some("function dispatch()"),
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let results = search.search_files("dispatch", 10).unwrap();

        assert_eq!(results.len(), 2, "should have 2 files");

        // First result should be the file with more matches (src/a.ts)
        let a_file = results.iter().find(|r| r.file_path == "src/a.ts").unwrap();
        assert_eq!(a_file.matched_symbols, 2);
        assert!(a_file.top_symbols.contains(&"dispatch".to_string()));

        let b_file = results.iter().find(|r| r.file_path == "src/b.ts").unwrap();
        assert_eq!(b_file.matched_symbols, 1);
    }

    #[test]
    fn search_files_empty_query() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:foo:1",
                "foo",
                "a.ts",
                NodeKind::Function,
                1,
                None,
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let results = search.search_files("", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn search_files_respects_limit() {
        let store = setup();
        for i in 0..10 {
            store
                .upsert_node(&make_node(
                    &format!("fn:f{}.ts:handler:{}", i, i),
                    "handler",
                    &format!("src/f{}.ts", i),
                    NodeKind::Function,
                    1,
                    Some("function handler()"),
                    None,
                ))
                .unwrap();
        }

        let search = HybridSearch::new(&store.conn);
        let results = search.search_files("handler", 3).unwrap();
        assert!(results.len() <= 3);
    }

    #[test]
    fn search_files_top_symbols_limited_to_five() {
        let store = setup();
        // Insert 8 symbols in the same file — use underscore names so FTS5 tokenizes "render" separately
        let suffixes = [
            "component",
            "page",
            "header",
            "footer",
            "sidebar",
            "modal",
            "button",
            "icon",
        ];
        for (i, suffix) in suffixes.iter().enumerate() {
            store
                .upsert_node(&make_node(
                    &format!("fn:a.ts:render_{}:{}", suffix, i),
                    &format!("render_{}", suffix),
                    "a.ts",
                    NodeKind::Function,
                    (i as u32) * 10,
                    Some(&format!("function render_{}()", suffix)),
                    None,
                ))
                .unwrap();
        }

        let search = HybridSearch::new(&store.conn);
        let results = search.search_files("render", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(
            results[0].top_symbols.len() <= 5,
            "top_symbols should be capped at 5"
        );
        assert_eq!(results[0].matched_symbols, 8);
    }

    #[test]
    fn search_files_no_match() {
        let store = setup();
        store
            .upsert_node(&make_node(
                "fn:a.ts:foo:1",
                "foo",
                "a.ts",
                NodeKind::Function,
                1,
                None,
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let results = search.search_files("zzz_nonexistent", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn search_files_relevance_ordering() {
        let store = setup();
        // File "heavy.ts" has 5 matches, "light.ts" has 1
        for i in 0..5 {
            store
                .upsert_node(&make_node(
                    &format!("fn:heavy.ts:process{}:{}", i, i),
                    "process",
                    "heavy.ts",
                    NodeKind::Function,
                    i * 10,
                    Some("function process()"),
                    None,
                ))
                .unwrap();
        }
        store
            .upsert_node(&make_node(
                "fn:light.ts:process:1",
                "process",
                "light.ts",
                NodeKind::Function,
                1,
                Some("function process()"),
                None,
            ))
            .unwrap();

        let search = HybridSearch::new(&store.conn);
        let results = search.search_files("process", 10).unwrap();
        assert_eq!(results.len(), 2);
        // File with higher aggregate score should come first
        assert!(
            results[0].relevance_score >= results[1].relevance_score,
            "results should be sorted by relevance descending"
        );
    }
}

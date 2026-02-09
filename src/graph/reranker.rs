//! Cross-encoder re-ranking for deep search.
//!
//! Uses fastembed's `TextRerank` (BAAI/bge-reranker-base) to score each
//! (query, document) pair through a cross-encoder transformer, producing
//! much higher-quality relevance scores than bi-encoder cosine similarity
//! or BM25 alone.
//!
//! The pipeline is: hybrid search (top-N candidates) → cross-encoder
//! re-rank → return top-K.  Cross-encoders are slower than bi-encoders
//! (they encode the pair jointly) but dramatically more accurate because
//! they see both query and document together.
//!
//! Feature-gated behind `reranking`.  Requires the `embedding` feature
//! (and thus fastembed) to be enabled.
//!
//! ## Cargo.toml changes needed
//!
//! ```toml
//! [features]
//! default = ["embedding"]
//! embedding = ["dep:fastembed"]
//! reranking = ["embedding"]           # cross-encoder depends on fastembed
//! ```

#[cfg(feature = "reranking")]
use fastembed::{RerankInitOptions, RerankResult, TextRerank};

use crate::graph::search::SearchResult;

// ---------------------------------------------------------------------------
// Reranker
// ---------------------------------------------------------------------------

/// Cross-encoder reranker powered by BAAI/bge-reranker-base via fastembed.
///
/// Wraps fastembed's `TextRerank` model.  Expensive to construct (downloads
/// ~300 MB model on first use) but fast to call (`rerank` is batched).
#[cfg(feature = "reranking")]
pub struct Reranker {
    model: TextRerank,
}

#[cfg(feature = "reranking")]
impl Reranker {
    /// Create a new cross-encoder reranker.
    ///
    /// Downloads BAAI/bge-reranker-base (~300 MB ONNX) on first use;
    /// subsequent calls load from the fastembed cache directory.
    pub fn try_new() -> Result<Self, String> {
        let options = RerankInitOptions::default().with_show_download_progress(true);
        let model = TextRerank::try_new(options)
            .map_err(|e| format!("Failed to initialize reranker: {e}"))?;
        Ok(Self { model })
    }

    /// Re-rank search results against a query using the cross-encoder.
    ///
    /// Each candidate's text representation is scored jointly with the
    /// query. Returns at most `top_k` results, re-ordered by cross-encoder
    /// score (descending).
    ///
    /// The original `SearchResult.score` is replaced with the cross-encoder
    /// score (f32 cast to f64).  The `fts_score` and `vec_score` fields
    /// are preserved from the original results for provenance.
    pub fn rerank(
        &self,
        query: &str,
        results: &[SearchResult],
        top_k: usize,
    ) -> Result<Vec<SearchResult>, String> {
        if results.is_empty() || top_k == 0 {
            return Ok(Vec::new());
        }

        // Build document texts for the cross-encoder.
        let documents: Vec<String> = results
            .iter()
            .map(|r| search_result_to_rerank_text(r))
            .collect();

        let doc_refs: Vec<&str> = documents.iter().map(|s| s.as_str()).collect();

        let reranked: Vec<RerankResult> = self
            .model
            .rerank(query, doc_refs, false, None)
            .map_err(|e| format!("Reranking failed: {e}"))?;

        // Map back to SearchResult, keeping at most top_k.
        let limit = top_k.min(reranked.len());
        let mut output = Vec::with_capacity(limit);

        for rr in reranked.into_iter().take(limit) {
            let original = &results[rr.index];
            let mut reranked_result = original.clone();
            reranked_result.score = rr.score as f64;
            output.push(reranked_result);
        }

        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Deep search compositor
// ---------------------------------------------------------------------------

/// Compose hybrid search + cross-encoder re-ranking into a single
/// "deep search" pipeline.
///
/// Takes pre-computed hybrid search results (typically top 20-30 from RRF
/// fusion) and re-ranks them through the cross-encoder, returning at most
/// `top_k` results with much higher relevance precision.
///
/// This is the function that the MCP `codegraph_deep_query` tool should
/// call after obtaining hybrid search results.
#[cfg(feature = "reranking")]
pub fn deep_search(
    query: &str,
    reranker: &Reranker,
    search_results: Vec<SearchResult>,
    top_k: usize,
) -> Result<Vec<SearchResult>, String> {
    if search_results.is_empty() {
        return Ok(Vec::new());
    }
    reranker.rerank(query, &search_results, top_k)
}

// ---------------------------------------------------------------------------
// Text construction
// ---------------------------------------------------------------------------

/// Build a text representation of a `SearchResult` for the cross-encoder.
///
/// Combines name, kind, file path, and snippet into a single string that
/// gives the reranker enough signal to judge relevance.
///
/// Format: `"{kind} {name} in {file_path}: {snippet}"`
fn search_result_to_rerank_text(result: &SearchResult) -> String {
    let snippet = result.snippet.as_deref().unwrap_or("");
    format!(
        "{} {} in {}: {}",
        result.kind, result.name, result.file_path, snippet
    )
    .trim()
    .to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a minimal SearchResult for testing.
    fn make_result(
        node_id: &str,
        name: &str,
        kind: &str,
        file_path: &str,
        score: f64,
        snippet: Option<&str>,
    ) -> SearchResult {
        SearchResult {
            node_id: node_id.to_string(),
            name: name.to_string(),
            kind: kind.to_string(),
            file_path: file_path.to_string(),
            score,
            fts_score: Some(score),
            vec_score: None,
            snippet: snippet.map(|s| s.to_string()),
        }
    }

    // -- search_result_to_rerank_text -----------------------------------------

    #[test]
    fn rerank_text_includes_all_fields() {
        let r = make_result(
            "fn:a.ts:hello:1",
            "hello",
            "function",
            "src/greet.ts",
            0.5,
            Some("Say hello to someone"),
        );
        let text = search_result_to_rerank_text(&r);
        assert!(text.contains("function"));
        assert!(text.contains("hello"));
        assert!(text.contains("src/greet.ts"));
        assert!(text.contains("Say hello to someone"));
    }

    #[test]
    fn rerank_text_without_snippet() {
        let r = make_result("fn:a.ts:foo:1", "foo", "function", "a.ts", 0.3, None);
        let text = search_result_to_rerank_text(&r);
        assert_eq!(text, "function foo in a.ts:");
    }

    #[test]
    fn rerank_text_trims_whitespace() {
        let r = make_result("cls:b.ts:Bar:1", "Bar", "class", "b.ts", 0.2, Some(""));
        let text = search_result_to_rerank_text(&r);
        // Trailing colon + space from empty snippet, trimmed
        assert_eq!(text, "class Bar in b.ts:");
    }

    // -- Reranker (integration, feature-gated) --------------------------------

    #[cfg(feature = "reranking")]
    #[test]
    fn reranker_reranks_known_good_bad_pair() {
        let reranker = match Reranker::try_new() {
            Ok(r) => r,
            Err(_) => return, // Skip if model unavailable
        };

        let results = vec![
            make_result(
                "fn:a.ts:parse_json:1",
                "parse_json",
                "function",
                "src/parser.ts",
                0.8,
                Some("Parse a JSON string into an object"),
            ),
            make_result(
                "fn:b.ts:send_email:1",
                "send_email",
                "function",
                "src/email.ts",
                0.9, // higher original score
                Some("Send an email notification to users"),
            ),
        ];

        let reranked = reranker.rerank("how to parse JSON", &results, 2).unwrap();

        // Cross-encoder should rank the JSON parser higher than the email sender,
        // regardless of original scores.
        assert_eq!(reranked.len(), 2);
        assert_eq!(reranked[0].name, "parse_json");
    }

    #[cfg(feature = "reranking")]
    #[test]
    fn reranker_respects_top_k() {
        let reranker = match Reranker::try_new() {
            Ok(r) => r,
            Err(_) => return,
        };

        let results: Vec<SearchResult> = (0..5)
            .map(|i| {
                make_result(
                    &format!("fn:a.ts:func{}:1", i),
                    &format!("func{}", i),
                    "function",
                    "a.ts",
                    0.5,
                    Some(&format!("Function number {}", i)),
                )
            })
            .collect();

        let reranked = reranker.rerank("function", &results, 2).unwrap();
        assert_eq!(reranked.len(), 2);
    }

    #[cfg(feature = "reranking")]
    #[test]
    fn reranker_empty_input() {
        let reranker = match Reranker::try_new() {
            Ok(r) => r,
            Err(_) => return,
        };
        let reranked = reranker.rerank("query", &[], 10).unwrap();
        assert!(reranked.is_empty());
    }

    #[cfg(feature = "reranking")]
    #[test]
    fn reranker_top_k_zero() {
        let reranker = match Reranker::try_new() {
            Ok(r) => r,
            Err(_) => return,
        };
        let results = vec![make_result(
            "fn:a.ts:foo:1",
            "foo",
            "function",
            "a.ts",
            0.5,
            None,
        )];
        let reranked = reranker.rerank("foo", &results, 0).unwrap();
        assert!(reranked.is_empty());
    }

    // -- deep_search ----------------------------------------------------------

    #[cfg(feature = "reranking")]
    #[test]
    fn deep_search_empty_input() {
        let reranker = match Reranker::try_new() {
            Ok(r) => r,
            Err(_) => return,
        };
        let result = deep_search("query", &reranker, Vec::new(), 10).unwrap();
        assert!(result.is_empty());
    }

    #[cfg(feature = "reranking")]
    #[test]
    fn deep_search_preserves_metadata() {
        let reranker = match Reranker::try_new() {
            Ok(r) => r,
            Err(_) => return,
        };

        let mut input = make_result(
            "fn:a.ts:hello:1",
            "hello",
            "function",
            "a.ts",
            0.5,
            Some("greeting function"),
        );
        input.vec_score = Some(0.9);

        let result = deep_search("hello", &reranker, vec![input], 10).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].node_id, "fn:a.ts:hello:1");
        assert_eq!(result[0].name, "hello");
        assert_eq!(result[0].kind, "function");
        assert_eq!(result[0].file_path, "a.ts");
        // fts_score and vec_score should be preserved
        assert!(result[0].fts_score.is_some());
        assert!(result[0].vec_score.is_some());
        // score should be from cross-encoder, not original
        assert!(result[0].score != 0.5);
    }

    // -- Non-feature-gated tests (always run) ---------------------------------

    #[test]
    fn search_result_to_rerank_text_format() {
        let r = make_result(
            "fn:x.rs:do_stuff:1",
            "do_stuff",
            "function",
            "src/lib.rs",
            1.0,
            Some("Does important stuff"),
        );
        let text = search_result_to_rerank_text(&r);
        assert_eq!(
            text,
            "function do_stuff in src/lib.rs: Does important stuff"
        );
    }

    #[test]
    fn search_result_to_rerank_text_with_class() {
        let r = make_result(
            "cls:app.py:UserService:1",
            "UserService",
            "class",
            "app/services.py",
            0.7,
            Some("Manages user operations"),
        );
        let text = search_result_to_rerank_text(&r);
        assert_eq!(
            text,
            "class UserService in app/services.py: Manages user operations"
        );
    }
}

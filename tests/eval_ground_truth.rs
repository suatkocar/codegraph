//! Ground-truth evaluation of CodeGraph against its own codebase.
//!
//! This test opens the pre-built `.codegraph/codegraph.db` database, loads
//! the ground-truth queries from `eval/ground-truth/codegraph.json`, runs the
//! evaluation harness, and writes measured metrics to `eval/results/`.
//!
//! Run with: `cargo test --test eval_ground_truth -- --nocapture`
//!
//! The test is gated on the database file existing (i.e., `codegraph index .`
//! must have been run first).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use codegraph::eval::harness::{load_ground_truth, run_evaluation, EvalMetrics};
use codegraph::graph::search::{HybridSearch, SearchOptions};
use codegraph::graph::store::GraphStore;

/// Open the pre-indexed CodeGraph database.
fn open_codegraph_db() -> Option<GraphStore> {
    let db_path = PathBuf::from(".codegraph/codegraph.db");
    if !db_path.exists() {
        eprintln!(
            "[eval] Database not found at {:?} — run `codegraph index .` first",
            db_path
        );
        return None;
    }
    match GraphStore::new(db_path.to_str().unwrap()) {
        Ok(store) => Some(store),
        Err(e) => {
            eprintln!("[eval] Failed to open database: {}", e);
            None
        }
    }
}

#[test]
fn ground_truth_full_evaluation() {
    let store = match open_codegraph_db() {
        Some(s) => s,
        None => {
            eprintln!("[eval] Skipping — database not available");
            return;
        }
    };

    let gt_path = PathBuf::from("eval/ground-truth/codegraph.json");
    assert!(
        gt_path.exists(),
        "Ground truth file not found at {:?}",
        gt_path
    );

    let ground_truth = load_ground_truth(&gt_path).unwrap();
    let report = run_evaluation(&store, &ground_truth);

    eprintln!("\n========================================");
    eprintln!("  CodeGraph v0.3.0 Evaluation Report");
    eprintln!("========================================");
    eprintln!(
        "  Node count OK:  {} (min {})",
        report.node_count_ok, ground_truth.expected_node_count_min
    );
    eprintln!(
        "  Edge count OK:  {} (min {})",
        report.edge_count_ok, ground_truth.expected_edge_count_min
    );
    eprintln!("  ---");
    eprintln!(
        "  Search   — P: {:.3}, R: {:.3}, F1: {:.3}",
        report.search_metrics.precision, report.search_metrics.recall, report.search_metrics.f1
    );
    eprintln!(
        "  Callers  — P: {:.3}, R: {:.3}, F1: {:.3}",
        report.caller_metrics.precision, report.caller_metrics.recall, report.caller_metrics.f1
    );
    eprintln!(
        "  Dead     — P: {:.3}, R: {:.3}, F1: {:.3}",
        report.dead_code_metrics.precision,
        report.dead_code_metrics.recall,
        report.dead_code_metrics.f1
    );
    eprintln!(
        "  Deps     — P: {:.3}, R: {:.3}, F1: {:.3}",
        report.dependency_metrics.precision,
        report.dependency_metrics.recall,
        report.dependency_metrics.f1
    );
    eprintln!("  ---");
    eprintln!(
        "  Overall  — P: {:.3}, R: {:.3}, F1: {:.3}",
        report.overall.precision, report.overall.recall, report.overall.f1
    );
    eprintln!("========================================\n");

    // Write results to JSON
    let results_dir = Path::new("eval/results");
    std::fs::create_dir_all(results_dir).unwrap();
    let results_path = results_dir.join("codegraph-v030-harness.json");
    let json = serde_json::to_string_pretty(&report).unwrap();
    std::fs::write(&results_path, &json).unwrap();
    eprintln!("[eval] Results written to {:?}", results_path);

    // Assertions
    assert!(report.node_count_ok, "Node count below expected minimum");
    assert!(report.edge_count_ok, "Edge count below expected minimum");
}

/// Detailed per-query search evaluation with MRR (Mean Reciprocal Rank).
#[test]
fn ground_truth_search_with_mrr() {
    let store = match open_codegraph_db() {
        Some(s) => s,
        None => {
            eprintln!("[eval] Skipping — database not available");
            return;
        }
    };

    let gt_path = PathBuf::from("eval/ground-truth/codegraph.json");
    let ground_truth = load_ground_truth(&gt_path).unwrap();
    let search = HybridSearch::new(&store.conn);

    let mut total_precision = 0.0;
    let mut total_recall = 0.0;
    let mut total_reciprocal_rank = 0.0;
    let query_count = ground_truth.search_queries.len();

    eprintln!("\n========================================");
    eprintln!("  Per-Query Search Evaluation (MRR)");
    eprintln!("========================================");

    for sq in &ground_truth.search_queries {
        let opts = SearchOptions {
            limit: Some(10),
            ..Default::default()
        };
        let results = search.search(&sq.query, &opts).unwrap_or_default();

        let actual_symbols: HashSet<String> = results.iter().map(|r| r.name.clone()).collect();
        let expected_symbols: HashSet<String> = sq.expected_top5_symbols.iter().cloned().collect();
        let actual_files: HashSet<String> = results.iter().map(|r| r.file_path.clone()).collect();
        let expected_files: HashSet<String> = sq.expected_top5_files.iter().cloned().collect();

        let symbol_metrics = EvalMetrics::compute(&expected_symbols, &actual_symbols);
        let file_metrics = EvalMetrics::compute(&expected_files, &actual_files);

        // MRR: find the rank of the first expected symbol in the results
        let mut reciprocal_rank = 0.0;
        for (rank, result) in results.iter().enumerate() {
            if expected_symbols.contains(&result.name) {
                reciprocal_rank = 1.0 / (rank as f64 + 1.0);
                break;
            }
        }

        total_precision += symbol_metrics.precision;
        total_recall += symbol_metrics.recall;
        total_reciprocal_rank += reciprocal_rank;

        eprintln!(
            "  {:30} | Sym P:{:.2} R:{:.2} F1:{:.2} | File P:{:.2} R:{:.2} | RR:{:.3}",
            sq.query,
            symbol_metrics.precision,
            symbol_metrics.recall,
            symbol_metrics.f1,
            file_metrics.precision,
            file_metrics.recall,
            reciprocal_rank,
        );

        // Show top 5 actual results for debugging
        for (i, r) in results.iter().take(5).enumerate() {
            let marker = if expected_symbols.contains(&r.name) {
                "+"
            } else {
                " "
            };
            eprintln!(
                "    {}{}. {} ({}) — {}",
                marker,
                i + 1,
                r.name,
                r.kind,
                r.file_path
            );
        }
    }

    let n = query_count as f64;
    let avg_precision = total_precision / n;
    let avg_recall = total_recall / n;
    let mrr = total_reciprocal_rank / n;

    eprintln!("\n  ----------------------------------------");
    eprintln!("  Avg Precision: {:.3}", avg_precision);
    eprintln!("  Avg Recall:    {:.3}", avg_recall);
    eprintln!("  MRR:           {:.3}", mrr);
    eprintln!("  ----------------------------------------\n");

    // Write detailed results
    let results_dir = Path::new("eval/results");
    std::fs::create_dir_all(results_dir).unwrap();

    let detailed = serde_json::json!({
        "version": "0.3.0",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "database": ".codegraph/codegraph.db",
        "queries": query_count,
        "search_metrics": {
            "avg_precision": format!("{:.3}", avg_precision),
            "avg_recall": format!("{:.3}", avg_recall),
            "mrr": format!("{:.3}", mrr),
        }
    });

    let path = results_dir.join("codegraph-v030-search.json");
    std::fs::write(&path, serde_json::to_string_pretty(&detailed).unwrap()).unwrap();
    eprintln!("[eval] Detailed search results written to {:?}", path);

    // MRR should be reasonable — the expected symbols should appear in top results
    assert!(mrr > 0.1, "MRR too low: {:.3}", mrr);
}

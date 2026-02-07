//! Structured logging, observability, and security utilities.
//!
//! This module provides:
//! - [`init_logging`] — One-time structured logging setup with `RUST_LOG` support
//! - [`validate_path`] — Path traversal prevention for MCP tool inputs
//! - [`redact_secrets`] — Secret pattern redaction for tool output
//! - [`Metrics`] — Lightweight performance metrics collector

use std::path::{Path, PathBuf};

use regex::Regex;
use tracing_subscriber::EnvFilter;

/// Initialize structured logging with `RUST_LOG` environment variable support.
///
/// Defaults to `codegraph=info` when `RUST_LOG` is not set. Call once at
/// program startup — subsequent calls are silently ignored by
/// `tracing_subscriber`.
pub fn init_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("codegraph=info"));

    // try_init so double-init in tests doesn't panic
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .try_init();
}

/// Validate a file path to prevent path traversal attacks.
///
/// Joins `path` onto `project_root`, canonicalizes both, and checks that the
/// result still lives under the root. Returns the canonical path on success.
pub fn validate_path(path: &str, project_root: &Path) -> Result<PathBuf, String> {
    let requested = project_root.join(path);
    let canonical = requested
        .canonicalize()
        .map_err(|e| format!("Path not found: {}: {}", path, e))?;

    let root_canonical = project_root
        .canonicalize()
        .map_err(|e| format!("Invalid project root: {}", e))?;

    if !canonical.starts_with(&root_canonical) {
        return Err(format!(
            "Path traversal detected: {} escapes project root",
            path
        ));
    }

    Ok(canonical)
}

/// Redact potential secrets from text.
///
/// Replaces patterns that look like API keys, tokens, passwords, AWS
/// credentials, and Bearer tokens with `***REDACTED***`.
pub fn redact_secrets(text: &str) -> String {
    let patterns: &[(&str, &str)] = &[
        (
            r#"(?i)(api[_-]?key|apikey)\s*[:=]\s*['"]?([a-zA-Z0-9_\-]{20,})['"]?"#,
            "$1=***REDACTED***",
        ),
        (
            r#"(?i)(password|passwd|pwd)\s*[:=]\s*['"]?([^\s'"]{8,})['"]?"#,
            "$1=***REDACTED***",
        ),
        (
            r#"(?i)(secret|token)\s*[:=]\s*['"]?([a-zA-Z0-9_\-]{20,})['"]?"#,
            "$1=***REDACTED***",
        ),
        (
            r#"(?i)(aws_access_key_id)\s*[:=]\s*['"]?(AKIA[0-9A-Z]{16})['"]?"#,
            "$1=***REDACTED***",
        ),
        (
            r#"(?i)(aws_secret_access_key)\s*[:=]\s*['"]?([a-zA-Z0-9/+]{40})['"]?"#,
            "$1=***REDACTED***",
        ),
        (
            r"(?i)Bearer\s+[a-zA-Z0-9_\-\.]{20,}",
            "Bearer ***REDACTED***",
        ),
        (
            r#"(?i)(connection_string|conn_str)\s*[:=]\s*['"]?([^\s'"]{20,})['"]?"#,
            "$1=***REDACTED***",
        ),
    ];

    let mut result = text.to_string();
    for (pattern, replacement) in patterns {
        if let Ok(re) = Regex::new(pattern) {
            result = re.replace_all(&result, *replacement).to_string();
        }
    }
    result
}

/// Lightweight performance metrics collector.
///
/// Tracks indexing performance, graph sizes, and cache hit rates.
/// Serializable to JSON via [`Metrics::to_json`].
pub struct Metrics {
    pub indexing_duration_ms: Option<u64>,
    pub files_indexed: usize,
    pub nodes_extracted: usize,
    pub edges_extracted: usize,
    pub search_queries: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            indexing_duration_ms: None,
            files_indexed: 0,
            nodes_extracted: 0,
            edges_extracted: 0,
            search_queries: 0,
            cache_hits: 0,
            cache_misses: 0,
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "indexing_duration_ms": self.indexing_duration_ms,
            "files_indexed": self.files_indexed,
            "nodes_extracted": self.nodes_extracted,
            "edges_extracted": self.edges_extracted,
            "search_queries": self.search_queries,
            "cache_hits": self.cache_hits,
            "cache_misses": self.cache_misses,
        })
    }

    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            return 0.0;
        }
        self.cache_hits as f64 / total as f64
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -- init_logging -------------------------------------------------------

    #[test]
    fn init_logging_does_not_panic() {
        init_logging();
        // Second call should also not panic (try_init ignores re-init).
        init_logging();
    }

    // -- validate_path ------------------------------------------------------

    #[test]
    fn validate_path_accepts_valid_relative_path() {
        let tmp = TempDir::new().unwrap();
        let child = tmp.path().join("src");
        std::fs::create_dir_all(&child).unwrap();

        let result = validate_path("src", tmp.path());
        assert!(result.is_ok());
        assert!(result.unwrap().starts_with(tmp.path().canonicalize().unwrap()));
    }

    #[test]
    fn validate_path_rejects_traversal_attempt() {
        let tmp = TempDir::new().unwrap();
        let result = validate_path("../../../etc/passwd", tmp.path());
        // Either the path doesn't exist (Err) or it escapes root (Err).
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("Path traversal") || err.contains("Path not found"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn validate_path_rejects_absolute_path_outside_root() {
        let tmp = TempDir::new().unwrap();
        let result = validate_path("/tmp", tmp.path());
        // /tmp exists but is outside the project root.
        assert!(result.is_err());
    }

    #[test]
    fn validate_path_rejects_nonexistent_path() {
        let tmp = TempDir::new().unwrap();
        let result = validate_path("nonexistent/deep/path", tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Path not found"));
    }

    #[test]
    fn validate_path_accepts_nested_valid_path() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&nested).unwrap();

        let result = validate_path("a/b/c", tmp.path());
        assert!(result.is_ok());
    }

    #[test]
    fn validate_path_with_dot_segments_inside_root() {
        let tmp = TempDir::new().unwrap();
        let child = tmp.path().join("src").join("lib");
        std::fs::create_dir_all(&child).unwrap();

        // src/lib/../lib resolves to src/lib — still inside root.
        let result = validate_path("src/lib/../lib", tmp.path());
        assert!(result.is_ok());
    }

    // -- redact_secrets -----------------------------------------------------

    #[test]
    fn redact_api_key() {
        let input = "config: api_key=rk_skey_abcdefghij1234567890";
        let output = redact_secrets(input);
        assert!(output.contains("***REDACTED***"));
        assert!(!output.contains("rk_skey_abcdefghij1234567890"));
    }

    #[test]
    fn redact_password() {
        let input = "password=SuperSecretPass123!";
        let output = redact_secrets(input);
        assert!(output.contains("***REDACTED***"));
        assert!(!output.contains("SuperSecretPass123!"));
    }

    #[test]
    fn redact_bearer_token() {
        let input = "Authorization: Bearer fakejwtheader0000000000000000000000.payload.signature";
        let output = redact_secrets(input);
        assert!(output.contains("Bearer ***REDACTED***"));
        assert!(!output.contains("fakejwtheader0000000000000000000000"));
    }

    #[test]
    fn redact_aws_access_key() {
        let input = "aws_access_key_id=FKIAEXAMPLEKEY000000";
        let output = redact_secrets(input);
        assert!(output.contains("***REDACTED***"));
        assert!(!output.contains("FKIAEXAMPLEKEY000000"));
    }

    #[test]
    fn redact_aws_secret_key() {
        let input = "aws_secret_access_key=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
        let output = redact_secrets(input);
        assert!(output.contains("***REDACTED***"));
        assert!(!output.contains("wJalrXUtnFEMI"));
    }

    #[test]
    fn redact_secret_token() {
        let input = "secret=ghx_FaKeToKeNxFoRxTeSt_000000000000";
        let output = redact_secrets(input);
        assert!(output.contains("***REDACTED***"));
        assert!(!output.contains("ghx_FaKeToKeNxFoRxTeSt_000000000000"));
    }

    #[test]
    fn redact_connection_string() {
        let input = "connection_string=Server=mydb.server.com;Database=prod;User=admin;Password=s3cr3t";
        let output = redact_secrets(input);
        assert!(output.contains("***REDACTED***"));
    }

    #[test]
    fn no_false_positives_on_normal_text() {
        let input = "This is a normal log message about processing 42 files in 100ms.";
        let output = redact_secrets(input);
        assert_eq!(input, output, "normal text should be unchanged");
    }

    #[test]
    fn no_false_positives_on_short_password() {
        // Short passwords (< 8 chars) should NOT be redacted.
        let input = "pwd=abc";
        let output = redact_secrets(input);
        assert_eq!(input, output);
    }

    #[test]
    fn no_false_positives_on_code_identifiers() {
        let input = "let api_key_length = calculate_length(); // not a secret";
        let output = redact_secrets(input);
        assert_eq!(input, output);
    }

    #[test]
    fn redact_multiple_secrets_in_same_text() {
        let input = "api_key=rk_skey_aaaabbbbccccddddeeee password=MyS3cr3tP@ss!";
        let output = redact_secrets(input);
        assert!(!output.contains("rk_skey_aaaabbbbccccddddeeee"));
        assert!(!output.contains("MyS3cr3tP@ss!"));
    }

    // -- Metrics ------------------------------------------------------------

    #[test]
    fn metrics_new_has_zero_values() {
        let m = Metrics::new();
        assert_eq!(m.files_indexed, 0);
        assert_eq!(m.nodes_extracted, 0);
        assert_eq!(m.edges_extracted, 0);
        assert_eq!(m.search_queries, 0);
        assert_eq!(m.cache_hits, 0);
        assert_eq!(m.cache_misses, 0);
        assert!(m.indexing_duration_ms.is_none());
    }

    #[test]
    fn metrics_default_equals_new() {
        let a = Metrics::new();
        let b = Metrics::default();
        assert_eq!(a.files_indexed, b.files_indexed);
        assert_eq!(a.search_queries, b.search_queries);
        assert_eq!(a.indexing_duration_ms, b.indexing_duration_ms);
    }

    #[test]
    fn metrics_to_json_contains_all_fields() {
        let mut m = Metrics::new();
        m.files_indexed = 100;
        m.nodes_extracted = 500;
        m.edges_extracted = 1200;
        m.indexing_duration_ms = Some(450);
        m.search_queries = 10;
        m.cache_hits = 7;
        m.cache_misses = 3;

        let json = m.to_json();
        assert_eq!(json["files_indexed"], 100);
        assert_eq!(json["nodes_extracted"], 500);
        assert_eq!(json["edges_extracted"], 1200);
        assert_eq!(json["indexing_duration_ms"], 450);
        assert_eq!(json["search_queries"], 10);
        assert_eq!(json["cache_hits"], 7);
        assert_eq!(json["cache_misses"], 3);
    }

    #[test]
    fn metrics_to_json_null_duration() {
        let m = Metrics::new();
        let json = m.to_json();
        assert!(json["indexing_duration_ms"].is_null());
    }

    #[test]
    fn metrics_cache_hit_rate() {
        let mut m = Metrics::new();
        m.cache_hits = 7;
        m.cache_misses = 3;
        let rate = m.cache_hit_rate();
        assert!((rate - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn metrics_cache_hit_rate_zero_total() {
        let m = Metrics::new();
        assert_eq!(m.cache_hit_rate(), 0.0);
    }
}

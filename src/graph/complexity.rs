//! Cyclomatic and cognitive complexity analysis.
//!
//! Calculates complexity metrics for functions by analyzing source text.
//! No tree-sitter required — uses keyword counting with nesting awareness.

use rusqlite::Connection;

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

/// Complexity metrics for a single function.
#[derive(Debug, Clone)]
pub struct ComplexityResult {
    pub node_id: String,
    pub name: String,
    pub file_path: String,
    /// McCabe cyclomatic complexity: 1 + number of branch points.
    pub cyclomatic: u32,
    /// Cognitive complexity: nesting-aware, penalizes deeply nested branches.
    pub cognitive: u32,
    /// Number of lines in the function body.
    pub line_count: u32,
}

// ---------------------------------------------------------------------------
// Keyword tables
// ---------------------------------------------------------------------------

/// Branch keywords that increment cyclomatic complexity by 1.
const BRANCH_KEYWORDS: &[&str] = &[
    "if", "else if", "elif", "else", "for", "while", "loop", "match",
    "case", "catch", "except", "?",
];

/// Logical operators that increment cyclomatic complexity.
const LOGICAL_OPS: &[&str] = &["&&", "||"];

/// Keywords that increment cognitive complexity AND increase nesting.
const NESTING_KEYWORDS: &[&str] = &[
    "if", "else if", "elif", "for", "while", "loop", "match", "switch",
    "try", "catch", "except",
];

/// Keywords that increment cognitive complexity but do NOT nest.
const FLAT_INCREMENT_KEYWORDS: &[&str] = &["else", "case", "break", "continue", "?"];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Calculate complexity for a function by analyzing its body text.
///
/// Uses keyword counting for cyclomatic complexity and nesting-depth
/// penalties for cognitive complexity.
pub fn calculate_complexity(
    name: &str,
    body: &str,
    file_path: &str,
    node_id: &str,
) -> ComplexityResult {
    let line_count = body.lines().count().max(1) as u32;
    let cyclomatic = compute_cyclomatic(body);
    let cognitive = compute_cognitive(body);

    ComplexityResult {
        node_id: node_id.to_string(),
        name: name.to_string(),
        file_path: file_path.to_string(),
        cyclomatic,
        cognitive,
        line_count,
    }
}

/// Calculate complexity for all functions stored in the graph.
///
/// Reads node bodies from the database and computes metrics for each
/// function/method node that has a body.
pub fn calculate_all_complexities(conn: &Connection) -> Vec<ComplexityResult> {
    let sql = "\
        SELECT n.id, n.name, n.file_path, n.metadata
        FROM nodes n
        WHERE n.type IN ('function', 'method')";

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let rows = match stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let name: String = row.get(1)?;
        let file_path: String = row.get(2)?;
        let metadata_json: Option<String> = row.get(3)?;
        Ok((id, name, file_path, metadata_json))
    }) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    for row in rows.flatten() {
        let (id, name, file_path, metadata_json) = row;
        let body = metadata_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| v.get("body").and_then(|b| b.as_str()).map(String::from))
            .unwrap_or_default();

        if body.is_empty() {
            continue;
        }

        results.push(calculate_complexity(&name, &body, &file_path, &id));
    }

    results
}

// ---------------------------------------------------------------------------
// Internal: cyclomatic complexity
// ---------------------------------------------------------------------------

/// Cyclomatic complexity = 1 + branch_points.
fn compute_cyclomatic(body: &str) -> u32 {
    let mut cc: u32 = 1;

    for line in body.lines() {
        let trimmed = line.trim();

        // Count branch keywords (whole-word match via boundary check).
        for &kw in BRANCH_KEYWORDS {
            cc += count_keyword_occurrences(trimmed, kw);
        }

        // Count logical operators.
        for &op in LOGICAL_OPS {
            cc += count_substr_occurrences(trimmed, op);
        }
    }

    cc
}

// ---------------------------------------------------------------------------
// Internal: cognitive complexity
// ---------------------------------------------------------------------------

/// Cognitive complexity: each control-flow keyword adds (1 + current_nesting).
/// Nesting keywords also push the nesting depth for their block.
fn compute_cognitive(body: &str) -> u32 {
    let mut cog: u32 = 0;

    // Track nesting by brace depth relative to control-flow keywords.
    // Simplified approach: use indentation level as a proxy for nesting.
    let base_indent = body
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Estimate nesting depth from indentation.
        let indent = line.len() - line.trim_start().len();
        let nesting = if indent > base_indent {
            ((indent - base_indent) / 2).min(10) as u32 // assume 2-space indent
        } else {
            0
        };

        // Nesting keywords: increment by (1 + nesting).
        for &kw in NESTING_KEYWORDS {
            let count = count_keyword_occurrences(trimmed, kw);
            cog += count * (1 + nesting);
        }

        // Flat-increment keywords: increment by 1 (no nesting penalty).
        for &kw in FLAT_INCREMENT_KEYWORDS {
            cog += count_keyword_occurrences(trimmed, kw);
        }

        // Logical operators: increment by 1 each.
        for &op in LOGICAL_OPS {
            cog += count_substr_occurrences(trimmed, op);
        }
    }

    cog
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Count whole-word occurrences of a keyword in a line.
///
/// A "word boundary" means the character before/after the keyword is NOT
/// alphanumeric or underscore. This prevents matching "elif" inside
/// "elif_handler" etc.
fn count_keyword_occurrences(line: &str, keyword: &str) -> u32 {
    if keyword.len() > line.len() {
        return 0;
    }

    // For operators like "?" — just count substring occurrences.
    if !keyword.chars().next().map_or(false, |c| c.is_alphabetic()) {
        return count_substr_occurrences(line, keyword);
    }

    let mut count = 0u32;
    let bytes = line.as_bytes();
    let kw_bytes = keyword.as_bytes();
    let kw_len = kw_bytes.len();

    let mut start = 0;
    while start + kw_len <= bytes.len() {
        if let Some(pos) = line[start..].find(keyword) {
            let abs_pos = start + pos;
            let before_ok = abs_pos == 0
                || !is_ident_char(bytes[abs_pos - 1]);
            let after_ok = abs_pos + kw_len >= bytes.len()
                || !is_ident_char(bytes[abs_pos + kw_len]);

            if before_ok && after_ok {
                count += 1;
            }
            start = abs_pos + kw_len;
        } else {
            break;
        }
    }

    count
}

/// Count raw substring occurrences.
fn count_substr_occurrences(line: &str, needle: &str) -> u32 {
    line.matches(needle).count() as u32
}

/// Check if a byte is an identifier character (alphanumeric or underscore).
fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // calculate_complexity: simple function
    // -------------------------------------------------------------------

    #[test]
    fn simple_function_has_complexity_one() {
        let body = "function greet() { return 'hello'; }";
        let result = calculate_complexity("greet", body, "src/lib.ts", "fn:greet:1");

        assert_eq!(result.cyclomatic, 1, "no branches = CC of 1");
        assert_eq!(result.cognitive, 0, "no nesting = cognitive 0");
        assert_eq!(result.line_count, 1);
    }

    // -------------------------------------------------------------------
    // calculate_complexity: single if
    // -------------------------------------------------------------------

    #[test]
    fn single_if_increments_both_metrics() {
        let body = "\
fn check(x: i32) -> bool {
    if x > 0 {
        return true;
    }
    false
}";
        let result = calculate_complexity("check", body, "src/lib.rs", "fn:check:1");

        // CC = 1 (base) + 1 (if) = 2
        assert_eq!(result.cyclomatic, 2);
        // Cognitive: if at nesting 1 => 1 + 1 = 2
        assert!(result.cognitive >= 1, "cognitive should be >= 1 for an if");
    }

    // -------------------------------------------------------------------
    // calculate_complexity: nested ifs
    // -------------------------------------------------------------------

    #[test]
    fn nested_ifs_penalize_cognitive_more() {
        let body = "\
function validate(x, y) {
  if (x > 0) {
    if (y > 0) {
      return true;
    }
  }
  return false;
}";
        let result = calculate_complexity("validate", body, "src/lib.js", "fn:validate:1");

        // CC = 1 + 2 (two ifs) = 3
        assert_eq!(result.cyclomatic, 3);
        // Cognitive: outer if + inner if (with nesting penalty) > just 2
        assert!(
            result.cognitive > result.cyclomatic - 1,
            "cognitive should penalize nesting: got {}",
            result.cognitive
        );
    }

    // -------------------------------------------------------------------
    // calculate_complexity: match/switch
    // -------------------------------------------------------------------

    #[test]
    fn match_cases_increment_complexity() {
        let body = "\
fn describe(x: i32) -> &str {
    match x {
        1 => \"one\",
        2 => \"two\",
        _ => \"other\",
    }
}";
        let result = calculate_complexity("describe", body, "src/lib.rs", "fn:describe:1");

        // match keyword + implicit branches
        assert!(result.cyclomatic >= 2, "match should increment CC");
    }

    // -------------------------------------------------------------------
    // calculate_complexity: loops
    // -------------------------------------------------------------------

    #[test]
    fn loops_increment_complexity() {
        let body = "\
def process(items):
    for item in items:
        while item.active:
            item.step()
    return items";
        let result = calculate_complexity("process", body, "lib.py", "fn:process:1");

        // CC = 1 + 1 (for) + 1 (while) = 3
        assert_eq!(result.cyclomatic, 3);
        // Cognitive should be > 2 due to nested while inside for
        assert!(result.cognitive >= 2);
    }

    // -------------------------------------------------------------------
    // calculate_complexity: logical operators
    // -------------------------------------------------------------------

    #[test]
    fn logical_operators_increment_complexity() {
        let body = "if (a && b || c) { doSomething(); }";
        let result = calculate_complexity("check", body, "src/lib.js", "fn:check:1");

        // CC = 1 (base) + 1 (if) + 1 (&&) + 1 (||) = 4
        assert_eq!(result.cyclomatic, 4);
    }

    // -------------------------------------------------------------------
    // calculate_complexity: empty body
    // -------------------------------------------------------------------

    #[test]
    fn empty_body_has_base_complexity() {
        let body = "";
        let result = calculate_complexity("noop", body, "src/lib.ts", "fn:noop:1");

        assert_eq!(result.cyclomatic, 1, "empty body = CC 1");
        assert_eq!(result.cognitive, 0, "empty body = cognitive 0");
        assert_eq!(result.line_count, 1, "empty body = 1 line (min)");
    }

    // -------------------------------------------------------------------
    // calculate_complexity: line count
    // -------------------------------------------------------------------

    #[test]
    fn line_count_is_accurate() {
        let body = "line1\nline2\nline3\nline4\nline5";
        let result = calculate_complexity("multi", body, "src/lib.ts", "fn:multi:1");
        assert_eq!(result.line_count, 5);
    }

    // -------------------------------------------------------------------
    // calculate_all_complexities: integration
    // -------------------------------------------------------------------

    #[test]
    fn calculate_all_from_database() {
        let conn = crate::db::schema::initialize_database(":memory:")
            .expect("schema init should succeed");

        let meta = serde_json::json!({
            "body": "function foo() {\n  if (x) {\n    return 1;\n  }\n  return 0;\n}"
        });

        conn.execute(
            "INSERT INTO nodes (id, type, name, file_path, start_line, end_line, language, source_hash, metadata) \
             VALUES ('fn:foo:1', 'function', 'foo', 'src/lib.js', 1, 6, 'javascript', 'h1', ?1)",
            [meta.to_string()],
        ).unwrap();

        // A node without body should be skipped.
        conn.execute(
            "INSERT INTO nodes (id, type, name, file_path, start_line, end_line, language, source_hash) \
             VALUES ('fn:bar:1', 'function', 'bar', 'src/lib.js', 10, 12, 'javascript', 'h2')",
            [],
        ).unwrap();

        let results = calculate_all_complexities(&conn);

        assert_eq!(results.len(), 1, "only nodes with body should be analyzed");
        assert_eq!(results[0].name, "foo");
        assert!(results[0].cyclomatic >= 2, "foo has an if");
    }

    // -------------------------------------------------------------------
    // count_keyword_occurrences: word boundary
    // -------------------------------------------------------------------

    #[test]
    fn keyword_boundary_avoids_substrings() {
        // "for" should not match inside "performance"
        assert_eq!(count_keyword_occurrences("performance", "for"), 0);
        // "if" should not match inside "elif"
        assert_eq!(count_keyword_occurrences("elif x:", "if"), 0);
        // "if" standalone
        assert_eq!(count_keyword_occurrences("if (x > 0)", "if"), 1);
        // "else if" in a line
        assert_eq!(count_keyword_occurrences("} else if (y) {", "else if"), 1);
    }
}

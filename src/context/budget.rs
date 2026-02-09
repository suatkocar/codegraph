//! Token budget utilities for context assembly.
//!
//! Ports the TypeScript `context/budget.ts` to Rust. Provides lightweight
//! token estimation and text-shaping helpers that let the assembler pack
//! as much relevant code as possible into a fixed token budget without
//! exceeding it.

// ---------------------------------------------------------------------------
// Token estimation
// ---------------------------------------------------------------------------

/// Estimate the number of tokens in `text` using a character-class heuristic
/// tuned for source code.
///
/// Unlike the naive `len / 4` rule, this scans character by character and
/// counts:
/// - **Words** (alphanumeric/underscore runs) — each is ~1 token
/// - **Operators/punctuation** (individual special chars) — each is ~1 token
/// - **String literals** (content between quotes) — estimated at `len / 4`
/// - **Whitespace** — free (0 tokens)
///
/// This is more accurate for code because `{` is 1 token (not 0.25), and
/// `processUserInput` is ~1 token (not ~6).
pub fn estimate_tokens(text: &str) -> usize {
    estimate_tokens_heuristic(text)
}

/// Character-class token estimation heuristic for source code.
///
/// Walks through the text and classifies character sequences:
/// - Alphanumeric/underscore runs → 1 token each
/// - Individual punctuation/operator characters → 1 token each
/// - Quoted string literal content → `content_len / 4` tokens (rounded up)
/// - Whitespace → 0 tokens
///
/// Returns an estimate that is typically more accurate than `len / 4` for
/// code, producing lower counts for operator-heavy and identifier-heavy text.
pub fn estimate_tokens_heuristic(text: &str) -> usize {
    let mut tokens: usize = 0;
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        // Skip whitespace — free.
        if ch.is_whitespace() {
            i += 1;
            continue;
        }

        // String literals: content between matching quotes.
        if ch == '"' || ch == '\'' || ch == '`' {
            let quote = ch;
            tokens += 1; // The opening quote itself is ~1 token.
            i += 1;
            let content_start = i;
            while i < len && chars[i] != quote {
                // Skip escaped characters.
                if chars[i] == '\\' && i + 1 < len {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            let content_len = i - content_start;
            if content_len > 0 {
                tokens += content_len.div_ceil(4);
            }
            if i < len {
                // Closing quote — already accounted for with opening.
                i += 1;
            }
            continue;
        }

        // Word: alphanumeric or underscore run → 1 token.
        if ch.is_alphanumeric() || ch == '_' {
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens += 1;
            continue;
        }

        // Operator / punctuation: each is ~1 token.
        tokens += 1;
        i += 1;
    }

    tokens
}

// ---------------------------------------------------------------------------
// Truncation
// ---------------------------------------------------------------------------

/// Truncate `text` to fit within `max_tokens`, preserving whole lines.
///
/// Walks the text line by line, accumulating tokens until adding the next
/// line would exceed the budget. Returns everything up to (and including)
/// the last line that fits. If even the first line exceeds the budget, it
/// is included anyway so the caller always gets *something*.
pub fn truncate_to_fit(text: &str, max_tokens: usize) -> String {
    if max_tokens == 0 {
        return String::new();
    }

    if estimate_tokens(text) <= max_tokens {
        return text.to_string();
    }

    let mut result = String::new();
    let mut current_tokens: usize = 0;

    for (i, line) in text.lines().enumerate() {
        // +1 for the newline character that `lines()` strips.
        let line_tokens = estimate_tokens(line) + 1;

        if current_tokens + line_tokens > max_tokens && i > 0 {
            break;
        }

        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(line);
        current_tokens += line_tokens;
    }

    result
}

// ---------------------------------------------------------------------------
// Signature extraction
// ---------------------------------------------------------------------------

/// Extract the function/class signature from a full source body.
///
/// Strategies (tried in order):
///
/// 1. **Opening brace** -- find the first `{` and return everything before
///    it (trimmed), which captures `function foo(x: number): boolean` from
///    the full body.
/// 2. **Arrow function** -- find `=>` and return everything up to and
///    including the arrow.
/// 3. **First line fallback** -- return just the first line of the body.
///
/// Multi-line signatures (e.g. parameter lists that span lines) are
/// compacted into a single line with normalised whitespace.
pub fn signature_only(body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        return String::new();
    }

    // Strategy 1: find the opening brace.
    if let Some(brace_pos) = body.find('{') {
        let before_brace = body[..brace_pos].trim();
        if !before_brace.is_empty() {
            return compact_multiline(before_brace);
        }
    }

    // Strategy 2: arrow function (`=>`).
    if let Some(arrow_pos) = body.find("=>") {
        let through_arrow = &body[..arrow_pos + 2];
        return compact_multiline(through_arrow.trim());
    }

    // Strategy 3: first line.
    let first_line = body.lines().next().unwrap_or(body);
    compact_multiline(first_line.trim())
}

/// Collapse multi-line text into a single line with normalised whitespace.
///
/// Replaces every run of whitespace (including newlines) with a single
/// space, producing a compact one-liner suitable for summary display.
fn compact_multiline(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- estimate_tokens ---------------------------------------------------

    #[test]
    fn estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn estimate_tokens_short() {
        // Single word → 1 token
        assert_eq!(estimate_tokens("abcd"), 1);
    }

    #[test]
    fn estimate_tokens_single_word() {
        // Single word regardless of length → 1 token
        assert_eq!(estimate_tokens("abcde"), 1);
    }

    #[test]
    fn estimate_tokens_longer_text() {
        let text = "function hello(name: string): void";
        // function(1) hello(1) ((1) name(1) :(1) string(1) )(1) :(1) void(1) = 9
        assert_eq!(estimate_tokens(text), 9);
    }

    // -- truncate_to_fit ---------------------------------------------------

    #[test]
    fn truncate_fits_entirely() {
        let text = "short text";
        assert_eq!(truncate_to_fit(text, 100), text);
    }

    #[test]
    fn truncate_zero_budget() {
        assert_eq!(truncate_to_fit("anything", 0), "");
    }

    #[test]
    fn truncate_preserves_whole_lines() {
        let text = "line one\nline two\nline three\nline four";
        let result = truncate_to_fit(text, 6);
        // Each line ~2-3 tokens + 1 for newline.
        // The result should contain some lines but not all.
        assert!(result.lines().count() < text.lines().count());
        // Every line in the result should be a complete line from the input.
        for line in result.lines() {
            assert!(text.contains(line));
        }
    }

    #[test]
    fn truncate_always_includes_first_line() {
        let text = "this is a very long first line that exceeds any reasonable token budget by far";
        let result = truncate_to_fit(text, 1);
        assert_eq!(result, text);
    }

    #[test]
    fn truncate_multiline_budget_exact() {
        // Two short lines, budget that fits both exactly-ish.
        let text = "ab\ncd";
        // "ab" = 1 token + 1 newline = 2; "cd" = 1 token + 1 newline = 2; total ~4
        let result = truncate_to_fit(text, 100);
        assert_eq!(result, text);
    }

    // -- signature_only ----------------------------------------------------

    #[test]
    fn signature_from_function_body() {
        let body = "function greet(name: string): void {\n  console.log(name);\n}";
        assert_eq!(signature_only(body), "function greet(name: string): void");
    }

    #[test]
    fn signature_from_class_body() {
        let body = "class Foo extends Bar {\n  method() {}\n}";
        assert_eq!(signature_only(body), "class Foo extends Bar");
    }

    #[test]
    fn signature_from_arrow_function() {
        let body = "const add = (a: number, b: number) => a + b;";
        assert_eq!(
            signature_only(body),
            "const add = (a: number, b: number) =>"
        );
    }

    #[test]
    fn signature_multiline_params() {
        let body = "function create(\n  name: string,\n  age: number\n): Person {\n  return {};\n}";
        let sig = signature_only(body);
        // Should be compacted to a single line.
        assert!(!sig.contains('\n'));
        assert!(sig.contains("name: string,"));
        assert!(sig.contains("age: number"));
        assert!(sig.contains("): Person"));
    }

    #[test]
    fn signature_empty_body() {
        assert_eq!(signature_only(""), "");
    }

    #[test]
    fn signature_first_line_fallback() {
        // No brace, no arrow -- just returns the first line.
        let body = "const x = 42;";
        assert_eq!(signature_only(body), "const x = 42;");
    }

    // -- compact_multiline -------------------------------------------------

    #[test]
    fn compact_multiline_collapses_whitespace() {
        assert_eq!(compact_multiline("  hello\n  world  "), "hello world");
    }

    #[test]
    fn compact_multiline_single_line() {
        assert_eq!(compact_multiline("already compact"), "already compact");
    }

    // =====================================================================
    // NEW TESTS: Phase 18C — Budget comprehensive coverage
    // =====================================================================

    // -- estimate_tokens edge cases ---------------------------------------

    #[test]
    fn estimate_tokens_one_char() {
        assert_eq!(estimate_tokens("x"), 1);
    }

    #[test]
    fn estimate_tokens_four_chars() {
        assert_eq!(estimate_tokens("abcd"), 1);
    }

    #[test]
    fn estimate_tokens_five_chars() {
        // Single word → 1 token (heuristic counts words, not chars/4)
        assert_eq!(estimate_tokens("abcde"), 1);
    }

    #[test]
    fn estimate_tokens_eight_chars() {
        // Single word → 1 token
        assert_eq!(estimate_tokens("abcdefgh"), 1);
    }

    #[test]
    fn estimate_tokens_unicode() {
        let text = "hello world!"; // 12 chars -> ceil(12/4) = 3
        assert_eq!(estimate_tokens(text), 3);
    }

    #[test]
    fn estimate_tokens_code_snippet() {
        let code = "fn main() {\n    println!(\"Hello, world!\");\n}";
        let tokens = estimate_tokens(code);
        assert!(tokens > 0);
        // fn(1) main(1) ((1) )(1) {(1) println(1) !(1) ((1)
        // "→open_quote(1) + "Hello, world!" content(13)→ceil(13/4)=4
        // )(1) ;(1) }(1) = 16
        assert_eq!(tokens, 16);
    }

    // -- truncate_to_fit edge cases ---------------------------------------

    #[test]
    fn truncate_single_line_within_budget() {
        let text = "short";
        let result = truncate_to_fit(text, 10);
        assert_eq!(result, "short");
    }

    #[test]
    fn truncate_exact_budget() {
        let text = "abcd"; // 1 token
        let result = truncate_to_fit(text, 1);
        assert_eq!(result, "abcd");
    }

    #[test]
    fn truncate_multiple_lines_budget_for_two() {
        // Each word-per-line = 1 token, +1 for newline = 2 per line.
        // Use a budget of 3 so only 1 line fits (line 0: 2 tokens, line 1 would be 4 > 3).
        let text = "abcdefgh\nabcdefgh\nabcdefgh\nabcdefgh";
        let result = truncate_to_fit(text, 3);
        assert!(result.lines().count() < text.lines().count());
    }

    #[test]
    fn truncate_preserves_content() {
        let text = "line one\nline two\nline three";
        let result = truncate_to_fit(text, 6);
        for line in result.lines() {
            assert!(text.contains(line));
        }
    }

    #[test]
    fn truncate_empty_string() {
        assert_eq!(truncate_to_fit("", 10), "");
    }

    #[test]
    fn truncate_large_budget() {
        let text = "line1\nline2\nline3";
        let result = truncate_to_fit(text, 1000);
        assert_eq!(result, text);
    }

    // -- signature_only edge cases ----------------------------------------

    #[test]
    fn signature_rust_function() {
        let body =
            "pub fn process(data: &[u8]) -> Result<String, Error> {\n    Ok(String::new())\n}";
        let sig = signature_only(body);
        assert_eq!(sig, "pub fn process(data: &[u8]) -> Result<String, Error>");
    }

    #[test]
    fn signature_python_def() {
        // Python doesn't use braces, so signature extraction falls back
        let body = "def compute(x, y):\n    return x + y";
        let sig = signature_only(body);
        assert_eq!(sig, "def compute(x, y):");
    }

    #[test]
    fn signature_class_with_generics() {
        let body = "class Container<T> implements Iterable<T> {\n  items: T[] = [];\n}";
        let sig = signature_only(body);
        assert!(sig.contains("Container"));
        assert!(!sig.contains("items"));
    }

    #[test]
    fn signature_only_brace_body() {
        let body = "{\n  return 42;\n}";
        // Nothing before the brace, should fall back
        let sig = signature_only(body);
        assert!(!sig.is_empty());
    }

    #[test]
    fn signature_arrow_no_brace() {
        let body = "const fn = (x) => x * 2";
        let sig = signature_only(body);
        assert!(sig.contains("=>"));
    }

    #[test]
    fn signature_whitespace_only() {
        let sig = signature_only("   ");
        assert_eq!(sig, "");
    }

    // -- compact_multiline edge cases -------------------------------------

    #[test]
    fn compact_multiline_tabs_and_spaces() {
        assert_eq!(compact_multiline("\t  hello\t\n\t  world\t"), "hello world");
    }

    #[test]
    fn compact_multiline_empty() {
        assert_eq!(compact_multiline(""), "");
    }

    #[test]
    fn compact_multiline_whitespace_only() {
        assert_eq!(compact_multiline("   \n   \n   "), "");
    }

    #[test]
    fn compact_multiline_preserves_words() {
        let input = "function  greet(\n  name: string,\n  age: number\n)";
        let result = compact_multiline(input);
        assert!(result.contains("function"));
        assert!(result.contains("greet("));
        assert!(result.contains("name: string,"));
        assert!(result.contains("age: number"));
    }

    // =====================================================================
    // Heuristic token estimation — old vs new comparison tests
    // =====================================================================

    /// The old naive estimator for comparison purposes.
    fn estimate_tokens_naive(text: &str) -> usize {
        text.len().div_ceil(4)
    }

    #[test]
    fn heuristic_vs_naive_short_code() {
        // `fn foo() { return 1 + 2; }`
        // Old (naive): ceil(28/4) = 7
        // New (heuristic): fn(1) foo(1) ((1) )(1) {(1) return(1) 1(1) +(1) 2(1) ;(1) }(1) = 11
        let code = "fn foo() { return 1 + 2; }";
        let old = estimate_tokens_naive(code);
        let new = estimate_tokens_heuristic(code);
        assert_eq!(old, 7, "naive: ceil(26/4)");
        assert_eq!(new, 11, "heuristic: each word and operator is 1 token");
        // The heuristic is more accurate here — each operator/keyword IS a
        // separate token in real tokenizers, so the true count is closer to 11.
        assert!(
            new > old,
            "heuristic counts more tokens for operator-heavy code"
        );
    }

    #[test]
    fn heuristic_vs_naive_long_identifier() {
        // A single long camelCase identifier.
        // Old (naive): ceil(26/4) = 7
        // New (heuristic): 1 word = 1 token
        let ident = "processUserInputValidation";
        let old = estimate_tokens_naive(ident);
        let new = estimate_tokens_heuristic(ident);
        assert_eq!(old, 7, "naive: ceil(26/4) = 7");
        assert_eq!(new, 1, "heuristic: single word = 1 token");
        assert!(new < old, "heuristic much lower for long identifiers");
    }

    #[test]
    fn heuristic_vs_naive_string_heavy_code() {
        // Code with a long string literal.
        let code = r#"let msg = "This is a long error message for the user";"#;
        let old = estimate_tokens_naive(code);
        let new = estimate_tokens_heuristic(code);
        // Old: ceil(54/4) = 14
        // New: let(1) msg(1) =(1) "(1) content(ceil(41/4)=11) ;(1) = 16
        assert_eq!(old, 14);
        assert_eq!(new, 16);
    }

    #[test]
    fn heuristic_vs_naive_whitespace_heavy() {
        // Indented code — lots of whitespace.
        let code = "    if (x) {\n        return y;\n    }";
        let old = estimate_tokens_naive(code);
        let new = estimate_tokens_heuristic(code);
        // Old: ceil(35/4) = 9 (counts whitespace as chars)
        // New: if(1) ((1) x(1) )(1) {(1) return(1) y(1) ;(1) }(1) = 9
        assert_eq!(old, 9);
        assert_eq!(new, 9);
    }

    #[test]
    fn heuristic_vs_naive_dense_operators() {
        // Dense operator expression: `a+b*c-d/e%f`
        // Old: ceil(12/4) = 3
        // New: a(1) +(1) b(1) *(1) c(1) -(1) d(1) /(1) e(1) %(1) f(1) = 11
        let code = "a+b*c-d/e%f";
        let old = estimate_tokens_naive(code);
        let new = estimate_tokens_heuristic(code);
        assert_eq!(old, 3, "naive undercounts operator-dense code");
        assert_eq!(new, 11, "heuristic: every operator and var is a token");
    }

    #[test]
    fn heuristic_empty() {
        assert_eq!(estimate_tokens_heuristic(""), 0);
    }

    #[test]
    fn heuristic_whitespace_only() {
        assert_eq!(estimate_tokens_heuristic("   \n\t  "), 0);
    }

    #[test]
    fn heuristic_escaped_string() {
        // String with escape sequences.
        let code = r#""hello \"world\"""#;
        let tokens = estimate_tokens_heuristic(code);
        // "(1) → content: hello \"world\" → 14 chars → ceil(14/4) = 4
        assert_eq!(tokens, 5);
    }

    #[test]
    fn heuristic_mixed_real_code() {
        // A realistic Rust function.
        let code =
            "pub fn process(data: &[u8]) -> Result<String, Error> {\n    Ok(String::new())\n}";
        let new = estimate_tokens_heuristic(code);
        let old = estimate_tokens_naive(code);
        // Both should be reasonable, but heuristic accounts for structure.
        assert!(new > 0);
        assert!(old > 0);
        // The heuristic should give a different (more accurate) result.
        assert_ne!(old, new, "heuristic should differ from naive for real code");
    }
}

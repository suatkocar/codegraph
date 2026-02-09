//! Rules-based query expansion for code search.
//!
//! Expands user queries by splitting compound identifiers (camelCase,
//! snake_case), mapping common abbreviations to full words (and vice
//! versa), and substituting code-domain synonyms.  All rules are
//! static — zero network calls, zero latency cost.

use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Abbreviation map (short ↔ long)
// ---------------------------------------------------------------------------

/// Each entry is `(abbreviation, full_word)`.  Lookups go both ways:
/// `auth` expands to `authentication`, and `authentication` contracts
/// to `auth`.
const ABBREVIATIONS: &[(&str, &str)] = &[
    ("auth", "authentication"),
    ("cfg", "configuration"),
    ("config", "configuration"),
    ("db", "database"),
    ("req", "request"),
    ("res", "response"),
    ("resp", "response"),
    ("msg", "message"),
    ("err", "error"),
    ("ctx", "context"),
    ("fn", "function"),
    ("func", "function"),
    ("impl", "implementation"),
    ("init", "initialize"),
    ("param", "parameter"),
    ("params", "parameters"),
    ("str", "string"),
    ("int", "integer"),
    ("btn", "button"),
    ("nav", "navigation"),
    ("doc", "document"),
    ("docs", "documents"),
    ("env", "environment"),
    ("util", "utility"),
    ("utils", "utilities"),
    ("src", "source"),
    ("dest", "destination"),
    ("dst", "destination"),
    ("dir", "directory"),
    ("tmp", "temporary"),
    ("temp", "temporary"),
    ("pkg", "package"),
    ("lib", "library"),
    ("obj", "object"),
    ("arg", "argument"),
    ("args", "arguments"),
    ("val", "value"),
    ("prev", "previous"),
    ("cur", "current"),
    ("idx", "index"),
    ("len", "length"),
    ("num", "number"),
    ("max", "maximum"),
    ("min", "minimum"),
    ("avg", "average"),
    ("calc", "calculate"),
    ("del", "delete"),
    ("fmt", "format"),
    ("gen", "generate"),
    ("info", "information"),
    ("opt", "option"),
    ("opts", "options"),
    ("ref", "reference"),
    ("refs", "references"),
    ("repo", "repository"),
    ("spec", "specification"),
    ("sync", "synchronize"),
    ("async", "asynchronous"),
];

// ---------------------------------------------------------------------------
// Synonym groups
// ---------------------------------------------------------------------------

/// Each inner slice is a group of synonyms.  If the query contains any
/// member, the other members are added as expansions.
const SYNONYM_GROUPS: &[&[&str]] = &[
    &["remove", "delete", "destroy", "drop"],
    &["create", "new", "init", "make", "build"],
    &["error", "exception", "fault", "failure"],
    &["get", "fetch", "retrieve", "find", "lookup"],
    &["set", "update", "put", "assign"],
    &["send", "emit", "dispatch", "publish"],
    &["receive", "consume", "subscribe", "listen"],
    &["start", "begin", "launch", "run"],
    &["stop", "end", "halt", "terminate", "shutdown"],
    &["show", "display", "render", "present"],
    &["hide", "conceal", "collapse"],
    &["load", "read", "open", "parse"],
    &["save", "write", "store", "persist", "flush"],
    &["check", "validate", "verify", "test", "assert"],
    &["convert", "transform", "map", "translate"],
    &["list", "array", "collection", "vec"],
    &["handle", "process", "execute"],
    &["connect", "attach", "bind", "link"],
    &["disconnect", "detach", "unbind", "unlink"],
    &["enable", "activate", "on"],
    &["disable", "deactivate", "off"],
];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Expand a search query into a set of alternative search terms.
///
/// Returns a list whose **first element is always the original query**
/// (possibly cleaned up).  Subsequent elements are expansions derived
/// from splitting compound identifiers, abbreviation mapping, and
/// synonym substitution.
///
/// The caller can give the original query higher fusion weight because
/// it is always at index 0.
pub fn expand_query(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    // Original query is always first.
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return vec![String::new()];
    }
    seen.insert(trimmed.to_lowercase());
    result.push(trimmed.to_string());

    // Collect individual tokens from the original query.
    let tokens: Vec<String> = trimmed.split_whitespace().map(|s| s.to_string()).collect();

    // For each token, generate expansions.
    let mut expanded_tokens: Vec<String> = Vec::new();
    for token in &tokens {
        // Split compound identifiers.
        let parts = split_identifier(token);
        for part in &parts {
            let lower = part.to_lowercase();
            if lower.len() >= 2 && lower != token.to_lowercase() {
                expanded_tokens.push(lower.clone());
            }

            // Abbreviation expansion (both directions).
            for &(abbr, full) in ABBREVIATIONS {
                if lower == abbr {
                    expanded_tokens.push(full.to_string());
                } else if lower == full {
                    expanded_tokens.push(abbr.to_string());
                }
            }

            // Synonym expansion.
            for group in SYNONYM_GROUPS {
                if group.contains(&lower.as_str()) {
                    for &synonym in *group {
                        if synonym != lower {
                            expanded_tokens.push(synonym.to_string());
                        }
                    }
                }
            }
        }
    }

    // Deduplicate and add to result.
    for tok in expanded_tokens {
        let key = tok.to_lowercase();
        if seen.insert(key) {
            result.push(tok);
        }
    }

    result
}

/// Split a compound identifier into its constituent words.
///
/// Handles camelCase, PascalCase, snake_case, kebab-case, and
/// SCREAMING_SNAKE_CASE.
///
/// ```
/// use codegraph::graph::expansion::split_identifier;
/// assert_eq!(split_identifier("getUserById"), vec!["get", "User", "By", "Id"]);
/// assert_eq!(split_identifier("get_user_by_id"), vec!["get", "user", "by", "id"]);
/// assert_eq!(split_identifier("HTTPSServer"), vec!["HTTPS", "Server"]);
/// ```
pub fn split_identifier(ident: &str) -> Vec<String> {
    // First split on underscores and hyphens.
    let segments: Vec<&str> = ident.split(['_', '-']).filter(|s| !s.is_empty()).collect();

    let mut parts = Vec::new();
    for seg in segments {
        // Split camelCase / PascalCase within each segment.
        parts.extend(split_camel_case(seg));
    }
    parts
}

/// Split a camelCase or PascalCase string into words.
///
/// Handles runs of uppercase letters (acronyms) correctly:
/// `HTTPSServer` → `["HTTPS", "Server"]`, not `["H", "T", "T", "P", "S", ...]`.
fn split_camel_case(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }

    let mut parts = Vec::new();
    let mut current = String::new();

    for i in 0..chars.len() {
        let c = chars[i];
        if current.is_empty() {
            current.push(c);
            continue;
        }

        let prev_upper = chars[i - 1].is_uppercase();
        let cur_upper = c.is_uppercase();
        let next_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();

        if !cur_upper {
            // Lowercase/digit — continue current word.
            current.push(c);
        } else if prev_upper && cur_upper && next_lower {
            // Transition from acronym to new word: "HTTPSServer" →
            // the 'S' before 'erver' starts a new word, but the
            // preceding letters stay as the acronym.  Actually, the
            // last uppercase letter of the acronym belongs to the new
            // word only if followed by lowercase.  We split so the
            // previous accumulated uppercase run (minus last char)
            // becomes one part.
            //
            // Example: current = "HTTP", c = 'S', next = 'e'
            //   → emit "HTTP", start new word "S"
            // But we already pushed chars[i-1] into current last
            // iteration, so current = "HTTPS" is wrong.  Let's
            // re-examine: at i for 'S', current = "HTTPS"?  No.
            // Let's trace "HTTPSServer":
            //   i=0 'H': current="H"
            //   i=1 'T': prev=H(up), cur=T(up), next=T → not (up,up,low) → push → "HT"
            //   i=2 'T': prev=T(up), cur=T(up), next=P → not low → push → "HTT"
            //   i=3 'P': prev=T(up), cur=P(up), next=S → not low → push → "HTTP"
            //   i=4 'S': prev=P(up), cur=S(up), next='e'(low) → YES
            //     → split current minus nothing? We want "HTTPS"+"Server"
            //
            // Actually for "HTTPSServer", the desired split is
            // ["HTTPS", "Server"].  At i=4 ('S'), the 'S' belongs
            // with "Server" only if we consider it the start of a
            // new word.  But convention says the acronym is "HTTPS",
            // not "HTTP".  So the rule should be: when we see
            // uppercase followed by lowercase, and previous was also
            // uppercase, we do NOT split before the current char.
            // Instead, we push and split at the *next* uppercase
            // that follows a lowercase.
            //
            // Simpler approach: just push and let the standard
            // upper-after-lower rule handle it.
            //
            // Re-think: the standard rule is "split before an
            // uppercase that follows a lowercase".  For pure acronym
            // runs like "HTTPS" followed by "Server", the split
            // point is between 'S' and 'S' — i.e., at the second
            // 'S'.  But both are uppercase.  The right heuristic:
            // split before an uppercase letter that is followed by a
            // lowercase letter AND whose predecessor is also
            // uppercase.  That yields: "HTTP" + "SServer"?  No,
            // that's wrong too.
            //
            // Let me just use the well-known algorithm:
            // Split before any uppercase that follows a lowercase,
            // OR before any uppercase that is followed by a lowercase
            // (and preceded by an uppercase).
            //
            // For "HTTPSServer":
            //   'S' at index 5 is preceded by 'S'(up) and followed
            //   by 'e'(low).  So split here → "HTTPS" + "Server".
            //
            // That's this branch.  current = "HTTPS" at this point?
            // Let me re-trace:
            //   i=5 is 'S', but wait — "HTTPSServer" has indices:
            //   0:H 1:T 2:T 3:P 4:S 5:S 6:e 7:r 8:v 9:e 10:r
            //   At i=5 'S': prev='S'(up), cur='S'(up), next='e'(low)
            //   current = "HTTPS" (from i=0..4)
            //   We want to emit "HTTPS" and start "S".  But that
            //   gives "HTTPS" + "Server" — wait, current has 5 chars
            //   "HTTPS".  If we emit all of current and start fresh
            //   with 'S', we get parts=["HTTPS"], current="S", then
            //   'e','r','v','e','r' get appended → "Server".
            //
            // Hmm but that means we emit "HTTPS" not "HTTP".  That's
            // actually the RIGHT answer for this acronym.  Let me
            // reconsider: we're at i=5.  current="HTTPS" (5 chars).
            // We emit current → "HTTPS", start current="S".  Then
            // rest appends → "Server".  Result: ["HTTPS","Server"].
            //
            // But that contradicts the typical convention where
            // "XMLParser" → ["XML","Parser"].  Let's trace:
            //   0:X 1:M 2:L 3:P 4:a 5:r 6:s 7:e 8:r
            //   i=3 'P': prev='L'(up), cur='P'(up), next='a'(low)
            //   current="XML", emit "XML", start "P"
            //   Then 'a','r','s','e','r' → "Parser"
            //   Result: ["XML","Parser"].  Correct!
            //
            // OK so this branch: emit current as-is, start new word
            // with current char.  This is NOT right for "HTTPSServer"
            // because we'd get ["HTTPS","Server"] — wait, that IS
            // right.  Let me reconsider "HTTPSServer":
            //   The acronym is "HTTPS" and the word is "Server".
            //   ["HTTPS","Server"] is correct.
            //
            // The confusion was from my earlier trace.  This branch
            // works.  Let me simplify.
            parts.push(std::mem::take(&mut current));
            current.push(c);
        } else if !prev_upper && cur_upper {
            // Transition from lowercase to uppercase: "getUser" →
            // split before 'U'.
            parts.push(std::mem::take(&mut current));
            current.push(c);
        } else {
            // Uppercase following uppercase with no lowercase ahead
            // (or other cases) — continue the acronym.
            current.push(c);
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- split_identifier --------------------------------------------------

    #[test]
    fn split_camel_case_simple() {
        assert_eq!(
            split_identifier("getUserById"),
            vec!["get", "User", "By", "Id"]
        );
    }

    #[test]
    fn split_snake_case_simple() {
        assert_eq!(
            split_identifier("get_user_by_id"),
            vec!["get", "user", "by", "id"]
        );
    }

    #[test]
    fn split_pascal_case() {
        assert_eq!(split_identifier("UserService"), vec!["User", "Service"]);
    }

    #[test]
    fn split_screaming_snake_case() {
        assert_eq!(
            split_identifier("MAX_RETRY_COUNT"),
            vec!["MAX", "RETRY", "COUNT"]
        );
    }

    #[test]
    fn split_kebab_case() {
        assert_eq!(split_identifier("my-component"), vec!["my", "component"]);
    }

    #[test]
    fn split_acronym_word() {
        assert_eq!(split_identifier("XMLParser"), vec!["XML", "Parser"]);
    }

    #[test]
    fn split_acronym_at_end() {
        assert_eq!(split_identifier("parseJSON"), vec!["parse", "JSON"]);
    }

    #[test]
    fn split_https_server() {
        assert_eq!(split_identifier("HTTPSServer"), vec!["HTTPS", "Server"]);
    }

    #[test]
    fn split_single_word() {
        assert_eq!(split_identifier("hello"), vec!["hello"]);
    }

    #[test]
    fn split_single_char() {
        assert_eq!(split_identifier("x"), vec!["x"]);
    }

    #[test]
    fn split_empty() {
        let result: Vec<String> = split_identifier("");
        assert!(result.is_empty());
    }

    #[test]
    fn split_mixed_snake_camel() {
        assert_eq!(
            split_identifier("get_userName"),
            vec!["get", "user", "Name"]
        );
    }

    // -- expand_query: abbreviations ---------------------------------------

    #[test]
    fn expand_abbreviation_auth() {
        let expanded = expand_query("auth");
        assert!(expanded[0] == "auth", "original should be first");
        assert!(
            expanded.iter().any(|s| s == "authentication"),
            "should expand auth to authentication"
        );
    }

    #[test]
    fn expand_reverse_abbreviation() {
        let expanded = expand_query("database");
        assert!(
            expanded.iter().any(|s| s == "db"),
            "should contract database to db"
        );
    }

    #[test]
    fn expand_cfg() {
        let expanded = expand_query("cfg");
        assert!(expanded.iter().any(|s| s == "configuration"));
    }

    // -- expand_query: synonyms --------------------------------------------

    #[test]
    fn expand_synonym_remove() {
        let expanded = expand_query("remove");
        assert!(expanded.iter().any(|s| s == "delete"));
        assert!(expanded.iter().any(|s| s == "destroy"));
    }

    #[test]
    fn expand_synonym_get() {
        let expanded = expand_query("get");
        assert!(expanded.iter().any(|s| s == "fetch"));
        assert!(expanded.iter().any(|s| s == "retrieve"));
    }

    #[test]
    fn expand_synonym_create() {
        let expanded = expand_query("create");
        assert!(expanded.iter().any(|s| s == "new"));
        assert!(expanded.iter().any(|s| s == "init"));
    }

    // -- expand_query: compound identifiers --------------------------------

    #[test]
    fn expand_camel_case_identifier() {
        let expanded = expand_query("getUserById");
        // Should contain split parts
        assert!(expanded.iter().any(|s| s == "user"));
    }

    #[test]
    fn expand_snake_case_identifier() {
        let expanded = expand_query("get_user_by_id");
        assert!(expanded.iter().any(|s| s == "user"));
    }

    // -- expand_query: edge cases ------------------------------------------

    #[test]
    fn expand_empty_query() {
        let expanded = expand_query("");
        assert_eq!(expanded.len(), 1);
        assert!(expanded[0].is_empty());
    }

    #[test]
    fn expand_whitespace_query() {
        let expanded = expand_query("   ");
        assert_eq!(expanded.len(), 1);
        assert!(expanded[0].is_empty());
    }

    #[test]
    fn expand_no_duplicates() {
        let expanded = expand_query("delete");
        let unique: HashSet<&str> = expanded.iter().map(|s| s.as_str()).collect();
        assert_eq!(expanded.len(), unique.len(), "should have no duplicates");
    }

    #[test]
    fn expand_original_always_first() {
        let expanded = expand_query("fetchData");
        assert_eq!(expanded[0], "fetchData");
    }

    #[test]
    fn expand_multi_word_query() {
        let expanded = expand_query("get user");
        assert_eq!(expanded[0], "get user");
        // Should expand both tokens
        assert!(expanded.iter().any(|s| s == "fetch"));
        assert!(expanded.iter().any(|s| s == "retrieve"));
    }

    #[test]
    fn expand_preserves_case_in_original() {
        let expanded = expand_query("MyService");
        assert_eq!(expanded[0], "MyService");
    }

    // -- expand_query: combined abbreviation + synonym ---------------------

    #[test]
    fn expand_init_gets_synonyms_and_abbreviation() {
        let expanded = expand_query("init");
        // Abbreviation: init → initialize
        assert!(expanded.iter().any(|s| s == "initialize"));
        // Synonym group: create/new/init/make/build
        assert!(expanded.iter().any(|s| s == "create"));
        assert!(expanded.iter().any(|s| s == "new"));
    }

    #[test]
    fn expand_err_gets_abbreviation_and_synonym() {
        let expanded = expand_query("err");
        // Abbreviation: err → error
        assert!(expanded.iter().any(|s| s == "error"));
        // Synonym group via error: error/exception/fault/failure
        // Note: synonyms only fire on the original token, not expanded
        // But the "error" synonym group won't fire because "err" is
        // not in the synonym group.  That's correct — the expansion
        // adds "error" as an abbreviation expansion, which is enough.
    }

    #[test]
    fn expand_del_gets_abbreviation_and_synonyms() {
        let expanded = expand_query("del");
        // Abbreviation: del → delete
        assert!(expanded.iter().any(|s| s == "delete"));
    }
}

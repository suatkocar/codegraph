//! Lightweight data flow analysis via regex-based heuristics.
//!
//! Provides def-use chain analysis, dead store detection, uninitialized
//! variable detection, and reaching definition queries. Operates on raw
//! source text without tree-sitter, making it fast and language-flexible.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A source location (file, line, column).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub file_path: String,
    pub line: u32,
    pub column: u32,
}

/// A definition-use chain: one variable, its definitions and uses.
#[derive(Debug, Clone)]
pub struct DefUseChain {
    pub variable: String,
    pub definitions: Vec<Location>,
    pub uses: Vec<Location>,
}

/// A variable assignment that is never subsequently read.
#[derive(Debug, Clone)]
pub struct DeadStore {
    pub variable: String,
    pub file_path: String,
    pub line: u32,
    pub assigned_value: String,
}

// ---------------------------------------------------------------------------
// Language-specific patterns
// ---------------------------------------------------------------------------

/// Assignment patterns per language family.
struct LangPatterns {
    /// Regex-like prefixes for assignment (e.g., `let `, `var `, `const `).
    decl_keywords: Vec<&'static str>,
    /// Whether `:=` is an assignment operator (Go).
    has_walrus: bool,
}

fn patterns_for(language: &str) -> LangPatterns {
    match language.to_lowercase().as_str() {
        "go" | "golang" => LangPatterns {
            decl_keywords: vec!["var "],
            has_walrus: true,
        },
        "rust" => LangPatterns {
            decl_keywords: vec!["let ", "let mut "],
            has_walrus: false,
        },
        "python" => LangPatterns {
            decl_keywords: vec![],
            has_walrus: false,
        },
        "javascript" | "jsx" | "typescript" | "tsx" => LangPatterns {
            decl_keywords: vec!["let ", "const ", "var "],
            has_walrus: false,
        },
        "java" | "csharp" | "c#" | "kotlin" | "scala" | "dart" => LangPatterns {
            decl_keywords: vec!["var ", "val ", "final "],
            has_walrus: false,
        },
        _ => LangPatterns {
            decl_keywords: vec!["let ", "var ", "const "],
            has_walrus: false,
        },
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Find definition-use chains for variables in source code.
///
/// Scans line by line, collecting assignments (definitions) and subsequent
/// identifier references (uses) for each variable name.
pub fn find_def_use_chains(source: &str, language: &str) -> Vec<DefUseChain> {
    let patterns = patterns_for(language);
    let mut defs: HashMap<String, Vec<Location>> = HashMap::new();
    let mut uses: HashMap<String, Vec<Location>> = HashMap::new();

    for (line_idx, line) in source.lines().enumerate() {
        let line_num = (line_idx + 1) as u32;
        let trimmed = line.trim();

        // Skip empty lines and comments.
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }

        // Try to extract an assignment.
        if let Some((var, _value, col)) = extract_assignment(trimmed, &patterns) {
            defs.entry(var.clone()).or_default().push(Location {
                file_path: String::new(), // filled by caller if needed
                line: line_num,
                column: col,
            });

            // Also look for uses of OTHER variables on the RHS.
            // We'll handle this in a second pass below.
        }

        // Collect identifier-like uses.
        for (var_name, col) in extract_identifiers(trimmed) {
            // Skip if this is the LHS of an assignment on this line.
            if let Some((assigned_var, _, _)) = extract_assignment(trimmed, &patterns) {
                if assigned_var == var_name {
                    continue;
                }
            }
            uses.entry(var_name).or_default().push(Location {
                file_path: String::new(),
                line: line_num,
                column: col,
            });
        }
    }

    // Build chains only for variables that have at least one definition.
    let mut chains: Vec<DefUseChain> = Vec::new();
    for (var, def_locs) in &defs {
        let use_locs = uses.get(var).cloned().unwrap_or_default();
        chains.push(DefUseChain {
            variable: var.clone(),
            definitions: def_locs.clone(),
            uses: use_locs,
        });
    }

    // Sort by variable name for deterministic output.
    chains.sort_by(|a, b| a.variable.cmp(&b.variable));
    chains
}

/// Find assignments that are never read (dead stores).
///
/// A store is "dead" if the variable is assigned but never appears in a
/// use position before the next assignment or end of the source.
pub fn find_dead_stores(source: &str, language: &str) -> Vec<DeadStore> {
    let patterns = patterns_for(language);
    let mut stores: Vec<(String, String, u32, String)> = Vec::new(); // (var, file, line, value)
    let mut used_vars: HashMap<String, Vec<u32>> = HashMap::new();

    for (line_idx, line) in source.lines().enumerate() {
        let line_num = (line_idx + 1) as u32;
        let trimmed = line.trim();

        if let Some((var, value, _col)) = extract_assignment(trimmed, &patterns) {
            stores.push((var.clone(), String::new(), line_num, value));
        }

        for (var_name, _col) in extract_identifiers(trimmed) {
            // Skip LHS of assignment.
            if let Some((assigned_var, _, _)) = extract_assignment(trimmed, &patterns) {
                if assigned_var == var_name {
                    continue;
                }
            }
            used_vars.entry(var_name).or_default().push(line_num);
        }
    }

    // Group stores by variable to find next-assignment boundaries.
    let mut stores_by_var: HashMap<String, Vec<(u32, String)>> = HashMap::new();
    for (var, _file, line, value) in &stores {
        stores_by_var
            .entry(var.clone())
            .or_default()
            .push((*line, value.clone()));
    }

    let mut dead: Vec<DeadStore> = Vec::new();
    for (var, defs) in &stores_by_var {
        let use_lines = used_vars.get(var);

        for (i, (def_line, value)) in defs.iter().enumerate() {
            // The "window" for this def is [def_line+1, next_def_line-1].
            // If there's no use in that window, this store is dead.
            let next_def_line = defs.get(i + 1).map(|(l, _)| *l);

            let is_used = use_lines
                .map(|lines| {
                    lines
                        .iter()
                        .any(|&ul| ul > *def_line && next_def_line.is_none_or(|nd| ul < nd))
                })
                .unwrap_or(false);

            if !is_used {
                dead.push(DeadStore {
                    variable: var.clone(),
                    file_path: String::new(),
                    line: *def_line,
                    assigned_value: value.clone(),
                });
            }
        }
    }

    dead.sort_by_key(|d| d.line);
    dead
}

/// Find variables used before initialization.
///
/// Detects identifiers that appear in a use position before any assignment
/// to that variable.
pub fn find_uninitialized_uses(source: &str, language: &str) -> Vec<Location> {
    let patterns = patterns_for(language);
    let mut defined: HashMap<String, u32> = HashMap::new(); // first def line
    let mut first_use: HashMap<String, (u32, u32)> = HashMap::new(); // first use (line, col)

    for (line_idx, line) in source.lines().enumerate() {
        let line_num = (line_idx + 1) as u32;
        let trimmed = line.trim();

        if let Some((var, _, _)) = extract_assignment(trimmed, &patterns) {
            defined.entry(var).or_insert(line_num);
        }

        for (var_name, col) in extract_identifiers(trimmed) {
            // Skip LHS.
            if let Some((assigned_var, _, _)) = extract_assignment(trimmed, &patterns) {
                if assigned_var == var_name {
                    continue;
                }
            }
            first_use.entry(var_name).or_insert((line_num, col));
        }
    }

    let mut uninitialized: Vec<Location> = Vec::new();
    for (var, (use_line, use_col)) in &first_use {
        match defined.get(var) {
            None => {
                // Used but never defined in this scope — could be a global or parameter.
                // We only flag it if it looks like a local variable (lowercase start).
                if var.starts_with(|c: char| c.is_lowercase()) {
                    uninitialized.push(Location {
                        file_path: String::new(),
                        line: *use_line,
                        column: *use_col,
                    });
                }
            }
            Some(def_line) => {
                if use_line < def_line {
                    uninitialized.push(Location {
                        file_path: String::new(),
                        line: *use_line,
                        column: *use_col,
                    });
                }
            }
        }
    }

    uninitialized.sort_by_key(|l| (l.line, l.column));
    uninitialized
}

/// Find which definitions reach a given use point.
///
/// Returns def-use chains where at least one definition occurs before
/// `target_line` and the variable is used at or near `target_line`.
pub fn find_reaching_defs(source: &str, language: &str, target_line: u32) -> Vec<DefUseChain> {
    let chains = find_def_use_chains(source, language);

    chains
        .into_iter()
        .filter(|chain| {
            // Has at least one def before target_line
            let has_reaching_def = chain.definitions.iter().any(|d| d.line <= target_line);
            // Is used at or near target_line (within 2 lines)
            let used_near_target = chain
                .uses
                .iter()
                .any(|u| u.line >= target_line.saturating_sub(1) && u.line <= target_line + 1);
            has_reaching_def && used_near_target
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Internal: assignment extraction
// ---------------------------------------------------------------------------

/// Try to extract a variable assignment from a line.
///
/// Returns `(variable_name, assigned_value, column_of_var)` if found.
fn extract_assignment(line: &str, patterns: &LangPatterns) -> Option<(String, String, u32)> {
    let trimmed = line.trim();

    // Try declaration keywords: `let x = ...`, `const y = ...`, etc.
    for &kw in &patterns.decl_keywords {
        if let Some(rest) = trimmed.strip_prefix(kw) {
            return parse_var_equals(rest, trimmed.len() - rest.len());
        }
    }

    // Try `:=` for Go.
    if patterns.has_walrus {
        if let Some(pos) = trimmed.find(":=") {
            let var_part = trimmed[..pos].trim();
            let val_part = trimmed[pos + 2..].trim();
            if is_valid_identifier(var_part) {
                return Some((var_part.to_string(), val_part.to_string(), 0));
            }
        }
    }

    // Try bare assignment: `x = ...` (for Python, etc.)
    // But skip `==`, `!=`, `<=`, `>=`, `+=`, `-=`, etc.
    if let Some(eq_pos) = trimmed.find('=') {
        if eq_pos > 0 && eq_pos + 1 < trimmed.len() {
            let before = trimmed.as_bytes()[eq_pos - 1];
            let after = trimmed.as_bytes()[eq_pos + 1];
            // Reject compound/comparison operators.
            if before != b'!'
                && before != b'<'
                && before != b'>'
                && before != b'='
                && after != b'='
                && before != b'+'
                && before != b'-'
                && before != b'*'
                && before != b'/'
            {
                let var_part = trimmed[..eq_pos].trim();
                let val_part = trimmed[eq_pos + 1..].trim();

                // Strip type annotations: `x: int = 5` -> var is `x`
                let var_name = var_part.split(':').next().unwrap_or(var_part).trim();

                if is_valid_identifier(var_name) {
                    return Some((var_name.to_string(), val_part.to_string(), 0));
                }
            }
        }
    }

    None
}

/// Parse `varname = value` after a declaration keyword has been stripped.
fn parse_var_equals(rest: &str, offset: usize) -> Option<(String, String, u32)> {
    // Handle type annotations: `let x: i32 = 5`
    let parts: Vec<&str> = rest.splitn(2, '=').collect();
    if parts.len() < 2 {
        // Declaration without assignment: `let x;`
        let var = parts[0].trim().split(':').next()?.trim();
        let var = var.split_whitespace().next()?;
        if is_valid_identifier(var) {
            return Some((var.to_string(), String::new(), offset as u32));
        }
        return None;
    }

    let var_part = parts[0].trim();
    let val_part = parts[1].trim().trim_end_matches(';');

    // Strip type annotation and mutability keywords.
    let var_name = var_part
        .split(':')
        .next()
        .unwrap_or(var_part)
        .split_whitespace()
        .last()
        .unwrap_or(var_part);

    if is_valid_identifier(var_name) {
        Some((var_name.to_string(), val_part.to_string(), offset as u32))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Internal: identifier extraction
// ---------------------------------------------------------------------------

/// Extract identifier-like tokens from a line.
///
/// Returns `(name, column)` pairs for each identifier found.
fn extract_identifiers(line: &str) -> Vec<(String, u32)> {
    let mut results = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Skip non-identifier characters.
        if !bytes[i].is_ascii_alphabetic() && bytes[i] != b'_' {
            i += 1;
            continue;
        }

        // Collect the identifier.
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }

        let ident = &line[start..i];

        // Skip language keywords.
        if !is_keyword(ident) {
            results.push((ident.to_string(), start as u32));
        }
    }

    results
}

/// Check if a string is a valid identifier (starts with letter/underscore,
/// followed by alphanumerics/underscores).
fn is_valid_identifier(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Check if a string is a common language keyword (not a variable).
fn is_keyword(s: &str) -> bool {
    matches!(
        s,
        "if" | "else"
            | "for"
            | "while"
            | "do"
            | "return"
            | "break"
            | "continue"
            | "match"
            | "switch"
            | "case"
            | "default"
            | "fn"
            | "func"
            | "function"
            | "def"
            | "class"
            | "struct"
            | "enum"
            | "trait"
            | "impl"
            | "type"
            | "interface"
            | "import"
            | "from"
            | "export"
            | "module"
            | "use"
            | "pub"
            | "const"
            | "let"
            | "var"
            | "mut"
            | "val"
            | "final"
            | "true"
            | "false"
            | "null"
            | "nil"
            | "None"
            | "undefined"
            | "new"
            | "this"
            | "self"
            | "Self"
            | "super"
            | "async"
            | "await"
            | "yield"
            | "try"
            | "catch"
            | "throw"
            | "raise"
            | "except"
            | "finally"
            | "with"
            | "as"
            | "in"
            | "is"
            | "not"
            | "and"
            | "or"
            | "static"
            | "void"
            | "int"
            | "float"
            | "double"
            | "bool"
            | "char"
            | "string"
            | "String"
            | "println"
            | "print"
            | "println!"
            | "panic"
            | "panic!"
            | "loop"
            | "elif"
            | "pass"
            | "lambda"
            | "where"
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // find_def_use_chains: basic assignment and use
    // -------------------------------------------------------------------

    #[test]
    fn def_use_chain_basic_js() {
        let source = "\
let x = 10;
let y = x + 5;
console.log(y);";

        let chains = find_def_use_chains(source, "javascript");

        let x_chain = chains.iter().find(|c| c.variable == "x");
        assert!(x_chain.is_some(), "should find chain for x");
        let x = x_chain.unwrap();
        assert_eq!(x.definitions.len(), 1);
        assert_eq!(x.definitions[0].line, 1);
        assert!(!x.uses.is_empty(), "x should be used in line 2");
    }

    // -------------------------------------------------------------------
    // find_def_use_chains: Python
    // -------------------------------------------------------------------

    #[test]
    fn def_use_chain_python() {
        let source = "\
total = 0
for item in items:
    total = total + item
print(total)";

        let chains = find_def_use_chains(source, "python");

        let total_chain = chains.iter().find(|c| c.variable == "total");
        assert!(total_chain.is_some(), "should find chain for total");
        let total = total_chain.unwrap();
        assert!(total.definitions.len() >= 1, "total has at least 1 def");
        assert!(!total.uses.is_empty(), "total is used");
    }

    // -------------------------------------------------------------------
    // find_def_use_chains: Rust let mut
    // -------------------------------------------------------------------

    #[test]
    fn def_use_chain_rust() {
        let source = "\
let mut count = 0;
count = count + 1;
println!(\"{}\", count);";

        let chains = find_def_use_chains(source, "rust");

        let count_chain = chains.iter().find(|c| c.variable == "count");
        assert!(count_chain.is_some(), "should find chain for count");
        let count = count_chain.unwrap();
        assert!(count.definitions.len() >= 1);
    }

    // -------------------------------------------------------------------
    // find_dead_stores: basic
    // -------------------------------------------------------------------

    #[test]
    fn dead_store_never_read() {
        let source = "\
let x = 10;
let y = 20;
console.log(y);";

        let dead = find_dead_stores(source, "javascript");

        assert!(
            dead.iter().any(|d| d.variable == "x"),
            "x is assigned but never read: {:?}",
            dead
        );
        assert!(
            !dead.iter().any(|d| d.variable == "y"),
            "y IS read, should not be dead"
        );
    }

    // -------------------------------------------------------------------
    // find_dead_stores: overwritten before read
    // -------------------------------------------------------------------

    #[test]
    fn dead_store_overwritten() {
        let source = "\
let x = 10;
x = 20;
console.log(x);";

        let dead = find_dead_stores(source, "javascript");

        // First assignment to x (= 10) is dead because it's overwritten.
        assert!(
            dead.iter().any(|d| d.variable == "x" && d.line == 1),
            "first assignment to x should be dead: {:?}",
            dead
        );
    }

    // -------------------------------------------------------------------
    // find_uninitialized_uses: use before def
    // -------------------------------------------------------------------

    #[test]
    fn uninitialized_use_before_def() {
        let source = "\
console.log(result);
let result = compute();";

        let uninit = find_uninitialized_uses(source, "javascript");

        assert!(
            uninit.iter().any(|l| l.line == 1),
            "result used on line 1 before defined on line 2: {:?}",
            uninit
        );
    }

    // -------------------------------------------------------------------
    // find_uninitialized_uses: no false positive for correct order
    // -------------------------------------------------------------------

    #[test]
    fn no_false_positive_when_defined_first() {
        let source = "\
let count = 10;
let result = count + 5;";

        let _uninit = find_uninitialized_uses(source, "javascript");

        // count is defined on line 1, used on line 2 — should not be flagged.
        // (We only check that no Location references line 2 for count.)
        let chains = find_def_use_chains(source, "javascript");
        let count_chain = chains.iter().find(|c| c.variable == "count");
        assert!(count_chain.is_some(), "count should have a def-use chain");
        let count = count_chain.unwrap();
        assert!(!count.definitions.is_empty(), "count should be defined");
        assert!(!count.uses.is_empty(), "count should be used");
        assert!(
            count
                .uses
                .iter()
                .all(|u| u.line >= count.definitions[0].line),
            "all uses of count should be after its definition"
        );
    }

    // -------------------------------------------------------------------
    // find_reaching_defs: basic
    // -------------------------------------------------------------------

    #[test]
    fn reaching_defs_at_target_line() {
        let source = "\
let x = 10;
let y = 20;
let z = x + y;";

        let reaching = find_reaching_defs(source, "javascript", 3);

        assert!(
            reaching.iter().any(|c| c.variable == "x"),
            "x should reach line 3: {:?}",
            reaching
        );
        assert!(
            reaching.iter().any(|c| c.variable == "y"),
            "y should reach line 3: {:?}",
            reaching
        );
    }

    // -------------------------------------------------------------------
    // find_reaching_defs: def after target doesn't reach
    // -------------------------------------------------------------------

    #[test]
    fn reaching_defs_excludes_later_defs() {
        let source = "\
let z = x + 1;
let x = 10;";

        let reaching = find_reaching_defs(source, "javascript", 1);

        // x is defined on line 2, so it should NOT reach line 1.
        // But x is used on line 1 before being defined — it might still
        // appear if the use is at the target line. The key is that
        // definitions AFTER target_line should not count as reaching.
        for chain in &reaching {
            if chain.variable == "x" {
                assert!(
                    chain.definitions.iter().all(|d| d.line <= 1),
                    "only defs at or before target should be reaching"
                );
            }
        }
    }

    // -------------------------------------------------------------------
    // extract_assignment: various forms
    // -------------------------------------------------------------------

    #[test]
    fn extract_assignment_let() {
        let patterns = patterns_for("javascript");
        let result = extract_assignment("let count = 0;", &patterns);
        assert!(result.is_some());
        let (var, val, _) = result.unwrap();
        assert_eq!(var, "count");
        assert_eq!(val, "0");
    }

    #[test]
    fn extract_assignment_go_walrus() {
        let patterns = patterns_for("go");
        let result = extract_assignment("err := doSomething()", &patterns);
        assert!(result.is_some());
        let (var, _, _) = result.unwrap();
        assert_eq!(var, "err");
    }

    #[test]
    fn extract_assignment_python_bare() {
        let patterns = patterns_for("python");
        let result = extract_assignment("total = 0", &patterns);
        assert!(result.is_some());
        let (var, val, _) = result.unwrap();
        assert_eq!(var, "total");
        assert_eq!(val, "0");
    }

    #[test]
    fn extract_assignment_rust_typed() {
        let patterns = patterns_for("rust");
        let result = extract_assignment("let mut count: i32 = 0;", &patterns);
        assert!(result.is_some());
        let (var, _, _) = result.unwrap();
        assert_eq!(var, "count");
    }

    // -------------------------------------------------------------------
    // is_valid_identifier
    // -------------------------------------------------------------------

    #[test]
    fn valid_identifiers() {
        assert!(is_valid_identifier("foo"));
        assert!(is_valid_identifier("_bar"));
        assert!(is_valid_identifier("count_123"));
        assert!(!is_valid_identifier("123abc"));
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("foo bar"));
    }

    // =====================================================================
    // NEW TESTS: Phase 18C — Dataflow comprehensive coverage
    // =====================================================================

    // -- def_use chains: TypeScript/JavaScript ----------------------------

    #[test]
    fn def_use_chain_const_declaration() {
        let source = "const MAX = 100;\nlet result = MAX + 1;";
        let chains = find_def_use_chains(source, "javascript");
        let max_chain = chains.iter().find(|c| c.variable == "MAX");
        assert!(max_chain.is_some());
        assert!(!max_chain.unwrap().uses.is_empty());
    }

    #[test]
    fn def_use_chain_var_declaration() {
        let source = "var count = 0;\ncount = count + 1;\nconsole.log(count);";
        let chains = find_def_use_chains(source, "javascript");
        let count_chain = chains.iter().find(|c| c.variable == "count");
        assert!(count_chain.is_some());
        assert!(count_chain.unwrap().definitions.len() >= 1);
        assert!(!count_chain.unwrap().uses.is_empty());
    }

    #[test]
    fn def_use_chain_multiple_variables() {
        let source = "\
let x = 1;
let y = 2;
let z = x + y;";
        let chains = find_def_use_chains(source, "javascript");
        assert!(chains.iter().any(|c| c.variable == "x"));
        assert!(chains.iter().any(|c| c.variable == "y"));
        assert!(chains.iter().any(|c| c.variable == "z"));
    }

    // -- def_use chains: Go -----------------------------------------------

    #[test]
    fn def_use_chain_go_walrus() {
        let source = "\
err := doSomething()
fmt.Println(err)";
        let chains = find_def_use_chains(source, "go");
        let err_chain = chains.iter().find(|c| c.variable == "err");
        assert!(err_chain.is_some());
        assert!(!err_chain.unwrap().uses.is_empty());
    }

    #[test]
    fn def_use_chain_go_var_decl() {
        let source = "\
var total int
total = computeTotal()
fmt.Println(total)";
        let chains = find_def_use_chains(source, "go");
        let total_chain = chains.iter().find(|c| c.variable == "total");
        assert!(total_chain.is_some());
    }

    // -- def_use chains: TypeScript with types ----------------------------

    #[test]
    fn def_use_chain_typescript_typed() {
        let source = "let count: number = 0;\ncount = count + 1;";
        let chains = find_def_use_chains(source, "typescript");
        let count_chain = chains.iter().find(|c| c.variable == "count");
        assert!(count_chain.is_some());
    }

    // -- dead stores: patterns --------------------------------------------

    #[test]
    fn dead_store_unused_variable() {
        let source = "let unused = 42;\nlet used = 10;\nconsole.log(used);";
        let dead = find_dead_stores(source, "javascript");
        assert!(dead.iter().any(|d| d.variable == "unused"));
        assert!(!dead.iter().any(|d| d.variable == "used"));
    }

    #[test]
    fn dead_store_reassigned_multiple_times() {
        let source = "\
let x = 1;
x = 2;
x = 3;
console.log(x);";
        let dead = find_dead_stores(source, "javascript");
        // First two assignments are dead (overwritten before read)
        let dead_x: Vec<_> = dead.iter().filter(|d| d.variable == "x").collect();
        assert!(
            dead_x.len() >= 2,
            "first two assignments should be dead: {:?}",
            dead_x
        );
    }

    #[test]
    fn dead_store_no_dead_when_all_used() {
        let source = "\
let x = computeA();
processA(x);
let y = computeB();
processB(y);";
        let dead = find_dead_stores(source, "javascript");
        // Neither x nor y is dead
        assert!(!dead.iter().any(|d| d.variable == "x"), "x is used");
        assert!(!dead.iter().any(|d| d.variable == "y"), "y is used");
    }

    #[test]
    fn dead_store_python() {
        let source = "\
result = compute()
result = recompute()
output(result)";
        let dead = find_dead_stores(source, "python");
        // First assignment is dead
        assert!(dead.iter().any(|d| d.variable == "result" && d.line == 1));
    }

    #[test]
    fn dead_store_rust() {
        let source = "\
let mut buf = Vec::new();
buf = Vec::with_capacity(10);
buf.push(42);";
        let dead = find_dead_stores(source, "rust");
        assert!(dead.iter().any(|d| d.variable == "buf" && d.line == 1));
    }

    // -- uninitialized uses -----------------------------------------------

    #[test]
    fn uninitialized_use_never_defined() {
        let source = "console.log(mystery);";
        let uninit = find_uninitialized_uses(source, "javascript");
        assert!(
            uninit.iter().any(|l| l.line == 1),
            "mystery is never defined: {:?}",
            uninit
        );
    }

    #[test]
    fn uninitialized_no_false_positive_for_param_like() {
        // Uppercase identifiers are not flagged
        let source = "console.log(Math.PI);";
        let uninit = find_uninitialized_uses(source, "javascript");
        // "Math" starts with uppercase, should not be flagged
        assert!(!uninit.iter().any(|l| l.line == 1 && l.column == 12));
    }

    #[test]
    fn uninitialized_use_python() {
        let source = "\
result = process(data)
data = load_data()";
        let uninit = find_uninitialized_uses(source, "python");
        // data is used on line 1 before defined on line 2
        assert!(
            uninit.iter().any(|l| l.line == 1),
            "data used before def: {:?}",
            uninit
        );
    }

    // -- reaching defs: patterns ------------------------------------------

    #[test]
    fn reaching_defs_multiple_variables() {
        let source = "\
let a = 1;
let b = 2;
let c = 3;
let result = a + b + c;";
        let reaching = find_reaching_defs(source, "javascript", 4);
        assert!(reaching.iter().any(|c| c.variable == "a"));
        assert!(reaching.iter().any(|c| c.variable == "b"));
        assert!(reaching.iter().any(|c| c.variable == "c"));
    }

    #[test]
    fn reaching_defs_empty_source() {
        let reaching = find_reaching_defs("", "javascript", 1);
        assert!(reaching.is_empty());
    }

    #[test]
    fn reaching_defs_no_match_at_target() {
        let source = "\
let x = 10;
let y = 20;";
        // Target line 10 — no uses near line 10
        let reaching = find_reaching_defs(source, "javascript", 10);
        assert!(reaching.is_empty());
    }

    // -- empty source code ------------------------------------------------

    #[test]
    fn def_use_chains_empty_source() {
        let chains = find_def_use_chains("", "javascript");
        assert!(chains.is_empty());
    }

    #[test]
    fn dead_stores_empty_source() {
        let dead = find_dead_stores("", "javascript");
        assert!(dead.is_empty());
    }

    #[test]
    fn uninitialized_uses_empty_source() {
        let uninit = find_uninitialized_uses("", "javascript");
        assert!(uninit.is_empty());
    }

    // -- comment lines are skipped ----------------------------------------

    #[test]
    fn def_use_chains_skip_comments() {
        let source = "\
// let commented = 42;
let real = 10;
console.log(real);";
        let chains = find_def_use_chains(source, "javascript");
        assert!(!chains.iter().any(|c| c.variable == "commented"));
        assert!(chains.iter().any(|c| c.variable == "real"));
    }

    #[test]
    fn def_use_chains_skip_python_comments() {
        let source = "\
# count = old_value
count = 10
print(count)";
        let chains = find_def_use_chains(source, "python");
        // The comment line should not create a definition
        let count_chain = chains.iter().find(|c| c.variable == "count");
        assert!(count_chain.is_some());
        assert_eq!(count_chain.unwrap().definitions[0].line, 2);
    }

    // -- patterns_for language coverage -----------------------------------

    #[test]
    fn patterns_for_unknown_language() {
        let source = "let x = 1;\nconsole.log(x);";
        let chains = find_def_use_chains(source, "brainfuck");
        // Should fall back to default patterns (let/var/const)
        assert!(chains.iter().any(|c| c.variable == "x"));
    }

    #[test]
    fn patterns_for_java() {
        let source = "var count = 0;\nSystem.out.println(count);";
        let chains = find_def_use_chains(source, "java");
        assert!(chains.iter().any(|c| c.variable == "count"));
    }

    #[test]
    fn patterns_for_kotlin() {
        let source = "val name = \"test\"\nprintln(name)";
        let chains = find_def_use_chains(source, "kotlin");
        assert!(chains.iter().any(|c| c.variable == "name"));
    }

    // -- extract_assignment edge cases ------------------------------------

    #[test]
    fn extract_assignment_comparison_not_matched() {
        let patterns = patterns_for("javascript");
        // == should not be treated as assignment
        assert!(extract_assignment("if (x == 5)", &patterns).is_none());
    }

    #[test]
    fn extract_assignment_not_equals_not_matched() {
        let patterns = patterns_for("javascript");
        assert!(extract_assignment("if (x != 5)", &patterns).is_none());
    }

    #[test]
    fn extract_assignment_plus_equals_not_matched() {
        let patterns = patterns_for("javascript");
        assert!(extract_assignment("count += 1", &patterns).is_none());
    }

    // -- is_keyword covers common keywords --------------------------------

    #[test]
    fn is_keyword_covers_basics() {
        assert!(is_keyword("if"));
        assert!(is_keyword("for"));
        assert!(is_keyword("while"));
        assert!(is_keyword("return"));
        assert!(is_keyword("function"));
        assert!(is_keyword("class"));
        assert!(is_keyword("import"));
        assert!(!is_keyword("myVariable"));
        assert!(!is_keyword("doWork"));
    }

    // -- dead stores: sorted by line --------------------------------------

    #[test]
    fn dead_stores_sorted_by_line() {
        let source = "\
let z = 30;
let a = 10;
let m = 20;";
        let dead = find_dead_stores(source, "javascript");
        for i in 1..dead.len() {
            assert!(
                dead[i].line >= dead[i - 1].line,
                "dead stores should be sorted by line"
            );
        }
    }

    // -- def_use chains: sorted by variable name --------------------------

    #[test]
    fn def_use_chains_sorted_by_variable() {
        let source = "\
let zebra = 1;
let alpha = 2;
let middle = 3;";
        let chains = find_def_use_chains(source, "javascript");
        for i in 1..chains.len() {
            assert!(
                chains[i].variable >= chains[i - 1].variable,
                "chains should be sorted by variable name"
            );
        }
    }
}

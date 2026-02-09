//! Tool-to-category registry for preset-based filtering.
//!
//! Maps each of the 46 MCP tools to its category so that `filter_tools()`
//! can decide which tools are visible for a given config preset.

use crate::config::preset::*;
use crate::config::schema::ToolMetadata;

/// Return metadata for all 46 MCP tools, mapping each to its category.
///
/// The order here mirrors the tool numbering in CLAUDE.md.
/// Categories come from [`crate::config::preset`] constants.
pub fn all_tool_metadata() -> Vec<ToolMetadata> {
    vec![
        // ── Core (14) ─────────────────────────────────────────────
        meta(
            "codegraph_query",
            CATEGORY_SEARCH,
            "Hybrid semantic + keyword search with query expansion",
            200,
        ),
        meta(
            "codegraph_search",
            CATEGORY_SEARCH,
            "Fast FTS5-only keyword search",
            150,
        ),
        meta(
            "codegraph_dependencies",
            CATEGORY_SEARCH,
            "Forward dependency traversal",
            180,
        ),
        meta(
            "codegraph_callers",
            CATEGORY_CALL_GRAPH,
            "Reverse call graph traversal",
            180,
        ),
        meta(
            "codegraph_callees",
            CATEGORY_CALL_GRAPH,
            "Forward call graph",
            180,
        ),
        meta(
            "codegraph_impact",
            CATEGORY_ANALYSIS,
            "Blast radius analysis",
            200,
        ),
        meta(
            "codegraph_structure",
            CATEGORY_REPOSITORY,
            "Project overview with PageRank",
            250,
        ),
        meta(
            "codegraph_tests",
            CATEGORY_SEARCH,
            "Test coverage discovery",
            150,
        ),
        meta(
            "codegraph_context",
            CATEGORY_CONTEXT,
            "LLM context assembly with adaptive budget",
            300,
        ),
        meta(
            "codegraph_diagram",
            CATEGORY_ANALYSIS,
            "Mermaid diagram generation",
            200,
        ),
        meta(
            "codegraph_node",
            CATEGORY_SEARCH,
            "Direct symbol lookup with relationships",
            180,
        ),
        meta(
            "codegraph_dead_code",
            CATEGORY_ANALYSIS,
            "Find unused symbols",
            180,
        ),
        meta(
            "codegraph_frameworks",
            CATEGORY_REPOSITORY,
            "Detect project frameworks",
            120,
        ),
        meta(
            "codegraph_languages",
            CATEGORY_REPOSITORY,
            "Language breakdown statistics",
            100,
        ),
        // ── Git Integration (9) ──────────────────────────────────
        meta("codegraph_blame", CATEGORY_GIT, "Line-by-line blame", 200),
        meta(
            "codegraph_file_history",
            CATEGORY_GIT,
            "File commit history",
            180,
        ),
        meta(
            "codegraph_recent_changes",
            CATEGORY_GIT,
            "Recent repository commits",
            180,
        ),
        meta(
            "codegraph_commit_diff",
            CATEGORY_GIT,
            "Commit diff details",
            200,
        ),
        meta(
            "codegraph_symbol_history",
            CATEGORY_GIT,
            "Symbol modification history",
            180,
        ),
        meta(
            "codegraph_branch_info",
            CATEGORY_GIT,
            "Branch status and tracking",
            120,
        ),
        meta(
            "codegraph_modified_files",
            CATEGORY_GIT,
            "Working tree changes",
            120,
        ),
        meta(
            "codegraph_hotspots",
            CATEGORY_GIT,
            "Churn-based hotspot detection",
            200,
        ),
        meta(
            "codegraph_contributors",
            CATEGORY_GIT,
            "Contributor statistics",
            150,
        ),
        // ── Security (9) ─────────────────────────────────────────
        meta(
            "codegraph_scan_security",
            CATEGORY_SECURITY,
            "YAML rule-based vulnerability scan",
            300,
        ),
        meta(
            "codegraph_check_owasp",
            CATEGORY_SECURITY,
            "OWASP Top 10 2021 scan",
            250,
        ),
        meta(
            "codegraph_check_cwe",
            CATEGORY_SECURITY,
            "CWE Top 25 scan",
            250,
        ),
        meta(
            "codegraph_explain_vulnerability",
            CATEGORY_SECURITY,
            "CWE explanation + remediation",
            200,
        ),
        meta(
            "codegraph_suggest_fix",
            CATEGORY_SECURITY,
            "Fix suggestion for findings",
            200,
        ),
        meta(
            "codegraph_find_injections",
            CATEGORY_SECURITY,
            "SQL/XSS/command injection via taint analysis",
            250,
        ),
        meta(
            "codegraph_taint_sources",
            CATEGORY_SECURITY,
            "Identify taint sources",
            200,
        ),
        meta(
            "codegraph_security_summary",
            CATEGORY_SECURITY,
            "Comprehensive risk assessment",
            300,
        ),
        meta(
            "codegraph_trace_taint",
            CATEGORY_SECURITY,
            "Data flow tracing from source",
            200,
        ),
        // ── Repository & Analysis (7) ────────────────────────────
        meta(
            "codegraph_stats",
            CATEGORY_REPOSITORY,
            "Index statistics",
            100,
        ),
        meta(
            "codegraph_circular_imports",
            CATEGORY_ANALYSIS,
            "Cycle detection (Tarjan SCC)",
            180,
        ),
        meta(
            "codegraph_project_tree",
            CATEGORY_REPOSITORY,
            "Directory tree with symbol counts",
            200,
        ),
        meta(
            "codegraph_find_references",
            CATEGORY_SEARCH,
            "Cross-reference search",
            180,
        ),
        meta(
            "codegraph_export_map",
            CATEGORY_REPOSITORY,
            "Module export listing",
            150,
        ),
        meta(
            "codegraph_import_graph",
            CATEGORY_ANALYSIS,
            "Import graph visualization",
            200,
        ),
        meta(
            "codegraph_file",
            CATEGORY_REPOSITORY,
            "File symbol listing",
            150,
        ),
        // ── Call Graph & Data Flow (6) ───────────────────────────
        meta(
            "codegraph_find_path",
            CATEGORY_CALL_GRAPH,
            "Shortest call path (BFS)",
            200,
        ),
        meta(
            "codegraph_complexity",
            CATEGORY_ANALYSIS,
            "Cyclomatic + cognitive complexity",
            180,
        ),
        meta(
            "codegraph_data_flow",
            CATEGORY_CALL_GRAPH,
            "Variable def-use chains",
            200,
        ),
        meta(
            "codegraph_dead_stores",
            CATEGORY_CALL_GRAPH,
            "Assignments never read",
            180,
        ),
        meta(
            "codegraph_find_uninitialized",
            CATEGORY_CALL_GRAPH,
            "Variables used before init",
            180,
        ),
        meta(
            "codegraph_reaching_defs",
            CATEGORY_CALL_GRAPH,
            "Reaching definition analysis",
            180,
        ),
        // ── Deep Search (1) ─────────────────────────────────────
        meta(
            "codegraph_deep_query",
            CATEGORY_SEARCH,
            "Cross-encoder re-ranked deep search",
            250,
        ),
    ]
}

/// Convenience constructor for [`ToolMetadata`].
fn meta(name: &str, category: &str, description: &str, estimated_tokens: usize) -> ToolMetadata {
    ToolMetadata {
        name: name.to_string(),
        category: category.to_string(),
        description: description.to_string(),
        estimated_tokens,
    }
}

/// Return the set of tool names enabled for a given config.
///
/// This is the core filtering function that bridges the registry
/// with `config::loader::filter_tools()`.
pub fn enabled_tool_names(
    config: &crate::config::schema::CodeGraphConfig,
) -> std::collections::HashSet<String> {
    crate::config::loader::filter_tools(config, &all_tool_metadata())
        .into_iter()
        .map(|t| t.name)
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{CodeGraphConfig, PresetName};
    use std::collections::HashSet;

    #[test]
    fn registry_has_46_tools() {
        let tools = all_tool_metadata();
        assert_eq!(
            tools.len(),
            46,
            "expected 46 tools in registry, got {}",
            tools.len()
        );
    }

    #[test]
    fn all_tool_names_unique() {
        let tools = all_tool_metadata();
        let names: HashSet<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names.len(), tools.len(), "duplicate tool names in registry");
    }

    #[test]
    fn all_categories_valid() {
        let tools = all_tool_metadata();
        let valid: HashSet<&str> = ALL_CATEGORIES.iter().copied().collect();
        for t in &tools {
            assert!(
                valid.contains(t.category.as_str()),
                "tool {} has invalid category: {}",
                t.name,
                t.category
            );
        }
    }

    #[test]
    fn full_preset_enables_all_46() {
        let config = CodeGraphConfig::default(); // Full preset
        let enabled = enabled_tool_names(&config);
        assert_eq!(
            enabled.len(),
            46,
            "full preset should enable all 46 tools, got {}",
            enabled.len()
        );
    }

    #[test]
    fn minimal_preset_filters_correctly() {
        let mut config = CodeGraphConfig::default();
        config.preset = PresetName::Minimal;
        let enabled = enabled_tool_names(&config);
        // Minimal = Repository + Search only
        for name in &enabled {
            let meta = all_tool_metadata();
            let tool = meta.iter().find(|t| &t.name == name).unwrap();
            assert!(
                tool.category == CATEGORY_REPOSITORY || tool.category == CATEGORY_SEARCH,
                "minimal preset should only have Repository/Search tools, got {} in {}",
                name,
                tool.category
            );
        }
        assert!(
            enabled.len() < 46,
            "minimal should have fewer than 46 tools"
        );
        assert!(enabled.len() >= 10, "minimal should have at least 10 tools");
    }

    #[test]
    fn balanced_preset_includes_callgraph_and_context() {
        let mut config = CodeGraphConfig::default();
        config.preset = PresetName::Balanced;
        let enabled = enabled_tool_names(&config);
        assert!(
            enabled.contains("codegraph_callers"),
            "balanced should include callers"
        );
        assert!(
            enabled.contains("codegraph_context"),
            "balanced should include context"
        );
        assert!(
            !enabled.contains("codegraph_blame"),
            "balanced should exclude git tools"
        );
        assert!(
            !enabled.contains("codegraph_scan_security"),
            "balanced should exclude security tools"
        );
    }

    #[test]
    fn security_preset_includes_security_tools() {
        let mut config = CodeGraphConfig::default();
        config.preset = PresetName::SecurityFocused;
        let enabled = enabled_tool_names(&config);
        assert!(
            enabled.contains("codegraph_scan_security"),
            "security preset should include scan_security"
        );
        assert!(
            enabled.contains("codegraph_check_owasp"),
            "security preset should include check_owasp"
        );
        assert!(
            !enabled.contains("codegraph_blame"),
            "security preset should exclude git tools"
        );
        assert!(
            !enabled.contains("codegraph_context"),
            "security preset should exclude context tools"
        );
    }

    #[test]
    fn category_distribution_reasonable() {
        let tools = all_tool_metadata();
        let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for t in &tools {
            *counts.entry(&t.category).or_default() += 1;
        }
        // Verify category counts match CLAUDE.md documentation
        assert!(
            counts[CATEGORY_REPOSITORY] >= 6,
            "Repository should have >= 6 tools"
        );
        assert!(
            counts[CATEGORY_SEARCH] >= 5,
            "Search should have >= 5 tools"
        );
        assert!(counts[CATEGORY_GIT] == 9, "Git should have exactly 9 tools");
        assert!(
            counts[CATEGORY_SECURITY] == 9,
            "Security should have exactly 9 tools"
        );
    }
}

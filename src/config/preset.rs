//! Preset definitions — minimal, balanced, full, security-focused.
//!
//! Each preset defines which tool categories are active and provides metadata
//! for display (tool count, estimated token budget). Presets map cleanly to
//! the 7 tool categories CodeGraph exposes.

use super::schema::PresetName;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Tool categories
// ---------------------------------------------------------------------------

/// All tool categories recognised by CodeGraph.
pub const CATEGORY_REPOSITORY: &str = "Repository";
pub const CATEGORY_SEARCH: &str = "Search";
pub const CATEGORY_CALL_GRAPH: &str = "CallGraph";
pub const CATEGORY_ANALYSIS: &str = "Analysis";
pub const CATEGORY_SECURITY: &str = "Security";
pub const CATEGORY_GIT: &str = "Git";
pub const CATEGORY_CONTEXT: &str = "Context";

/// All known category names, for iteration.
pub const ALL_CATEGORIES: &[&str] = &[
    CATEGORY_REPOSITORY,
    CATEGORY_SEARCH,
    CATEGORY_CALL_GRAPH,
    CATEGORY_ANALYSIS,
    CATEGORY_SECURITY,
    CATEGORY_GIT,
    CATEGORY_CONTEXT,
];

// ---------------------------------------------------------------------------
// PresetDefinition
// ---------------------------------------------------------------------------

/// Describes a single preset's characteristics.
#[derive(Debug, Clone)]
pub struct PresetDefinition {
    /// Which preset this describes.
    pub name: PresetName,
    /// Human-readable description.
    pub description: &'static str,
    /// Which categories are enabled in this preset.
    pub enabled_categories: Vec<&'static str>,
    /// Approximate number of tools enabled.
    pub tool_count: usize,
    /// Approximate token cost of the tool schemas in the system prompt.
    pub estimated_tokens: usize,
}

// ---------------------------------------------------------------------------
// Preset constructors
// ---------------------------------------------------------------------------

/// Get the preset definition for a given name.
pub fn get_preset(name: &PresetName) -> PresetDefinition {
    match name {
        PresetName::Minimal => minimal_preset(),
        PresetName::Balanced => balanced_preset(),
        PresetName::Full => full_preset(),
        PresetName::SecurityFocused => security_preset(),
    }
}

/// Minimal preset — fast, lightweight. Best for Zed, Vim, quick edits.
///
/// Only Repository + Search. Keeps context window lean.
pub fn minimal_preset() -> PresetDefinition {
    PresetDefinition {
        name: PresetName::Minimal,
        description: "Essential tools only — fast startup, minimal context window usage",
        enabled_categories: vec![CATEGORY_REPOSITORY, CATEGORY_SEARCH],
        tool_count: 15,
        estimated_tokens: 3_000,
    }
}

/// Balanced preset — good for VS Code, Cursor, JetBrains.
///
/// Adds CallGraph and Context on top of Minimal.
pub fn balanced_preset() -> PresetDefinition {
    PresetDefinition {
        name: PresetName::Balanced,
        description: "Good defaults — search, call graphs, and context assembly",
        enabled_categories: vec![
            CATEGORY_REPOSITORY,
            CATEGORY_SEARCH,
            CATEGORY_CALL_GRAPH,
            CATEGORY_CONTEXT,
        ],
        tool_count: 30,
        estimated_tokens: 6_000,
    }
}

/// Full preset — everything enabled. Best for Claude Desktop, Claude Code.
///
/// All 7 categories active.
pub fn full_preset() -> PresetDefinition {
    PresetDefinition {
        name: PresetName::Full,
        description: "All tools enabled — maximum capabilities for comprehensive analysis",
        enabled_categories: ALL_CATEGORIES.to_vec(),
        tool_count: 50,
        estimated_tokens: 10_000,
    }
}

/// Security-focused preset — security + analysis + repository basics.
///
/// Prioritises Security and Analysis, drops Git and Context to keep
/// the token budget focused.
pub fn security_preset() -> PresetDefinition {
    PresetDefinition {
        name: PresetName::SecurityFocused,
        description: "Security and analysis tools — vulnerability scanning, dead code, data flow",
        enabled_categories: vec![
            CATEGORY_REPOSITORY,
            CATEGORY_SEARCH,
            CATEGORY_ANALYSIS,
            CATEGORY_SECURITY,
        ],
        tool_count: 25,
        estimated_tokens: 5_000,
    }
}

/// Return the set of enabled category names for a preset.
pub fn enabled_categories(name: &PresetName) -> HashSet<&'static str> {
    get_preset(name).enabled_categories.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_preset_categories() {
        let p = minimal_preset();
        assert_eq!(p.name, PresetName::Minimal);
        assert_eq!(p.enabled_categories.len(), 2);
        assert!(p.enabled_categories.contains(&CATEGORY_REPOSITORY));
        assert!(p.enabled_categories.contains(&CATEGORY_SEARCH));
        assert!(!p.enabled_categories.contains(&CATEGORY_GIT));
    }

    #[test]
    fn test_minimal_preset_token_budget() {
        let p = minimal_preset();
        assert_eq!(p.tool_count, 15);
        assert_eq!(p.estimated_tokens, 3_000);
    }

    #[test]
    fn test_balanced_preset_categories() {
        let p = balanced_preset();
        assert_eq!(p.name, PresetName::Balanced);
        assert_eq!(p.enabled_categories.len(), 4);
        assert!(p.enabled_categories.contains(&CATEGORY_CALL_GRAPH));
        assert!(p.enabled_categories.contains(&CATEGORY_CONTEXT));
        assert!(!p.enabled_categories.contains(&CATEGORY_SECURITY));
    }

    #[test]
    fn test_balanced_preset_token_budget() {
        let p = balanced_preset();
        assert_eq!(p.tool_count, 30);
        assert_eq!(p.estimated_tokens, 6_000);
    }

    #[test]
    fn test_full_preset_all_categories() {
        let p = full_preset();
        assert_eq!(p.name, PresetName::Full);
        assert_eq!(p.enabled_categories.len(), ALL_CATEGORIES.len());
        for cat in ALL_CATEGORIES {
            assert!(
                p.enabled_categories.contains(cat),
                "missing category: {cat}"
            );
        }
    }

    #[test]
    fn test_full_preset_token_budget() {
        let p = full_preset();
        assert_eq!(p.tool_count, 50);
        assert_eq!(p.estimated_tokens, 10_000);
    }

    #[test]
    fn test_security_preset_categories() {
        let p = security_preset();
        assert_eq!(p.name, PresetName::SecurityFocused);
        assert!(p.enabled_categories.contains(&CATEGORY_SECURITY));
        assert!(p.enabled_categories.contains(&CATEGORY_ANALYSIS));
        assert!(p.enabled_categories.contains(&CATEGORY_REPOSITORY));
        assert!(!p.enabled_categories.contains(&CATEGORY_GIT));
        assert!(!p.enabled_categories.contains(&CATEGORY_CONTEXT));
    }

    #[test]
    fn test_security_preset_token_budget() {
        let p = security_preset();
        assert_eq!(p.tool_count, 25);
        assert_eq!(p.estimated_tokens, 5_000);
    }

    #[test]
    fn test_get_preset_dispatches_correctly() {
        assert_eq!(get_preset(&PresetName::Minimal).name, PresetName::Minimal);
        assert_eq!(get_preset(&PresetName::Balanced).name, PresetName::Balanced);
        assert_eq!(get_preset(&PresetName::Full).name, PresetName::Full);
        assert_eq!(
            get_preset(&PresetName::SecurityFocused).name,
            PresetName::SecurityFocused
        );
    }

    #[test]
    fn test_enabled_categories_set() {
        let cats = enabled_categories(&PresetName::Minimal);
        assert_eq!(cats.len(), 2);
        assert!(cats.contains(CATEGORY_REPOSITORY));
        assert!(cats.contains(CATEGORY_SEARCH));
    }

    #[test]
    fn test_full_enabled_categories_set() {
        let cats = enabled_categories(&PresetName::Full);
        assert_eq!(cats.len(), ALL_CATEGORIES.len());
    }

    #[test]
    fn test_all_categories_constant() {
        assert_eq!(ALL_CATEGORIES.len(), 7);
        assert!(ALL_CATEGORIES.contains(&"Repository"));
        assert!(ALL_CATEGORIES.contains(&"Security"));
        assert!(ALL_CATEGORIES.contains(&"Git"));
    }

    #[test]
    fn test_preset_descriptions_not_empty() {
        for name in [
            PresetName::Minimal,
            PresetName::Balanced,
            PresetName::Full,
            PresetName::SecurityFocused,
        ] {
            let p = get_preset(&name);
            assert!(!p.description.is_empty(), "empty description for {name}");
        }
    }

    #[test]
    fn test_preset_token_ordering() {
        let m = minimal_preset();
        let b = balanced_preset();
        let s = security_preset();
        let f = full_preset();
        assert!(m.estimated_tokens < b.estimated_tokens);
        assert!(s.estimated_tokens < f.estimated_tokens);
        assert!(b.estimated_tokens < f.estimated_tokens);
    }

    #[test]
    fn test_preset_tool_count_ordering() {
        let m = minimal_preset();
        let s = security_preset();
        let b = balanced_preset();
        let f = full_preset();
        assert!(m.tool_count < b.tool_count);
        assert!(s.tool_count < f.tool_count);
    }

    // ====================================================================
    // Phase 18B — extended preset tests
    // ====================================================================

    use pretty_assertions::assert_eq as pa_eq;
    use test_case::test_case;

    // --- Each preset has valid name ---

    #[test_case(PresetName::Minimal ; "minimal has correct name")]
    #[test_case(PresetName::Balanced ; "balanced has correct name")]
    #[test_case(PresetName::Full ; "full has correct name")]
    #[test_case(PresetName::SecurityFocused ; "security has correct name")]
    fn get_preset_returns_matching_name(name: PresetName) {
        let p = get_preset(&name);
        pa_eq!(p.name, name);
    }

    // --- All presets have non-zero tool counts ---

    #[test_case(PresetName::Minimal ; "minimal tools gt 0")]
    #[test_case(PresetName::Balanced ; "balanced tools gt 0")]
    #[test_case(PresetName::Full ; "full tools gt 0")]
    #[test_case(PresetName::SecurityFocused ; "security tools gt 0")]
    fn preset_tool_count_positive(name: PresetName) {
        let p = get_preset(&name);
        assert!(p.tool_count > 0, "tool_count should be > 0 for {:?}", name);
    }

    // --- All presets have non-zero estimated_tokens ---

    #[test_case(PresetName::Minimal ; "minimal tokens gt 0")]
    #[test_case(PresetName::Balanced ; "balanced tokens gt 0")]
    #[test_case(PresetName::Full ; "full tokens gt 0")]
    #[test_case(PresetName::SecurityFocused ; "security tokens gt 0")]
    fn preset_estimated_tokens_positive(name: PresetName) {
        let p = get_preset(&name);
        assert!(
            p.estimated_tokens > 0,
            "estimated_tokens should be > 0 for {:?}",
            name
        );
    }

    // --- All presets have non-empty categories ---

    #[test_case(PresetName::Minimal ; "minimal categories non-empty")]
    #[test_case(PresetName::Balanced ; "balanced categories non-empty")]
    #[test_case(PresetName::Full ; "full categories non-empty")]
    #[test_case(PresetName::SecurityFocused ; "security categories non-empty")]
    fn preset_categories_not_empty(name: PresetName) {
        let p = get_preset(&name);
        assert!(
            !p.enabled_categories.is_empty(),
            "categories should not be empty for {:?}",
            name
        );
    }

    // --- All presets have non-empty descriptions ---

    #[test_case(PresetName::Minimal ; "minimal desc non-empty")]
    #[test_case(PresetName::Balanced ; "balanced desc non-empty")]
    #[test_case(PresetName::Full ; "full desc non-empty")]
    #[test_case(PresetName::SecurityFocused ; "security desc non-empty")]
    fn preset_description_not_empty(name: PresetName) {
        let p = get_preset(&name);
        assert!(
            !p.description.is_empty(),
            "description should not be empty for {:?}",
            name
        );
    }

    // --- Repository is always included ---

    #[test_case(PresetName::Minimal ; "minimal has repo")]
    #[test_case(PresetName::Balanced ; "balanced has repo")]
    #[test_case(PresetName::Full ; "full has repo")]
    #[test_case(PresetName::SecurityFocused ; "security has repo")]
    fn all_presets_include_repository(name: PresetName) {
        let p = get_preset(&name);
        assert!(p.enabled_categories.contains(&CATEGORY_REPOSITORY));
    }

    // --- Search is always included ---

    #[test_case(PresetName::Minimal ; "minimal has search")]
    #[test_case(PresetName::Balanced ; "balanced has search")]
    #[test_case(PresetName::Full ; "full has search")]
    #[test_case(PresetName::SecurityFocused ; "security has search")]
    fn all_presets_include_search(name: PresetName) {
        let p = get_preset(&name);
        assert!(p.enabled_categories.contains(&CATEGORY_SEARCH));
    }

    // --- Security-focused preset specifics ---

    #[test]
    fn security_preset_includes_security_category() {
        let p = security_preset();
        assert!(p.enabled_categories.contains(&CATEGORY_SECURITY));
    }

    #[test]
    fn security_preset_includes_analysis_category() {
        let p = security_preset();
        assert!(p.enabled_categories.contains(&CATEGORY_ANALYSIS));
    }

    #[test]
    fn security_preset_excludes_git() {
        let p = security_preset();
        assert!(!p.enabled_categories.contains(&CATEGORY_GIT));
    }

    #[test]
    fn security_preset_excludes_context() {
        let p = security_preset();
        assert!(!p.enabled_categories.contains(&CATEGORY_CONTEXT));
    }

    // --- Full preset has ALL_CATEGORIES exactly ---

    #[test]
    fn full_preset_categories_match_all() {
        let p = full_preset();
        let cats: std::collections::HashSet<&str> = p.enabled_categories.into_iter().collect();
        let all: std::collections::HashSet<&str> = ALL_CATEGORIES.iter().copied().collect();
        pa_eq!(cats, all);
    }

    // --- Minimal preset exclusions ---

    #[test]
    fn minimal_preset_excludes_security() {
        let p = minimal_preset();
        assert!(!p.enabled_categories.contains(&CATEGORY_SECURITY));
    }

    #[test]
    fn minimal_preset_excludes_git() {
        let p = minimal_preset();
        assert!(!p.enabled_categories.contains(&CATEGORY_GIT));
    }

    #[test]
    fn minimal_preset_excludes_analysis() {
        let p = minimal_preset();
        assert!(!p.enabled_categories.contains(&CATEGORY_ANALYSIS));
    }

    #[test]
    fn minimal_preset_excludes_context() {
        let p = minimal_preset();
        assert!(!p.enabled_categories.contains(&CATEGORY_CONTEXT));
    }

    #[test]
    fn minimal_preset_excludes_callgraph() {
        let p = minimal_preset();
        assert!(!p.enabled_categories.contains(&CATEGORY_CALL_GRAPH));
    }

    // --- Balanced preset details ---

    #[test]
    fn balanced_preset_includes_callgraph() {
        let p = balanced_preset();
        assert!(p.enabled_categories.contains(&CATEGORY_CALL_GRAPH));
    }

    #[test]
    fn balanced_preset_includes_context() {
        let p = balanced_preset();
        assert!(p.enabled_categories.contains(&CATEGORY_CONTEXT));
    }

    #[test]
    fn balanced_preset_excludes_security() {
        let p = balanced_preset();
        assert!(!p.enabled_categories.contains(&CATEGORY_SECURITY));
    }

    #[test]
    fn balanced_preset_excludes_git() {
        let p = balanced_preset();
        assert!(!p.enabled_categories.contains(&CATEGORY_GIT));
    }

    // --- enabled_categories returns correct set sizes ---

    #[test_case(PresetName::Minimal, 2 ; "minimal set size")]
    #[test_case(PresetName::Balanced, 4 ; "balanced set size")]
    #[test_case(PresetName::Full, 7 ; "full set size")]
    #[test_case(PresetName::SecurityFocused, 4 ; "security set size")]
    fn enabled_categories_set_size(name: PresetName, expected: usize) {
        let cats = enabled_categories(&name);
        pa_eq!(cats.len(), expected);
    }

    // --- ALL_CATEGORIES ---

    #[test]
    fn all_categories_has_7_entries() {
        pa_eq!(ALL_CATEGORIES.len(), 7);
    }

    #[test]
    fn all_categories_contains_expected() {
        for expected in [
            "Repository",
            "Search",
            "CallGraph",
            "Analysis",
            "Security",
            "Git",
            "Context",
        ] {
            assert!(
                ALL_CATEGORIES.contains(&expected),
                "missing category: {}",
                expected
            );
        }
    }

    // --- Token budget reasonableness ---

    #[test]
    fn minimal_token_budget_reasonable() {
        let p = minimal_preset();
        assert!(p.estimated_tokens >= 1_000);
        assert!(p.estimated_tokens <= 5_000);
    }

    #[test]
    fn full_token_budget_reasonable() {
        let p = full_preset();
        assert!(p.estimated_tokens >= 5_000);
        assert!(p.estimated_tokens <= 20_000);
    }
}

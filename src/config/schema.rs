//! Configuration data structures for CodeGraph.
//!
//! Defines the YAML config format: presets, tool overrides, category toggles,
//! and performance budgets. Designed for multi-source loading with serde.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Top-level config
// ---------------------------------------------------------------------------

/// Root configuration for CodeGraph.
///
/// Loaded from YAML files, environment variables, and CLI flags.
/// Multiple sources are merged with well-defined priority.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeGraphConfig {
    /// Config format version (currently "1.0").
    #[serde(default = "default_version")]
    pub version: String,

    /// Active preset name.
    #[serde(default = "default_preset")]
    pub preset: PresetName,

    /// Per-tool and per-category overrides.
    #[serde(default)]
    pub tools: ToolsConfig,

    /// Performance tuning knobs.
    #[serde(default)]
    pub performance: PerformanceConfig,
}

impl Default for CodeGraphConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            preset: PresetName::Full,
            tools: ToolsConfig::default(),
            performance: PerformanceConfig::default(),
        }
    }
}

impl CodeGraphConfig {
    /// Check whether a specific category is enabled (defaults to true).
    pub fn is_category_enabled(&self, category: &str) -> bool {
        self.tools
            .categories
            .get(category)
            .map(|c| c.enabled)
            .unwrap_or(true)
    }

    /// Check whether a specific tool is enabled (defaults to true).
    pub fn is_tool_enabled(&self, tool_name: &str) -> bool {
        self.tools
            .overrides
            .get(tool_name)
            .map(|o| o.enabled)
            .unwrap_or(true)
    }
}

// ---------------------------------------------------------------------------
// PresetName
// ---------------------------------------------------------------------------

/// Named presets that control which tool categories are active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresetName {
    /// Essential tools only (~15 tools, ~3 000 tokens).
    Minimal,
    /// Good defaults for most editors (~30 tools, ~6 000 tokens).
    Balanced,
    /// All tools enabled (~50+ tools, ~10 000 tokens).
    Full,
    /// Security and analysis tools prioritized.
    #[serde(rename = "security-focused")]
    SecurityFocused,
}

impl PresetName {
    /// Parse from a loose string (case-insensitive, underscores accepted).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "minimal" => Some(Self::Minimal),
            "balanced" => Some(Self::Balanced),
            "full" => Some(Self::Full),
            "security-focused" | "security_focused" | "securityfocused" => {
                Some(Self::SecurityFocused)
            }
            _ => None,
        }
    }

    /// Canonical string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Balanced => "balanced",
            Self::Full => "full",
            Self::SecurityFocused => "security-focused",
        }
    }
}

impl std::fmt::Display for PresetName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// ToolsConfig
// ---------------------------------------------------------------------------

/// Per-tool and per-category configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolsConfig {
    /// Individual tool overrides (enable/disable specific tools).
    #[serde(default)]
    pub overrides: HashMap<String, ToolOverride>,

    /// Category-level toggles (enable/disable entire groups).
    #[serde(default)]
    pub categories: HashMap<String, CategoryConfig>,
}

// ---------------------------------------------------------------------------
// ToolOverride
// ---------------------------------------------------------------------------

/// Override the enabled state of a single tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOverride {
    /// Whether this tool is enabled.
    pub enabled: bool,

    /// Human-readable reason for the override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl ToolOverride {
    /// Create a disabled override with a reason.
    pub fn disabled(reason: impl Into<String>) -> Self {
        Self {
            enabled: false,
            reason: Some(reason.into()),
        }
    }

    /// Create an enabled override.
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            reason: None,
        }
    }
}

// ---------------------------------------------------------------------------
// CategoryConfig
// ---------------------------------------------------------------------------

/// Enable or disable an entire tool category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryConfig {
    /// Whether this category is enabled.
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// PerformanceConfig
// ---------------------------------------------------------------------------

/// Performance tuning knobs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Maximum number of tools to expose to the MCP client.
    /// Tools beyond this limit are dropped (lowest-priority first).
    #[serde(default = "default_max_tool_count")]
    pub max_tool_count: Option<usize>,

    /// Whether to exclude test files from indexing.
    #[serde(default)]
    pub exclude_tests: bool,
}

// ---------------------------------------------------------------------------
// ToolMetadata (for filtering)
// ---------------------------------------------------------------------------

/// Lightweight metadata about a single MCP tool, used for filtering.
#[derive(Debug, Clone)]
pub struct ToolMetadata {
    /// Tool name as registered in the MCP server.
    pub name: String,
    /// Category this tool belongs to.
    pub category: String,
    /// Human-readable description.
    pub description: String,
    /// Estimated token cost of this tool's schema in the system prompt.
    pub estimated_tokens: usize,
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

fn default_version() -> String {
    "1.0".to_string()
}

fn default_preset() -> PresetName {
    PresetName::Full
}

fn default_max_tool_count() -> Option<usize> {
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CodeGraphConfig::default();
        assert_eq!(config.version, "1.0");
        assert_eq!(config.preset, PresetName::Full);
        assert!(config.tools.overrides.is_empty());
        assert!(config.tools.categories.is_empty());
        assert_eq!(config.performance.max_tool_count, None);
        assert!(!config.performance.exclude_tests);
    }

    #[test]
    fn test_preset_name_roundtrip() {
        for preset in [
            PresetName::Minimal,
            PresetName::Balanced,
            PresetName::Full,
            PresetName::SecurityFocused,
        ] {
            let s = preset.as_str();
            assert_eq!(
                PresetName::from_str_loose(s),
                Some(preset),
                "roundtrip failed for {s}"
            );
        }
    }

    #[test]
    fn test_preset_name_loose_parsing() {
        assert_eq!(
            PresetName::from_str_loose("MINIMAL"),
            Some(PresetName::Minimal)
        );
        assert_eq!(
            PresetName::from_str_loose("  balanced  "),
            Some(PresetName::Balanced)
        );
        assert_eq!(
            PresetName::from_str_loose("security_focused"),
            Some(PresetName::SecurityFocused)
        );
        assert_eq!(
            PresetName::from_str_loose("securityfocused"),
            Some(PresetName::SecurityFocused)
        );
        assert_eq!(PresetName::from_str_loose("unknown"), None);
        assert_eq!(PresetName::from_str_loose(""), None);
    }

    #[test]
    fn test_preset_name_display() {
        assert_eq!(format!("{}", PresetName::Minimal), "minimal");
        assert_eq!(format!("{}", PresetName::Full), "full");
        assert_eq!(
            format!("{}", PresetName::SecurityFocused),
            "security-focused"
        );
    }

    #[test]
    fn test_serde_yaml_roundtrip() {
        let config = CodeGraphConfig {
            version: "1.0".to_string(),
            preset: PresetName::Balanced,
            tools: ToolsConfig::default(),
            performance: PerformanceConfig {
                max_tool_count: Some(30),
                exclude_tests: true,
            },
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        let back: CodeGraphConfig = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(back.version, "1.0");
        assert_eq!(back.preset, PresetName::Balanced);
        assert_eq!(back.performance.max_tool_count, Some(30));
        assert!(back.performance.exclude_tests);
    }

    #[test]
    fn test_serde_json_roundtrip() {
        let config = CodeGraphConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let back: CodeGraphConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.preset, PresetName::Full);
    }

    #[test]
    fn test_preset_only_yaml() {
        let yaml = r#"preset: "minimal""#;
        let config: CodeGraphConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.preset, PresetName::Minimal);
        assert_eq!(config.version, "1.0"); // default
    }

    #[test]
    fn test_full_yaml_config() {
        let yaml = r#"
version: "1.0"
preset: balanced
tools:
  overrides:
    codegraph_dead_code:
      enabled: false
      reason: "Too slow for interactive use"
  categories:
    Security:
      enabled: true
    Git:
      enabled: false
performance:
  max_tool_count: 25
  exclude_tests: true
"#;
        let config: CodeGraphConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.preset, PresetName::Balanced);
        assert!(!config.is_tool_enabled("codegraph_dead_code"));
        assert!(config.is_tool_enabled("codegraph_query")); // not overridden
        assert!(config.is_category_enabled("Security"));
        assert!(!config.is_category_enabled("Git"));
        assert!(config.is_category_enabled("Unknown")); // default true
        assert_eq!(config.performance.max_tool_count, Some(25));
        assert!(config.performance.exclude_tests);
    }

    #[test]
    fn test_invalid_yaml_returns_error() {
        let yaml = "{{invalid yaml}}";
        let result: Result<CodeGraphConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_override_disabled() {
        let ov = ToolOverride::disabled("too slow");
        assert!(!ov.enabled);
        assert_eq!(ov.reason.as_deref(), Some("too slow"));
    }

    #[test]
    fn test_tool_override_enabled() {
        let ov = ToolOverride::enabled();
        assert!(ov.enabled);
        assert!(ov.reason.is_none());
    }

    #[test]
    fn test_category_config_serde() {
        let yaml = r#"enabled: false"#;
        let cat: CategoryConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!cat.enabled);
    }

    #[test]
    fn test_performance_defaults() {
        let perf = PerformanceConfig::default();
        assert_eq!(perf.max_tool_count, None);
        assert!(!perf.exclude_tests);
    }

    #[test]
    fn test_security_focused_preset_yaml() {
        let yaml = r#"preset: "security-focused""#;
        let config: CodeGraphConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.preset, PresetName::SecurityFocused);
    }

    // ====================================================================
    // Phase 18B — extended config schema tests
    // ====================================================================

    use pretty_assertions::assert_eq as pa_eq;
    use proptest::prelude::*;
    use test_case::test_case;

    // --- PresetName from_str_loose parameterised ---

    #[test_case("minimal", Some(PresetName::Minimal) ; "minimal lowercase")]
    #[test_case("MINIMAL", Some(PresetName::Minimal) ; "minimal uppercase")]
    #[test_case("Minimal", Some(PresetName::Minimal) ; "minimal mixed")]
    #[test_case("balanced", Some(PresetName::Balanced) ; "balanced lowercase")]
    #[test_case("BALANCED", Some(PresetName::Balanced) ; "balanced uppercase")]
    #[test_case("full", Some(PresetName::Full) ; "full lowercase")]
    #[test_case("FULL", Some(PresetName::Full) ; "full uppercase")]
    #[test_case("security-focused", Some(PresetName::SecurityFocused) ; "secfocused hyphen")]
    #[test_case("security_focused", Some(PresetName::SecurityFocused) ; "secfocused underscore")]
    #[test_case("securityfocused", Some(PresetName::SecurityFocused) ; "secfocused concatenated")]
    #[test_case("SecurityFocused", Some(PresetName::SecurityFocused) ; "secfocused pascal")]
    #[test_case("", None ; "empty string")]
    #[test_case("unknown", None ; "unknown string")]
    #[test_case("   minimal   ", Some(PresetName::Minimal) ; "whitespace padded")]
    fn preset_from_str_loose(input: &str, expected: Option<PresetName>) {
        pa_eq!(PresetName::from_str_loose(input), expected);
    }

    // --- PresetName as_str ---

    #[test_case(PresetName::Minimal, "minimal" ; "minimal as str")]
    #[test_case(PresetName::Balanced, "balanced" ; "balanced as str")]
    #[test_case(PresetName::Full, "full" ; "full as str")]
    #[test_case(PresetName::SecurityFocused, "security-focused" ; "security as str")]
    fn preset_as_str(name: PresetName, expected: &str) {
        pa_eq!(name.as_str(), expected);
    }

    // --- PresetName display matches as_str ---

    #[test]
    fn preset_display_matches_as_str() {
        for p in [
            PresetName::Minimal,
            PresetName::Balanced,
            PresetName::Full,
            PresetName::SecurityFocused,
        ] {
            pa_eq!(format!("{}", p), p.as_str());
        }
    }

    // --- CodeGraphConfig defaults ---

    #[test]
    fn default_config_is_category_enabled_returns_true() {
        let config = CodeGraphConfig::default();
        assert!(config.is_category_enabled("Anything"));
        assert!(config.is_category_enabled("Security"));
        assert!(config.is_category_enabled("NonExistent"));
    }

    #[test]
    fn default_config_is_tool_enabled_returns_true() {
        let config = CodeGraphConfig::default();
        assert!(config.is_tool_enabled("codegraph_query"));
        assert!(config.is_tool_enabled("nonexistent_tool"));
    }

    // --- CodeGraphConfig with overrides ---

    #[test]
    fn config_disabled_category() {
        let mut config = CodeGraphConfig::default();
        config
            .tools
            .categories
            .insert("Git".to_string(), CategoryConfig { enabled: false });
        assert!(!config.is_category_enabled("Git"));
        assert!(config.is_category_enabled("Security")); // not touched
    }

    #[test]
    fn config_disabled_tool() {
        let mut config = CodeGraphConfig::default();
        config.tools.overrides.insert(
            "codegraph_dead_code".to_string(),
            ToolOverride::disabled("slow"),
        );
        assert!(!config.is_tool_enabled("codegraph_dead_code"));
        assert!(config.is_tool_enabled("codegraph_query"));
    }

    #[test]
    fn config_enabled_tool_override() {
        let mut config = CodeGraphConfig::default();
        config
            .tools
            .overrides
            .insert("codegraph_query".to_string(), ToolOverride::enabled());
        assert!(config.is_tool_enabled("codegraph_query"));
    }

    // --- ToolOverride ---

    #[test]
    fn tool_override_disabled_has_reason() {
        let ov = ToolOverride::disabled("too slow for interactive use");
        assert!(!ov.enabled);
        pa_eq!(ov.reason.as_deref(), Some("too slow for interactive use"));
    }

    #[test]
    fn tool_override_enabled_no_reason() {
        let ov = ToolOverride::enabled();
        assert!(ov.enabled);
        assert!(ov.reason.is_none());
    }

    #[test]
    fn tool_override_serde_roundtrip() {
        let ov = ToolOverride::disabled("reason");
        let json = serde_json::to_string(&ov).unwrap();
        let back: ToolOverride = serde_json::from_str(&json).unwrap();
        pa_eq!(back.enabled, false);
        pa_eq!(back.reason.as_deref(), Some("reason"));
    }

    #[test]
    fn tool_override_enabled_serde_roundtrip() {
        let ov = ToolOverride::enabled();
        let json = serde_json::to_string(&ov).unwrap();
        let back: ToolOverride = serde_json::from_str(&json).unwrap();
        assert!(back.enabled);
        // reason is skipped when None
        assert!(back.reason.is_none());
    }

    // --- CategoryConfig ---

    #[test]
    fn category_config_enabled_serde() {
        let yaml = "enabled: true";
        let cat: CategoryConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cat.enabled);
    }

    #[test]
    fn category_config_disabled_serde() {
        let yaml = "enabled: false";
        let cat: CategoryConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!cat.enabled);
    }

    // --- PerformanceConfig ---

    #[test]
    fn performance_config_with_max_tool_count() {
        let yaml = "max_tool_count: 25\nexclude_tests: true";
        let perf: PerformanceConfig = serde_yaml::from_str(yaml).unwrap();
        pa_eq!(perf.max_tool_count, Some(25));
        assert!(perf.exclude_tests);
    }

    #[test]
    fn performance_config_null_max_tool_count() {
        let yaml = "max_tool_count: null\nexclude_tests: false";
        let perf: PerformanceConfig = serde_yaml::from_str(yaml).unwrap();
        pa_eq!(perf.max_tool_count, None);
        assert!(!perf.exclude_tests);
    }

    // --- Full YAML config variations ---

    #[test]
    fn config_empty_yaml_uses_defaults() {
        let yaml = "{}";
        let config: CodeGraphConfig = serde_yaml::from_str(yaml).unwrap();
        pa_eq!(config.version, "1.0");
        pa_eq!(config.preset, PresetName::Full);
    }

    #[test]
    fn config_version_only_yaml() {
        let yaml = r#"version: "2.0""#;
        let config: CodeGraphConfig = serde_yaml::from_str(yaml).unwrap();
        pa_eq!(config.version, "2.0");
        pa_eq!(config.preset, PresetName::Full); // default
    }

    #[test]
    fn config_multiple_tool_overrides() {
        let yaml = r#"
tools:
  overrides:
    tool_a:
      enabled: false
      reason: "disabled by user"
    tool_b:
      enabled: true
    tool_c:
      enabled: false
      reason: "too slow"
"#;
        let config: CodeGraphConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!config.is_tool_enabled("tool_a"));
        assert!(config.is_tool_enabled("tool_b"));
        assert!(!config.is_tool_enabled("tool_c"));
    }

    #[test]
    fn config_multiple_category_overrides() {
        let yaml = r#"
tools:
  categories:
    Security:
      enabled: true
    Git:
      enabled: false
    Analysis:
      enabled: true
"#;
        let config: CodeGraphConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.is_category_enabled("Security"));
        assert!(!config.is_category_enabled("Git"));
        assert!(config.is_category_enabled("Analysis"));
        assert!(config.is_category_enabled("Unknown")); // default true
    }

    // --- proptest: serialization roundtrip ---

    proptest! {
        #[test]
        fn config_yaml_roundtrip_proptest(preset_idx in 0u8..4) {
            let presets = [PresetName::Minimal, PresetName::Balanced, PresetName::Full, PresetName::SecurityFocused];
            let name = presets[preset_idx as usize];
            let config = CodeGraphConfig { preset: name, ..Default::default() };
            let yaml = serde_yaml::to_string(&config).unwrap();
            let back: CodeGraphConfig = serde_yaml::from_str(&yaml).unwrap();
            pa_eq!(config.version, back.version);
            pa_eq!(config.preset, back.preset);
        }

        #[test]
        fn config_json_roundtrip_proptest(preset_idx in 0u8..4) {
            let presets = [PresetName::Minimal, PresetName::Balanced, PresetName::Full, PresetName::SecurityFocused];
            let name = presets[preset_idx as usize];
            let config = CodeGraphConfig { preset: name, ..Default::default() };
            let json = serde_json::to_string(&config).unwrap();
            let back: CodeGraphConfig = serde_json::from_str(&json).unwrap();
            pa_eq!(config.preset, back.preset);
        }

        #[test]
        fn preset_from_str_loose_never_panics(s in "\\PC{0,50}") {
            let _ = PresetName::from_str_loose(&s);
        }
    }

    // --- ToolMetadata ---

    #[test]
    fn tool_metadata_fields() {
        let meta = ToolMetadata {
            name: "codegraph_query".into(),
            category: "Search".into(),
            description: "Hybrid search".into(),
            estimated_tokens: 200,
        };
        pa_eq!(meta.name, "codegraph_query");
        pa_eq!(meta.category, "Search");
        pa_eq!(meta.estimated_tokens, 200);
    }
}

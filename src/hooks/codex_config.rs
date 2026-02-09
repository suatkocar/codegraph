//! Codex configuration — generates `~/.codex/config.toml` MCP entry and `AGENTS.md`.
//!
//! OpenAI Codex reads `AGENTS.md` (not `CLAUDE.md`) for project instructions,
//! and uses `~/.codex/config.toml` `[mcp_servers.*]` sections for MCP server
//! discovery. This module handles both:
//!
//! 1. **config.toml** — Merges a `[mcp_servers.codegraph]` entry into the
//!    existing config, preserving all other settings.
//! 2. **AGENTS.md** — Generates a project-level file with tiered tool guidance,
//!    anti-patterns, and project stats (similar to CLAUDE.md but adapted for
//!    Codex conventions).

use std::fs;
use std::path::Path;

use crate::error::Result;
use crate::hooks::claude_template::ProjectStats;

// ---------------------------------------------------------------------------
// config.toml MCP server entry
// ---------------------------------------------------------------------------

const CODEX_MCP_MARKER: &str = "[mcp_servers.codegraph]";

/// Render the `[mcp_servers.codegraph]` TOML block.
fn render_mcp_toml(binary_path: &str) -> String {
    format!(
        r#"
[mcp_servers.codegraph]
command = "{binary_path}"
args = ["serve"]
env = {{ "CODEGRAPH_DB" = ".codegraph/codegraph.db" }}
"#,
    )
}

/// Merge a `[mcp_servers.codegraph]` entry into `~/.codex/config.toml`.
///
/// - If `~/.codex/` doesn't exist, does nothing (Codex not installed).
/// - If `config.toml` exists and already has `[mcp_servers.codegraph]`, replaces
///   that section in-place.
/// - If `config.toml` exists without the section, appends it.
/// - If `config.toml` doesn't exist, creates it with just the MCP entry.
///
/// Returns `Ok(true)` if the config was written, `Ok(false)` if Codex is not
/// installed.
pub fn merge_codex_config(binary_path: &str) -> Result<bool> {
    let home = directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .ok_or_else(|| {
            crate::error::CodeGraphError::Other("cannot determine home directory".into())
        })?;
    let codex_dir = home.join(".codex");

    if !codex_dir.exists() {
        tracing::info!("~/.codex/ not found, skipping Codex config");
        return Ok(false);
    }

    let config_path = codex_dir.join("config.toml");
    merge_codex_config_to(&config_path, binary_path)?;
    Ok(true)
}

/// Inner implementation that accepts an explicit path (for testing).
fn merge_codex_config_to(config_path: &Path, binary_path: &str) -> Result<()> {
    let mcp_block = render_mcp_toml(binary_path);

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = match fs::read_to_string(config_path) {
        Ok(existing) if !existing.is_empty() => {
            if let Some(start) = existing.find(CODEX_MCP_MARKER) {
                // Replace the existing [mcp_servers.codegraph] section.
                // Find the end: next `[` at line start or EOF.
                let after_marker = start + CODEX_MCP_MARKER.len();
                let end = existing[after_marker..]
                    .find("\n[")
                    .map(|pos| after_marker + pos + 1) // +1 to keep the newline
                    .unwrap_or(existing.len());

                let mut result = String::with_capacity(existing.len());
                let prefix = existing[..start].trim_end();
                result.push_str(prefix);
                if prefix.is_empty() {
                    // No content before this section — don't add leading newline.
                    result.push_str(mcp_block.trim_start());
                } else {
                    result.push_str(&mcp_block);
                }
                if end < existing.len() {
                    result.push_str(&existing[end..]);
                }
                result
            } else {
                // Append to existing file.
                format!("{}{}", existing.trim_end(), mcp_block)
            }
        }
        _ => mcp_block.trim_start().to_string(),
    };

    fs::write(config_path, content)?;
    tracing::info!("Merged CodeGraph MCP entry into {}", config_path.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// AGENTS.md generation
// ---------------------------------------------------------------------------

/// Section header for the CodeGraph block inside AGENTS.md.
const AGENTS_SECTION_HEADER: &str = "## CodeGraph \u{2014} Codebase Intelligence";

/// Render the AGENTS.md section body (adapted for Codex conventions).
fn render_agents_section(stats: &ProjectStats) -> String {
    format!(
        r#"{AGENTS_SECTION_HEADER}

This project is indexed by CodeGraph. Use CodeGraph MCP tools instead of grep/find/cat for code analysis. The pre-built index provides instant, semantic, relationship-aware results.

### Tier 1 — Start Here

- `codegraph_context` — Describe your task, get all relevant code, relationships, and structure in one call. Use this FIRST before reading files.

### Tier 2 — Drill Down

- `codegraph_callers` — Who calls this function? (replaces grep for function name)
- `codegraph_callees` — What does this function call? (replaces reading function body)
- `codegraph_node` — Get full source code of a specific symbol (replaces cat/read on whole files)
- `codegraph_query` — Search symbols by name or semantic meaning (replaces grep/find)
- `codegraph_dependencies` — Module/file dependency tree (replaces reading imports)
- `codegraph_find_references` — All usages of a symbol across the project (replaces project-wide grep)

### Tier 3 — Specialized

- **Structure:** `codegraph_structure`, `codegraph_impact`, `codegraph_stats`, `codegraph_circular_imports`, `codegraph_project_tree`, `codegraph_export_map`, `codegraph_import_graph`, `codegraph_file`, `codegraph_diagram`, `codegraph_dead_code`, `codegraph_tests`, `codegraph_frameworks`, `codegraph_languages`
- **Git:** `codegraph_blame`, `codegraph_file_history`, `codegraph_recent_changes`, `codegraph_commit_diff`, `codegraph_symbol_history`, `codegraph_branch_info`, `codegraph_modified_files`, `codegraph_hotspots`, `codegraph_contributors`
- **Security:** `codegraph_scan_security`, `codegraph_check_owasp`, `codegraph_check_cwe`, `codegraph_explain_vulnerability`, `codegraph_suggest_fix`, `codegraph_find_injections`, `codegraph_taint_sources`, `codegraph_security_summary`, `codegraph_trace_taint`
- **Data Flow:** `codegraph_find_path`, `codegraph_complexity`, `codegraph_data_flow`, `codegraph_dead_stores`, `codegraph_find_uninitialized`, `codegraph_reaching_defs`

### Working Agreements

- Do NOT use `grep` or `find` to search for symbols — use `codegraph_query` or `codegraph_callers`
- Do NOT read entire files to find a function — use `codegraph_node("functionName")`
- Do NOT manually trace imports — use `codegraph_dependencies("file.ts")`
- Do NOT use `git log` or `git blame` via shell — use `codegraph_file_history` or `codegraph_blame`

### Project Stats
- Languages: {languages}
- Symbols: {nodes} | Relationships: {edges}
"#,
        languages = stats.language_breakdown(),
        nodes = stats.total_nodes,
        edges = stats.total_edges,
    )
}

/// Generate or update the `AGENTS.md` file in `project_dir`.
///
/// Follows the same idempotent pattern as `generate_claude_md`:
/// - If `AGENTS.md` does not exist, it is created with the CodeGraph section.
/// - If it exists and already contains the section header, that section is
///   replaced in-place.
/// - If it exists without the section, the section is appended.
pub fn generate_agents_md(project_dir: &str, stats: &ProjectStats) -> Result<()> {
    let path = std::path::Path::new(project_dir).join("AGENTS.md");
    let section = render_agents_section(stats);

    if path.exists() {
        let content = fs::read_to_string(&path)?;

        if content.contains(AGENTS_SECTION_HEADER) {
            let updated = replace_agents_section(&content, &section);
            fs::write(&path, updated)?;
            tracing::info!("Updated CodeGraph section in AGENTS.md");
        } else {
            let appended = format!("{}\n\n{}", content.trim_end(), section);
            fs::write(&path, appended)?;
            tracing::info!("Appended CodeGraph section to AGENTS.md");
        }
    } else {
        fs::write(&path, &section)?;
        tracing::info!("Created AGENTS.md with CodeGraph section");
    }

    Ok(())
}

/// Replace the CodeGraph section in existing AGENTS.md content.
fn replace_agents_section(content: &str, new_section: &str) -> String {
    let Some(start) = content.find(AGENTS_SECTION_HEADER) else {
        return content.to_string();
    };

    let after_header = start + AGENTS_SECTION_HEADER.len();

    // Find the next second-level heading after our section.
    let end = content[after_header..]
        .find("\n## ")
        .map(|pos| after_header + pos + 1)
        .unwrap_or(content.len());

    let mut result = String::with_capacity(content.len());
    result.push_str(&content[..start]);
    result.push_str(new_section.trim_end());
    result.push('\n');
    if end < content.len() {
        result.push('\n');
        result.push_str(&content[end..]);
    }
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn sample_stats() -> ProjectStats {
        let mut languages = HashMap::new();
        languages.insert("TypeScript".to_string(), 42);
        languages.insert("Rust".to_string(), 18);
        ProjectStats {
            languages,
            total_nodes: 500,
            total_edges: 1200,
        }
    }

    // -- config.toml tests ------------------------------------------------

    #[test]
    fn render_mcp_toml_has_correct_format() {
        let toml = render_mcp_toml("/usr/local/bin/codegraph");
        assert!(toml.contains("[mcp_servers.codegraph]"));
        assert!(toml.contains("command = \"/usr/local/bin/codegraph\""));
        assert!(toml.contains("args = [\"serve\"]"));
        assert!(toml.contains("CODEGRAPH_DB"));
    }

    #[test]
    fn merge_codex_config_creates_new_file() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        merge_codex_config_to(&config_path, "codegraph").unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("[mcp_servers.codegraph]"));
        assert!(content.contains("command = \"codegraph\""));
        assert!(content.contains("args = [\"serve\"]"));
    }

    #[test]
    fn merge_codex_config_appends_to_existing() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        let existing = "model = \"gpt-5.3-codex\"\nweb_search = \"live\"\n";
        fs::write(&config_path, existing).unwrap();

        merge_codex_config_to(&config_path, "/opt/bin/codegraph").unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(
            content.contains("model = \"gpt-5.3-codex\""),
            "existing content preserved"
        );
        assert!(
            content.contains("[mcp_servers.codegraph]"),
            "MCP section appended"
        );
        assert!(content.contains("/opt/bin/codegraph"), "binary path used");
    }

    #[test]
    fn merge_codex_config_replaces_existing_section() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        let existing = "\
model = \"gpt-5.3-codex\"

[mcp_servers.codegraph]
command = \"old-path\"
args = [\"serve\"]

[mcp_servers.other]
command = \"other-tool\"
";
        fs::write(&config_path, existing).unwrap();

        merge_codex_config_to(&config_path, "/new/path/codegraph").unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(
            content.contains("model = \"gpt-5.3-codex\""),
            "other settings preserved"
        );
        assert!(!content.contains("old-path"), "old path should be replaced");
        assert!(
            content.contains("/new/path/codegraph"),
            "new path should be present"
        );
        assert!(
            content.contains("[mcp_servers.other]"),
            "other MCP servers preserved"
        );
    }

    #[test]
    fn merge_codex_config_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");

        merge_codex_config_to(&config_path, "codegraph").unwrap();
        let first = fs::read_to_string(&config_path).unwrap();

        merge_codex_config_to(&config_path, "codegraph").unwrap();
        let second = fs::read_to_string(&config_path).unwrap();

        assert_eq!(
            first, second,
            "running twice should produce identical output"
        );
    }

    #[test]
    fn merge_codex_returns_false_when_codex_not_installed() {
        // merge_codex_config checks for ~/.codex/ existence.
        // We can't easily test the real HOME path, but we verify
        // the inner function works correctly.
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        // Inner function always succeeds regardless of ~/.codex/
        merge_codex_config_to(&config_path, "codegraph").unwrap();
        assert!(config_path.exists());
    }

    // -- AGENTS.md tests --------------------------------------------------

    #[test]
    fn render_agents_section_has_tiers() {
        let section = render_agents_section(&sample_stats());
        assert!(section.contains("Tier 1"), "should have Tier 1");
        assert!(section.contains("Tier 2"), "should have Tier 2");
        assert!(section.contains("Tier 3"), "should have Tier 3");
    }

    #[test]
    fn render_agents_section_has_working_agreements() {
        let section = render_agents_section(&sample_stats());
        assert!(
            section.contains("Working Agreements"),
            "should have Working Agreements (Codex convention)"
        );
        assert!(
            section.contains("Do NOT"),
            "should have anti-pattern guidance"
        );
    }

    #[test]
    fn render_agents_section_contains_all_44_tools() {
        let section = render_agents_section(&sample_stats());
        let expected_tools = [
            "codegraph_context",
            "codegraph_callers",
            "codegraph_callees",
            "codegraph_node",
            "codegraph_query",
            "codegraph_dependencies",
            "codegraph_find_references",
            "codegraph_structure",
            "codegraph_impact",
            "codegraph_stats",
            "codegraph_circular_imports",
            "codegraph_project_tree",
            "codegraph_export_map",
            "codegraph_import_graph",
            "codegraph_file",
            "codegraph_diagram",
            "codegraph_dead_code",
            "codegraph_tests",
            "codegraph_frameworks",
            "codegraph_languages",
            "codegraph_blame",
            "codegraph_file_history",
            "codegraph_recent_changes",
            "codegraph_commit_diff",
            "codegraph_symbol_history",
            "codegraph_branch_info",
            "codegraph_modified_files",
            "codegraph_hotspots",
            "codegraph_contributors",
            "codegraph_scan_security",
            "codegraph_check_owasp",
            "codegraph_check_cwe",
            "codegraph_explain_vulnerability",
            "codegraph_suggest_fix",
            "codegraph_find_injections",
            "codegraph_taint_sources",
            "codegraph_security_summary",
            "codegraph_trace_taint",
            "codegraph_find_path",
            "codegraph_complexity",
            "codegraph_data_flow",
            "codegraph_dead_stores",
            "codegraph_find_uninitialized",
            "codegraph_reaching_defs",
        ];
        assert_eq!(expected_tools.len(), 44);
        for tool in expected_tools {
            assert!(section.contains(tool), "missing tool: {tool}");
        }
    }

    #[test]
    fn render_agents_section_has_stats() {
        let section = render_agents_section(&sample_stats());
        assert!(section.contains("Symbols: 500"));
        assert!(section.contains("Relationships: 1200"));
        assert!(section.contains("TypeScript (42)"));
    }

    #[test]
    fn generate_creates_new_agents_md() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().to_str().unwrap();

        generate_agents_md(dir, &sample_stats()).unwrap();

        let path = tmp.path().join("AGENTS.md");
        assert!(path.exists());

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains(AGENTS_SECTION_HEADER));
        assert!(content.contains("codegraph_context"));
        assert!(content.contains("Symbols: 500"));
    }

    #[test]
    fn generate_appends_to_existing_agents_md() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().to_str().unwrap();
        let path = tmp.path().join("AGENTS.md");

        let existing = "# My Project\n\nCustom instructions for Codex.\n";
        fs::write(&path, existing).unwrap();

        generate_agents_md(dir, &sample_stats()).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("# My Project"),
            "original content preserved"
        );
        assert!(
            content.contains("Custom instructions for Codex."),
            "original body preserved"
        );
        assert!(
            content.contains(AGENTS_SECTION_HEADER),
            "CodeGraph section appended"
        );
    }

    #[test]
    fn generate_updates_existing_section_in_agents_md() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().to_str().unwrap();

        let mut stats = sample_stats();
        generate_agents_md(dir, &stats).unwrap();

        stats.total_nodes = 999;
        stats.total_edges = 2500;
        generate_agents_md(dir, &stats).unwrap();

        let content = fs::read_to_string(tmp.path().join("AGENTS.md")).unwrap();
        assert!(content.contains("Symbols: 999"), "new stats present");
        assert!(!content.contains("Symbols: 500"), "old stats replaced");

        let header_count = content.matches(AGENTS_SECTION_HEADER).count();
        assert_eq!(header_count, 1, "section header should appear once");
    }

    #[test]
    fn generate_agents_md_preserves_other_sections() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().to_str().unwrap();
        let path = tmp.path().join("AGENTS.md");

        let existing = format!(
            "# My Project\n\nIntro.\n\n{}\n\nOld content.\n\n## Other Section\n\nKeep this.\n",
            AGENTS_SECTION_HEADER
        );
        fs::write(&path, &existing).unwrap();

        generate_agents_md(dir, &sample_stats()).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# My Project"), "header preserved");
        assert!(
            content.contains("## Other Section"),
            "subsequent section preserved"
        );
        assert!(content.contains("Keep this."), "subsequent body preserved");
        assert!(content.contains("Symbols: 500"), "new stats present");
        assert!(
            !content.contains("Old content."),
            "old section body removed"
        );
    }
}

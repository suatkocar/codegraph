//! Hook installation — writes shell scripts and config files for Claude Code integration.
//!
//! The [`install_hooks`] function performs three non-destructive operations:
//!
//! 1. **Shell scripts** — Writes ten executable bash scripts into `.claude/hooks/`
//!    that delegate to `codegraph hook-*` subcommands.
//! 2. **`settings.json`** — Merges hook entries into `.claude/settings.json` so
//!    Claude Code invokes the scripts at the right lifecycle points.
//! 3. **`.mcp.json`** — Merges the CodeGraph MCP server entry so Claude Code
//!    can discover and launch it automatically.
//!
//! All JSON merges are additive: existing keys outside the CodeGraph namespace
//! are preserved verbatim.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serde_json::{json, Map, Value};

use crate::error::Result;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Install CodeGraph hooks, scripts, and MCP configuration into `project_dir`.
///
/// - `project_dir` — Root of the project (where `.claude/` lives).
/// - `binary_path` — Path or name of the `codegraph` binary (e.g. `"codegraph"`
///   or `"/usr/local/bin/codegraph"`).
///
/// This function is idempotent: running it twice produces the same result.
pub fn install_hooks(project_dir: &Path, binary_path: &str) -> Result<()> {
    let hooks_dir = project_dir.join(".claude").join("hooks");
    let settings_path = project_dir.join(".claude").join("settings.json");
    let mcp_path = project_dir.join(".mcp.json");

    // 1. Write shell scripts
    write_shell_scripts(&hooks_dir, binary_path)?;

    // 2. Merge hook entries into settings.json
    merge_settings(&settings_path)?;

    // 3. Merge MCP server entry into .mcp.json
    merge_mcp_config(&mcp_path, binary_path)?;

    tracing::info!("Hooks installed successfully.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Shell scripts
// ---------------------------------------------------------------------------

/// Metadata for a single hook script.
struct HookScript {
    filename: &'static str,
    subcommand: &'static str,
    comment: &'static str,
}

/// All hook scripts to install.
const HOOK_SCRIPTS: &[HookScript] = &[
    HookScript {
        filename: "session-start.sh",
        subcommand: "hook-session-start",
        comment: "CodeGraph session-start hook — re-index codebase",
    },
    HookScript {
        filename: "prompt-submit.sh",
        subcommand: "hook-prompt-submit",
        comment: "CodeGraph prompt-submit hook — inject relevant context",
    },
    HookScript {
        filename: "pre-compact.sh",
        subcommand: "hook-pre-compact",
        comment: "CodeGraph pre-compact hook — save graph summary",
    },
    HookScript {
        filename: "post-tool-use.sh",
        subcommand: "hook-post-edit",
        comment: "CodeGraph post-edit hook — re-index modified file",
    },
    HookScript {
        filename: "pre-tool-use.sh",
        subcommand: "hook-pre-tool-use",
        comment: "CodeGraph pre-tool-use hook — inject codebase context before tool execution",
    },
    HookScript {
        filename: "subagent-start.sh",
        subcommand: "hook-subagent-start",
        comment: "CodeGraph subagent-start hook — inject project overview into subagents",
    },
    HookScript {
        filename: "post-tool-failure.sh",
        subcommand: "hook-post-tool-failure",
        comment: "CodeGraph post-tool-failure hook — provide corrective context after failures",
    },
    HookScript {
        filename: "stop.sh",
        subcommand: "hook-stop",
        comment: "CodeGraph stop hook — quality check before agent stops",
    },
    HookScript {
        filename: "task-completed.sh",
        subcommand: "hook-task-completed",
        comment: "CodeGraph task-completed hook — quality gate on task completion",
    },
    HookScript {
        filename: "session-end.sh",
        subcommand: "hook-session-end",
        comment: "CodeGraph session-end hook — final re-index and diagnostics",
    },
];

/// Render a hook script body.
fn render_script(hook: &HookScript, binary_path: &str) -> String {
    format!(
        r#"#!/usr/bin/env bash
# {comment}
CODEGRAPH_BIN="${{CODEGRAPH_BIN:-{binary_path}}}"
"$CODEGRAPH_BIN" {subcommand} 2>/dev/null || echo '{{"continue":true}}'
"#,
        comment = hook.comment,
        binary_path = binary_path,
        subcommand = hook.subcommand,
    )
}

/// Write all hook shell scripts into `hooks_dir`, creating the directory if needed.
fn write_shell_scripts(hooks_dir: &Path, binary_path: &str) -> Result<()> {
    fs::create_dir_all(hooks_dir)?;

    for hook in HOOK_SCRIPTS {
        let path = hooks_dir.join(hook.filename);
        let body = render_script(hook, binary_path);
        fs::write(&path, body)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        tracing::info!("Wrote {}", path.display());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// settings.json merge
// ---------------------------------------------------------------------------

/// Build the hooks value that gets merged into settings.json.
fn build_hooks_value() -> Value {
    json!({
        "SessionStart": [{
            "hooks": [{
                "type": "command",
                "command": "bash .claude/hooks/session-start.sh"
            }]
        }],
        "UserPromptSubmit": [{
            "hooks": [{
                "type": "command",
                "command": "bash .claude/hooks/prompt-submit.sh"
            }]
        }],
        "PreCompact": [{
            "hooks": [{
                "type": "command",
                "command": "bash .claude/hooks/pre-compact.sh"
            }]
        }],
        "PostToolUse": [{
            "matcher": "Write|Edit",
            "hooks": [{
                "type": "command",
                "command": "bash .claude/hooks/post-tool-use.sh"
            }]
        }],
        "PreToolUse": [{
            "matcher": "Edit|Write|Read|Grep|Glob|Bash",
            "hooks": [{
                "type": "command",
                "command": "bash .claude/hooks/pre-tool-use.sh"
            }]
        }],
        "SubagentStart": [{
            "hooks": [{
                "type": "command",
                "command": "bash .claude/hooks/subagent-start.sh"
            }]
        }],
        "PostToolUseFailure": [{
            "hooks": [{
                "type": "command",
                "command": "bash .claude/hooks/post-tool-failure.sh"
            }]
        }],
        "Stop": [{
            "hooks": [{
                "type": "command",
                "command": "bash .claude/hooks/stop.sh"
            }]
        }],
        "TaskCompleted": [{
            "hooks": [{
                "type": "command",
                "command": "bash .claude/hooks/task-completed.sh"
            }]
        }],
        "SessionEnd": [{
            "hooks": [{
                "type": "command",
                "command": "bash .claude/hooks/session-end.sh"
            }]
        }]
    })
}

/// Read, merge, and write `.claude/settings.json`.
///
/// If the file exists, its JSON is parsed and the `"hooks"` key is merged
/// (our entries overwrite per-event, but unrelated keys are preserved).
/// If the file doesn't exist, it is created with only the hooks key.
fn merge_settings(settings_path: &Path) -> Result<()> {
    let mut root = read_json_or_empty_object(settings_path)?;
    let hooks_value = build_hooks_value();

    merge_object_key(&mut root, "hooks", hooks_value);

    // Ensure parent directory exists
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let pretty = serde_json::to_string_pretty(&root)?;
    fs::write(settings_path, pretty)?;
    tracing::info!("Merged hooks into {}", settings_path.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// .mcp.json merge
// ---------------------------------------------------------------------------

/// Build the `mcpServers.codegraph` value.
fn build_mcp_server_value(binary_path: &str) -> Value {
    json!({
        "command": binary_path,
        "args": ["serve"],
        "env": {
            "CODEGRAPH_DB": ".codegraph/codegraph.db"
        }
    })
}

/// Read, merge, and write `.mcp.json`.
///
/// The `"mcpServers"` map is extended with a `"codegraph"` entry. Other
/// server entries are left untouched.
fn merge_mcp_config(mcp_path: &Path, binary_path: &str) -> Result<()> {
    let mut root = read_json_or_empty_object(mcp_path)?;
    let server_value = build_mcp_server_value(binary_path);

    // Ensure mcpServers exists as an object, then insert codegraph
    let servers = root
        .as_object_mut()
        .expect("root is always an object")
        .entry("mcpServers")
        .or_insert_with(|| json!({}));

    if let Some(map) = servers.as_object_mut() {
        map.insert("codegraph".to_string(), server_value);
    }

    let pretty = serde_json::to_string_pretty(&root)?;
    fs::write(mcp_path, pretty)?;
    tracing::info!("Merged MCP config into {}", mcp_path.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read a JSON file and parse it as `Value`, returning an empty object if
/// the file doesn't exist or is empty.
fn read_json_or_empty_object(path: &Path) -> Result<Value> {
    match fs::read_to_string(path) {
        Ok(contents) if !contents.trim().is_empty() => {
            let val: Value = serde_json::from_str(&contents)?;
            Ok(val)
        }
        _ => Ok(Value::Object(Map::new())),
    }
}

/// Merge `value` into the top-level `key` of `root`.
///
/// If `root[key]` already exists as an object and `value` is also an object,
/// the entries from `value` are inserted into the existing object (overwriting
/// collisions but preserving non-colliding keys). Otherwise `root[key]` is
/// replaced entirely.
fn merge_object_key(root: &mut Value, key: &str, value: Value) {
    let map = root.as_object_mut().expect("root is always an object");

    match (map.get_mut(key), &value) {
        (Some(existing), Value::Object(new_entries)) if existing.is_object() => {
            let existing_map = existing.as_object_mut().unwrap();
            for (k, v) in new_entries {
                existing_map.insert(k.clone(), v.clone());
            }
        }
        _ => {
            map.insert(key.to_string(), value);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -- Shell script tests ------------------------------------------------

    #[test]
    fn shell_scripts_are_created_with_correct_content() {
        let tmp = TempDir::new().unwrap();
        let hooks_dir = tmp.path().join(".claude").join("hooks");

        write_shell_scripts(&hooks_dir, "codegraph").unwrap();

        for hook in HOOK_SCRIPTS {
            let path = hooks_dir.join(hook.filename);
            assert!(path.exists(), "missing: {}", hook.filename);

            let content = fs::read_to_string(&path).unwrap();
            assert!(content.starts_with("#!/usr/bin/env bash"));
            assert!(content.contains(hook.subcommand));
            assert!(content.contains("codegraph"));

            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(
                mode & 0o777,
                0o755,
                "wrong permissions on {}",
                hook.filename
            );
        }
    }

    #[test]
    fn shell_scripts_use_custom_binary_path() {
        let tmp = TempDir::new().unwrap();
        let hooks_dir = tmp.path().join("hooks");

        write_shell_scripts(&hooks_dir, "/opt/bin/codegraph").unwrap();

        let content = fs::read_to_string(hooks_dir.join("session-start.sh")).unwrap();
        assert!(content.contains("/opt/bin/codegraph"));
    }

    // -- merge_object_key tests -------------------------------------------

    #[test]
    fn merge_into_empty_object() {
        let mut root = json!({});
        merge_object_key(&mut root, "hooks", json!({"A": 1}));
        assert_eq!(root, json!({"hooks": {"A": 1}}));
    }

    #[test]
    fn merge_preserves_existing_keys() {
        let mut root = json!({"hooks": {"Existing": true}, "other": 42});
        merge_object_key(&mut root, "hooks", json!({"New": false}));

        assert_eq!(root["hooks"]["Existing"], json!(true));
        assert_eq!(root["hooks"]["New"], json!(false));
        assert_eq!(root["other"], json!(42));
    }

    #[test]
    fn merge_overwrites_colliding_keys() {
        let mut root = json!({"hooks": {"A": "old"}});
        merge_object_key(&mut root, "hooks", json!({"A": "new"}));

        assert_eq!(root["hooks"]["A"], json!("new"));
    }

    #[test]
    fn merge_replaces_non_object_value() {
        let mut root = json!({"hooks": "not an object"});
        merge_object_key(&mut root, "hooks", json!({"A": 1}));

        assert_eq!(root["hooks"], json!({"A": 1}));
    }

    // -- settings.json merge tests ----------------------------------------

    #[test]
    fn settings_created_from_scratch() {
        let tmp = TempDir::new().unwrap();
        let settings = tmp.path().join(".claude").join("settings.json");

        merge_settings(&settings).unwrap();

        let parsed: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
        assert!(parsed["hooks"]["SessionStart"].is_array());
        assert!(parsed["hooks"]["UserPromptSubmit"].is_array());
        assert!(parsed["hooks"]["PreCompact"].is_array());
        assert!(parsed["hooks"]["PostToolUse"].is_array());
        assert!(parsed["hooks"]["PreToolUse"].is_array());
        assert!(parsed["hooks"]["SubagentStart"].is_array());
        assert!(parsed["hooks"]["PostToolUseFailure"].is_array());
    }

    #[test]
    fn settings_preserves_unrelated_keys() {
        let tmp = TempDir::new().unwrap();
        let claude_dir = tmp.path().join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        let settings = claude_dir.join("settings.json");
        fs::write(
            &settings,
            serde_json::to_string_pretty(&json!({
                "theme": "dark",
                "hooks": {
                    "Custom": [{"hooks": [{"type": "command", "command": "echo hi"}]}]
                }
            }))
            .unwrap(),
        )
        .unwrap();

        merge_settings(&settings).unwrap();

        let parsed: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(parsed["theme"], json!("dark"));
        assert!(
            parsed["hooks"]["Custom"].is_array(),
            "Custom hook preserved"
        );
        assert!(
            parsed["hooks"]["SessionStart"].is_array(),
            "SessionStart added"
        );
    }

    // -- .mcp.json merge tests --------------------------------------------

    #[test]
    fn mcp_config_created_from_scratch() {
        let tmp = TempDir::new().unwrap();
        let mcp = tmp.path().join(".mcp.json");

        merge_mcp_config(&mcp, "codegraph").unwrap();

        let parsed: Value = serde_json::from_str(&fs::read_to_string(&mcp).unwrap()).unwrap();
        let cg = &parsed["mcpServers"]["codegraph"];
        assert_eq!(cg["command"], json!("codegraph"));
        assert_eq!(cg["args"], json!(["serve"]));
        assert_eq!(cg["env"]["CODEGRAPH_DB"], json!(".codegraph/codegraph.db"));
    }

    #[test]
    fn mcp_config_preserves_other_servers() {
        let tmp = TempDir::new().unwrap();
        let mcp = tmp.path().join(".mcp.json");
        fs::write(
            &mcp,
            serde_json::to_string_pretty(&json!({
                "mcpServers": {
                    "other-tool": {
                        "command": "other-bin",
                        "args": ["run"]
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        merge_mcp_config(&mcp, "codegraph").unwrap();

        let parsed: Value = serde_json::from_str(&fs::read_to_string(&mcp).unwrap()).unwrap();
        assert!(
            parsed["mcpServers"]["other-tool"].is_object(),
            "other-tool preserved"
        );
        assert!(
            parsed["mcpServers"]["codegraph"].is_object(),
            "codegraph added"
        );
    }

    // -- Full integration test --------------------------------------------

    #[test]
    fn install_hooks_end_to_end() {
        let tmp = TempDir::new().unwrap();

        install_hooks(tmp.path(), "codegraph").unwrap();

        // Shell scripts exist
        let hooks_dir = tmp.path().join(".claude").join("hooks");
        assert!(hooks_dir.join("session-start.sh").exists());
        assert!(hooks_dir.join("prompt-submit.sh").exists());
        assert!(hooks_dir.join("pre-compact.sh").exists());
        assert!(hooks_dir.join("post-tool-use.sh").exists());
        assert!(hooks_dir.join("pre-tool-use.sh").exists());
        assert!(hooks_dir.join("subagent-start.sh").exists());
        assert!(hooks_dir.join("post-tool-failure.sh").exists());
        assert!(hooks_dir.join("stop.sh").exists());
        assert!(hooks_dir.join("task-completed.sh").exists());
        assert!(hooks_dir.join("session-end.sh").exists());

        // settings.json has hooks
        let settings: Value = serde_json::from_str(
            &fs::read_to_string(tmp.path().join(".claude").join("settings.json")).unwrap(),
        )
        .unwrap();
        assert!(settings["hooks"]["SessionStart"].is_array());
        assert!(settings["hooks"]["PostToolUse"][0]["matcher"] == "Write|Edit");
        assert!(settings["hooks"]["PreToolUse"].is_array());
        assert!(settings["hooks"]["SubagentStart"].is_array());
        assert!(settings["hooks"]["PostToolUseFailure"].is_array());
        assert!(settings["hooks"]["Stop"].is_array());
        assert!(settings["hooks"]["TaskCompleted"].is_array());
        assert!(settings["hooks"]["SessionEnd"].is_array());

        // .mcp.json has codegraph server
        let mcp: Value =
            serde_json::from_str(&fs::read_to_string(tmp.path().join(".mcp.json")).unwrap())
                .unwrap();
        assert_eq!(mcp["mcpServers"]["codegraph"]["command"], "codegraph");
    }

    #[test]
    fn install_hooks_is_idempotent() {
        let tmp = TempDir::new().unwrap();

        install_hooks(tmp.path(), "codegraph").unwrap();
        install_hooks(tmp.path(), "codegraph").unwrap();

        let settings: Value = serde_json::from_str(
            &fs::read_to_string(tmp.path().join(".claude").join("settings.json")).unwrap(),
        )
        .unwrap();

        // SessionStart should still be an array with exactly one entry (not duplicated)
        assert_eq!(
            settings["hooks"]["SessionStart"].as_array().unwrap().len(),
            1
        );
    }

    // -- Additional hooks tests (Phase 18D) -----------------------------------

    #[test]
    fn all_hook_scripts_are_generated() {
        assert_eq!(
            HOOK_SCRIPTS.len(),
            10,
            "Should have exactly 10 hook scripts"
        );
    }

    #[test]
    fn hook_scripts_have_unique_filenames() {
        let filenames: Vec<&str> = HOOK_SCRIPTS.iter().map(|h| h.filename).collect();
        let unique: std::collections::HashSet<&str> = filenames.iter().copied().collect();
        assert_eq!(
            filenames.len(),
            unique.len(),
            "Hook script filenames should be unique"
        );
    }

    #[test]
    fn hook_scripts_have_unique_subcommands() {
        let subs: Vec<&str> = HOOK_SCRIPTS.iter().map(|h| h.subcommand).collect();
        let unique: std::collections::HashSet<&str> = subs.iter().copied().collect();
        assert_eq!(
            subs.len(),
            unique.len(),
            "Hook script subcommands should be unique"
        );
    }

    #[test]
    fn render_script_contains_shebang() {
        let hook = &HOOK_SCRIPTS[0];
        let script = render_script(hook, "codegraph");
        assert!(script.starts_with("#!/usr/bin/env bash"));
    }

    #[test]
    fn render_script_contains_fallback() {
        let hook = &HOOK_SCRIPTS[0];
        let script = render_script(hook, "codegraph");
        assert!(
            script.contains(r#"{"continue":true}"#),
            "Script should have fallback JSON"
        );
    }

    #[test]
    fn render_script_uses_env_variable() {
        let hook = &HOOK_SCRIPTS[0];
        let script = render_script(hook, "/custom/path");
        assert!(
            script.contains("CODEGRAPH_BIN"),
            "Script should reference CODEGRAPH_BIN env var"
        );
        assert!(
            script.contains("/custom/path"),
            "Script should use the provided binary path as default"
        );
    }

    #[test]
    fn build_hooks_value_has_all_events() {
        let hooks = build_hooks_value();
        let expected_events = [
            "SessionStart",
            "UserPromptSubmit",
            "PreCompact",
            "PostToolUse",
            "PreToolUse",
            "SubagentStart",
            "PostToolUseFailure",
            "Stop",
            "TaskCompleted",
            "SessionEnd",
        ];
        for event in &expected_events {
            assert!(
                hooks[event].is_array(),
                "hooks value should contain '{}' as array",
                event
            );
        }
    }

    #[test]
    fn build_hooks_value_post_tool_use_has_matcher() {
        let hooks = build_hooks_value();
        assert_eq!(
            hooks["PostToolUse"][0]["matcher"],
            json!("Write|Edit"),
            "PostToolUse should match Write|Edit"
        );
    }

    #[test]
    fn build_hooks_value_pre_tool_use_has_matcher() {
        let hooks = build_hooks_value();
        assert_eq!(
            hooks["PreToolUse"][0]["matcher"],
            json!("Edit|Write|Read|Grep|Glob|Bash"),
            "PreToolUse should match tool types"
        );
    }

    #[test]
    fn build_mcp_server_value_structure() {
        let mcp = build_mcp_server_value("/usr/bin/codegraph");
        assert_eq!(mcp["command"], json!("/usr/bin/codegraph"));
        assert_eq!(mcp["args"], json!(["serve"]));
        assert!(mcp["env"].is_object());
        assert_eq!(mcp["env"]["CODEGRAPH_DB"], json!(".codegraph/codegraph.db"));
    }

    #[test]
    fn merge_object_key_creates_new_key() {
        let mut root = json!({"existing": 42});
        merge_object_key(&mut root, "new_key", json!({"a": 1}));
        assert_eq!(root["new_key"]["a"], json!(1));
        assert_eq!(root["existing"], json!(42));
    }

    #[test]
    fn merge_object_key_deep_merge() {
        let mut root = json!({
            "hooks": {
                "A": {"x": 1},
                "B": {"y": 2}
            }
        });
        merge_object_key(&mut root, "hooks", json!({"C": {"z": 3}}));
        assert_eq!(root["hooks"]["A"]["x"], json!(1), "A preserved");
        assert_eq!(root["hooks"]["B"]["y"], json!(2), "B preserved");
        assert_eq!(root["hooks"]["C"]["z"], json!(3), "C added");
    }

    #[test]
    fn read_json_or_empty_object_with_missing_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nonexistent.json");
        let val = read_json_or_empty_object(&path).unwrap();
        assert!(val.is_object());
        assert!(val.as_object().unwrap().is_empty());
    }

    #[test]
    fn read_json_or_empty_object_with_empty_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("empty.json");
        fs::write(&path, "").unwrap();
        let val = read_json_or_empty_object(&path).unwrap();
        assert!(val.is_object());
        assert!(val.as_object().unwrap().is_empty());
    }

    #[test]
    fn read_json_or_empty_object_with_whitespace_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("whitespace.json");
        fs::write(&path, "   \n\t  ").unwrap();
        let val = read_json_or_empty_object(&path).unwrap();
        assert!(val.is_object());
        assert!(val.as_object().unwrap().is_empty());
    }

    #[test]
    fn read_json_or_empty_object_with_valid_json() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("valid.json");
        fs::write(&path, r#"{"key": "value"}"#).unwrap();
        let val = read_json_or_empty_object(&path).unwrap();
        assert_eq!(val["key"], json!("value"));
    }

    #[test]
    fn settings_includes_stop_and_task_completed() {
        let tmp = TempDir::new().unwrap();
        let settings_path = tmp.path().join(".claude").join("settings.json");
        merge_settings(&settings_path).unwrap();

        let parsed: Value =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert!(parsed["hooks"]["Stop"].is_array(), "Stop hook missing");
        assert!(
            parsed["hooks"]["TaskCompleted"].is_array(),
            "TaskCompleted hook missing"
        );
        assert!(
            parsed["hooks"]["SessionEnd"].is_array(),
            "SessionEnd hook missing"
        );
    }

    #[test]
    fn mcp_config_has_correct_env() {
        let tmp = TempDir::new().unwrap();
        let mcp_path = tmp.path().join(".mcp.json");
        merge_mcp_config(&mcp_path, "codegraph").unwrap();

        let parsed: Value = serde_json::from_str(&fs::read_to_string(&mcp_path).unwrap()).unwrap();
        let env = &parsed["mcpServers"]["codegraph"]["env"];
        assert_eq!(env["CODEGRAPH_DB"], json!(".codegraph/codegraph.db"));
    }

    #[test]
    fn shell_script_stderr_redirect() {
        for hook in HOOK_SCRIPTS {
            let script = render_script(hook, "codegraph");
            assert!(
                script.contains("2>/dev/null"),
                "{} should redirect stderr",
                hook.filename
            );
        }
    }

    #[test]
    fn shell_scripts_all_end_with_sh() {
        for hook in HOOK_SCRIPTS {
            assert!(
                hook.filename.ends_with(".sh"),
                "Hook filename '{}' should end with .sh",
                hook.filename
            );
        }
    }

    #[test]
    fn shell_scripts_subcommands_start_with_hook() {
        for hook in HOOK_SCRIPTS {
            assert!(
                hook.subcommand.starts_with("hook-"),
                "Subcommand '{}' should start with 'hook-'",
                hook.subcommand
            );
        }
    }
}

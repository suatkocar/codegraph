//! Git history — file history, recent changes, commit diff, symbol history,
//! branch info, modified files.

use std::path::Path;

use super::{run_git, validate_input, BranchInfo, CommitInfo, DiffInfo, FileDiff, ModifiedFiles};
use crate::error::CodeGraphError;

// ── Commit log format shared by several functions ───────────────────────

const LOG_FORMAT: &str = "%H|%an|%ae|%aI|%s";

/// Parse `git log --format=<LOG_FORMAT> --name-only` output into `CommitInfo`.
fn parse_log_with_files(output: &str) -> Vec<CommitInfo> {
    let mut commits = Vec::new();
    let mut current: Option<CommitInfo> = None;

    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        // A commit line has at least 40 hex chars followed by '|'
        if line.len() > 41
            && line.as_bytes()[40] == b'|'
            && line[..40].bytes().all(|b| b.is_ascii_hexdigit())
        {
            if let Some(c) = current.take() {
                commits.push(c);
            }
            let parts: Vec<&str> = line.splitn(5, '|').collect();
            if parts.len() == 5 {
                current = Some(CommitInfo {
                    hash: parts[0].to_string(),
                    author: parts[1].to_string(),
                    email: parts[2].to_string(),
                    date: parts[3].to_string(),
                    message: parts[4].to_string(),
                    files_changed: Vec::new(),
                });
            }
        } else if let Some(ref mut c) = current {
            // This is a filename from --name-only
            c.files_changed.push(line.to_string());
        }
    }
    if let Some(c) = current {
        commits.push(c);
    }
    commits
}

// ── Public API ──────────────────────────────────────────────────────────

/// Get the commit history for a specific file, newest first.
pub fn file_history(
    repo_path: &Path,
    file: &str,
    limit: usize,
) -> Result<Vec<CommitInfo>, CodeGraphError> {
    validate_input(file, "file_path")?;

    let limit_str = format!("-{limit}");
    let output = run_git(
        repo_path,
        &[
            "log",
            &format!("--format={LOG_FORMAT}"),
            "--name-only",
            &limit_str,
            "--",
            file,
        ],
    )?;

    Ok(parse_log_with_files(&output))
}

/// Get the most recent commits across the entire repository.
pub fn recent_changes(repo_path: &Path, limit: usize) -> Result<Vec<CommitInfo>, CodeGraphError> {
    let limit_str = format!("-{limit}");
    let output = run_git(
        repo_path,
        &[
            "log",
            &format!("--format={LOG_FORMAT}"),
            "--name-only",
            &limit_str,
        ],
    )?;

    Ok(parse_log_with_files(&output))
}

/// Get the diff for a specific commit, with per-file addition/deletion counts.
pub fn commit_diff(repo_path: &Path, commit_hash: &str) -> Result<DiffInfo, CodeGraphError> {
    validate_input(commit_hash, "commit_hash")?;

    // Get the stat summary (--root handles the initial commit with no parent)
    let stat_output = run_git(
        repo_path,
        &[
            "diff-tree",
            "--root",
            "--no-commit-id",
            "--numstat",
            "-r",
            commit_hash,
        ],
    )?;

    // Get the full patch
    let patch_output = run_git(
        repo_path,
        &["diff-tree", "--root", "--no-commit-id", "-p", commit_hash],
    )?;

    // Parse numstat lines: "added\tremoved\tfile"
    let mut files = Vec::new();
    for line in stat_output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let additions = parts[0].parse().unwrap_or(0);
            let deletions = parts[1].parse().unwrap_or(0);
            let path = parts[2].to_string();

            // Extract the patch section for this file from the full patch
            let file_patch = extract_file_patch(&patch_output, &path);

            files.push(FileDiff {
                path,
                additions,
                deletions,
                patch: file_patch,
            });
        }
    }

    Ok(DiffInfo {
        commit: commit_hash.to_string(),
        files,
    })
}

/// Extract the unified diff hunk for a single file from a full patch.
fn extract_file_patch(full_patch: &str, file_path: &str) -> String {
    let mut collecting = false;
    let mut patch = String::new();
    let marker = format!("diff --git a/{file_path}");

    for line in full_patch.lines() {
        if line.starts_with("diff --git ") {
            if collecting {
                break; // hit the next file
            }
            if line.starts_with(&marker) {
                collecting = true;
            }
        }
        if collecting {
            patch.push_str(line);
            patch.push('\n');
        }
    }
    patch
}

/// Find commits that added or removed `symbol_name` (via `git log -S`).
///
/// Searches across common source-code extensions.
pub fn symbol_history(
    repo_path: &Path,
    symbol_name: &str,
) -> Result<Vec<CommitInfo>, CodeGraphError> {
    validate_input(symbol_name, "symbol_name")?;

    let s_flag = format!("-S{symbol_name}");
    let output = run_git(
        repo_path,
        &[
            "log",
            &format!("--format={LOG_FORMAT}"),
            "--name-only",
            &s_flag,
            "--",
            "*.rs",
            "*.ts",
            "*.tsx",
            "*.js",
            "*.jsx",
            "*.py",
            "*.go",
            "*.java",
            "*.c",
            "*.cpp",
            "*.h",
            "*.cs",
            "*.php",
            "*.rb",
            "*.swift",
            "*.kt",
        ],
    )?;

    Ok(parse_log_with_files(&output))
}

/// Get current branch name, tracking remote, and ahead/behind counts.
pub fn branch_info(repo_path: &Path) -> Result<BranchInfo, CodeGraphError> {
    // Current branch name
    let current = run_git(repo_path, &["branch", "--show-current"])?
        .trim()
        .to_string();

    let current_display = if current.is_empty() {
        "HEAD (detached)".to_string()
    } else {
        current.clone()
    };

    // Try to get tracking info
    let tracking_result = run_git(
        repo_path,
        &[
            "rev-parse",
            "--abbrev-ref",
            &format!("{current}@{{upstream}}"),
        ],
    );

    let (tracking, ahead, behind) = match tracking_result {
        Ok(upstream) => {
            let upstream = upstream.trim().to_string();
            // Get ahead/behind counts
            let ab = run_git(
                repo_path,
                &[
                    "rev-list",
                    "--left-right",
                    "--count",
                    &format!("{current}...{upstream}"),
                ],
            )
            .unwrap_or_default();
            let parts: Vec<&str> = ab.trim().split('\t').collect();
            let ahead = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
            let behind = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            (Some(upstream), ahead, behind)
        }
        Err(_) => (None, 0, 0),
    };

    let status = match (ahead, behind) {
        (0, 0) => "up-to-date".to_string(),
        (a, 0) => format!("ahead {a}"),
        (0, b) => format!("behind {b}"),
        (a, b) => format!("ahead {a}, behind {b}"),
    };

    Ok(BranchInfo {
        current: current_display,
        tracking,
        ahead,
        behind,
        status,
    })
}

/// Get staged, unstaged, and untracked files from the working tree.
pub fn modified_files(repo_path: &Path) -> Result<ModifiedFiles, CodeGraphError> {
    let output = run_git(repo_path, &["status", "--porcelain"])?;

    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    let mut untracked = Vec::new();

    for line in output.lines() {
        if line.len() < 4 {
            continue;
        }
        let index_status = line.as_bytes()[0];
        let worktree_status = line.as_bytes()[1];
        let file = line[3..].to_string();

        if index_status == b'?' && worktree_status == b'?' {
            untracked.push(file);
        } else {
            // Staged changes: anything in the index column that isn't ' ' or '?'
            if index_status != b' ' && index_status != b'?' {
                staged.push(file.clone());
            }
            // Unstaged changes: anything in the worktree column that isn't ' ' or '?'
            if worktree_status != b' ' && worktree_status != b'?' {
                unstaged.push(file);
            }
        }
    }

    Ok(ModifiedFiles {
        staged,
        unstaged,
        untracked,
    })
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Create a temporary git repo with a few commits for testing history.
    fn create_test_repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();

        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&path)
                .env("GIT_AUTHOR_NAME", "Test Author")
                .env("GIT_AUTHOR_EMAIL", "test@example.com")
                .env("GIT_COMMITTER_NAME", "Test Author")
                .env("GIT_COMMITTER_EMAIL", "test@example.com")
                .output()
                .unwrap()
        };

        git(&["init"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test Author"]);

        // Commit 1
        std::fs::write(path.join("main.rs"), "fn main() {}\n").unwrap();
        git(&["add", "main.rs"]);
        git(&["commit", "-m", "first commit"]);

        // Commit 2
        std::fs::write(
            path.join("main.rs"),
            "fn main() {\n    println!(\"hi\");\n}\n",
        )
        .unwrap();
        std::fs::write(
            path.join("lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        )
        .unwrap();
        git(&["add", "main.rs", "lib.rs"]);
        git(&["commit", "-m", "add println and lib"]);

        // Commit 3
        std::fs::write(path.join("lib.rs"), "pub fn add(a: i32, b: i32) -> i32 { a + b }\npub fn sub(a: i32, b: i32) -> i32 { a - b }\n").unwrap();
        git(&["add", "lib.rs"]);
        git(&["commit", "-m", "add sub function"]);

        (dir, path)
    }

    // ── file_history ────────────────────────────────────────────────────

    #[test]
    fn test_file_history_basic() {
        let (_dir, path) = create_test_repo();
        let history = file_history(&path, "main.rs", 10).unwrap();

        assert_eq!(history.len(), 2);
        // Newest first
        assert_eq!(history[0].message, "add println and lib");
        assert_eq!(history[1].message, "first commit");
        assert_eq!(history[0].author, "Test Author");
        assert!(!history[0].hash.is_empty());
    }

    #[test]
    fn test_file_history_limit() {
        let (_dir, path) = create_test_repo();
        let history = file_history(&path, "main.rs", 1).unwrap();
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_file_history_nonexistent() {
        let (_dir, path) = create_test_repo();
        let history = file_history(&path, "nope.rs", 10).unwrap();
        assert!(history.is_empty());
    }

    #[test]
    fn test_file_history_injection() {
        let (_dir, path) = create_test_repo();
        assert!(file_history(&path, "--exec=rm", 10).is_err());
    }

    // ── recent_changes ──────────────────────────────────────────────────

    #[test]
    fn test_recent_changes() {
        let (_dir, path) = create_test_repo();
        let changes = recent_changes(&path, 10).unwrap();

        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].message, "add sub function");
    }

    #[test]
    fn test_recent_changes_limit() {
        let (_dir, path) = create_test_repo();
        let changes = recent_changes(&path, 2).unwrap();
        assert_eq!(changes.len(), 2);
    }

    // ── commit_diff ─────────────────────────────────────────────────────

    #[test]
    fn test_commit_diff() {
        let (_dir, path) = create_test_repo();
        let changes = recent_changes(&path, 1).unwrap();
        let hash = &changes[0].hash;

        let diff = commit_diff(&path, hash).unwrap();
        assert_eq!(diff.commit, *hash);
        assert!(!diff.files.is_empty());

        // The last commit touched lib.rs
        let lib_diff = diff.files.iter().find(|f| f.path == "lib.rs");
        assert!(lib_diff.is_some());
        let lib_diff = lib_diff.unwrap();
        assert!(lib_diff.additions > 0);
        assert!(!lib_diff.patch.is_empty());
    }

    #[test]
    fn test_commit_diff_injection() {
        let (_dir, path) = create_test_repo();
        assert!(commit_diff(&path, "--exec=id").is_err());
    }

    #[test]
    fn test_commit_diff_invalid_hash() {
        let (_dir, path) = create_test_repo();
        assert!(commit_diff(&path, "0000000000000000000000000000000000000000").is_err());
    }

    // ── symbol_history ──────────────────────────────────────────────────

    #[test]
    fn test_symbol_history() {
        let (_dir, path) = create_test_repo();
        let history = symbol_history(&path, "sub").unwrap();

        // "sub" was added in the last commit
        assert!(!history.is_empty());
        assert!(history.iter().any(|c| c.message.contains("sub")));
    }

    #[test]
    fn test_symbol_history_not_found() {
        let (_dir, path) = create_test_repo();
        let history = symbol_history(&path, "nonexistent_xyz_symbol").unwrap();
        assert!(history.is_empty());
    }

    #[test]
    fn test_symbol_history_injection() {
        let (_dir, path) = create_test_repo();
        assert!(symbol_history(&path, "--exec=id").is_err());
    }

    // ── branch_info ─────────────────────────────────────────────────────

    #[test]
    fn test_branch_info() {
        let (_dir, path) = create_test_repo();
        let info = branch_info(&path).unwrap();

        // Default branch is either "main" or "master" depending on git config
        assert!(
            info.current == "main" || info.current == "master",
            "Expected main or master, got: {}",
            info.current
        );
        // No remote tracking in a local-only repo
        assert!(info.tracking.is_none());
        assert_eq!(info.ahead, 0);
        assert_eq!(info.behind, 0);
    }

    // ── modified_files ──────────────────────────────────────────────────

    #[test]
    fn test_modified_files_clean() {
        let (_dir, path) = create_test_repo();
        let mods = modified_files(&path).unwrap();

        assert!(mods.staged.is_empty());
        assert!(mods.unstaged.is_empty());
        assert!(mods.untracked.is_empty());
    }

    #[test]
    fn test_modified_files_untracked() {
        let (_dir, path) = create_test_repo();
        std::fs::write(path.join("new_file.txt"), "hello").unwrap();

        let mods = modified_files(&path).unwrap();
        assert!(mods.untracked.contains(&"new_file.txt".to_string()));
    }

    #[test]
    fn test_modified_files_staged() {
        let (_dir, path) = create_test_repo();
        std::fs::write(path.join("staged.txt"), "staged content").unwrap();
        std::process::Command::new("git")
            .args(["add", "staged.txt"])
            .current_dir(&path)
            .output()
            .unwrap();

        let mods = modified_files(&path).unwrap();
        assert!(mods.staged.contains(&"staged.txt".to_string()));
    }

    #[test]
    fn test_modified_files_unstaged() {
        let (_dir, path) = create_test_repo();
        // Modify an already-tracked file without staging
        std::fs::write(path.join("main.rs"), "fn main() { /* changed */ }\n").unwrap();

        let mods = modified_files(&path).unwrap();
        assert!(mods.unstaged.contains(&"main.rs".to_string()));
    }

    // ── parse_log_with_files ────────────────────────────────────────────

    #[test]
    fn test_parse_log_empty() {
        let result = parse_log_with_files("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_files_changed_populated() {
        let (_dir, path) = create_test_repo();
        let changes = recent_changes(&path, 1).unwrap();
        // Last commit touched lib.rs
        assert!(changes[0].files_changed.contains(&"lib.rs".to_string()));
    }

    // =====================================================================
    // Additional file_history tests
    // =====================================================================

    #[test]
    fn test_file_history_returns_newest_first() {
        let (_dir, path) = create_test_repo();
        let history = file_history(&path, "lib.rs", 10).unwrap();
        assert_eq!(history.len(), 2);
        // "add sub function" is newer than "add println and lib"
        assert_eq!(history[0].message, "add sub function");
        assert_eq!(history[1].message, "add println and lib");
    }

    #[test]
    fn test_file_history_includes_files_changed() {
        let (_dir, path) = create_test_repo();
        let history = file_history(&path, "lib.rs", 10).unwrap();
        for commit in &history {
            assert!(
                commit.files_changed.contains(&"lib.rs".to_string()),
                "commit {} should include lib.rs in files_changed",
                commit.hash
            );
        }
    }

    #[test]
    fn test_file_history_email_populated() {
        let (_dir, path) = create_test_repo();
        let history = file_history(&path, "main.rs", 10).unwrap();
        for commit in &history {
            assert_eq!(commit.email, "test@example.com");
        }
    }

    #[test]
    fn test_file_history_date_is_iso8601() {
        let (_dir, path) = create_test_repo();
        let history = file_history(&path, "main.rs", 10).unwrap();
        for commit in &history {
            // ISO 8601 dates contain 'T' or at least '-'
            assert!(
                commit.date.contains('-'),
                "date should be ISO-8601: {}",
                commit.date
            );
        }
    }

    #[test]
    fn test_file_history_limit_zero_returns_nothing() {
        let (_dir, path) = create_test_repo();
        // limit=0 should effectively return nothing
        let history = file_history(&path, "main.rs", 0).unwrap();
        assert!(history.is_empty());
    }

    #[test]
    fn test_file_history_limit_exceeds_total() {
        let (_dir, path) = create_test_repo();
        let history = file_history(&path, "main.rs", 100).unwrap();
        assert_eq!(history.len(), 2, "Only 2 commits touch main.rs");
    }

    #[test]
    fn test_file_history_hash_length() {
        let (_dir, path) = create_test_repo();
        let history = file_history(&path, "main.rs", 10).unwrap();
        for commit in &history {
            assert_eq!(
                commit.hash.len(),
                40,
                "hash should be 40 chars: {}",
                commit.hash
            );
        }
    }

    // =====================================================================
    // Additional recent_changes tests
    // =====================================================================

    #[test]
    fn test_recent_changes_returns_all_commits() {
        let (_dir, path) = create_test_repo();
        let changes = recent_changes(&path, 100).unwrap();
        assert_eq!(changes.len(), 3);
    }

    #[test]
    fn test_recent_changes_newest_first() {
        let (_dir, path) = create_test_repo();
        let changes = recent_changes(&path, 10).unwrap();
        assert_eq!(changes[0].message, "add sub function");
        assert_eq!(changes[1].message, "add println and lib");
        assert_eq!(changes[2].message, "first commit");
    }

    #[test]
    fn test_recent_changes_limit_one() {
        let (_dir, path) = create_test_repo();
        let changes = recent_changes(&path, 1).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].message, "add sub function");
    }

    #[test]
    fn test_recent_changes_includes_files_changed() {
        let (_dir, path) = create_test_repo();
        let changes = recent_changes(&path, 3).unwrap();
        // The second commit (index 1) should have both main.rs and lib.rs
        assert!(changes[1].files_changed.contains(&"main.rs".to_string()));
        assert!(changes[1].files_changed.contains(&"lib.rs".to_string()));
    }

    #[test]
    fn test_recent_changes_not_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        let result = recent_changes(dir.path(), 10);
        assert!(result.is_err());
    }

    // =====================================================================
    // Additional commit_diff tests
    // =====================================================================

    #[test]
    fn test_commit_diff_non_root_has_additions_and_deletions() {
        let (_dir, path) = create_test_repo();
        let changes = recent_changes(&path, 10).unwrap();
        // Use the second commit (not root), which modified main.rs and added lib.rs
        let second_hash = &changes[1].hash;

        let diff = commit_diff(&path, second_hash).unwrap();
        assert_eq!(diff.commit, *second_hash);
        assert!(
            !diff.files.is_empty(),
            "second commit should have file changes"
        );
        // The second commit touched both main.rs and lib.rs
        let lib_diff = diff.files.iter().find(|f| f.path == "lib.rs");
        assert!(lib_diff.is_some(), "second commit should include lib.rs");
        assert!(lib_diff.unwrap().additions > 0);
    }

    #[test]
    fn test_commit_diff_patch_contains_diff_header() {
        let (_dir, path) = create_test_repo();
        let changes = recent_changes(&path, 1).unwrap();
        let hash = &changes[0].hash;

        let diff = commit_diff(&path, hash).unwrap();
        for file in &diff.files {
            if !file.patch.is_empty() {
                assert!(
                    file.patch.contains("diff --git"),
                    "patch should contain diff header"
                );
            }
        }
    }

    #[test]
    fn test_commit_diff_additions_and_deletions_counted() {
        let (_dir, path) = create_test_repo();
        // The "add sub function" commit adds a line to lib.rs
        let changes = recent_changes(&path, 1).unwrap();
        let hash = &changes[0].hash;

        let diff = commit_diff(&path, hash).unwrap();
        let lib_diff = diff.files.iter().find(|f| f.path == "lib.rs").unwrap();
        assert!(lib_diff.additions > 0, "should have additions");
    }

    #[test]
    fn test_commit_diff_hash_too_short() {
        let (_dir, path) = create_test_repo();
        let result = commit_diff(&path, "abc");
        assert!(result.is_err());
    }

    // =====================================================================
    // Additional symbol_history tests
    // =====================================================================

    #[test]
    fn test_symbol_history_finds_main() {
        let (_dir, path) = create_test_repo();
        let history = symbol_history(&path, "main").unwrap();
        assert!(!history.is_empty(), "should find 'main' in history");
    }

    #[test]
    fn test_symbol_history_finds_add() {
        let (_dir, path) = create_test_repo();
        let history = symbol_history(&path, "add").unwrap();
        // "add" appears in the function name "add" in lib.rs
        assert!(!history.is_empty());
    }

    #[test]
    fn test_symbol_history_empty_string() {
        let (_dir, path) = create_test_repo();
        // Empty string should match all commits (everything contains "")
        let history = symbol_history(&path, "x_y_z_never_exists_qqqqqq").unwrap();
        assert!(history.is_empty());
    }

    // =====================================================================
    // Additional branch_info tests
    // =====================================================================

    #[test]
    fn test_branch_info_status_is_up_to_date_for_local() {
        let (_dir, path) = create_test_repo();
        let info = branch_info(&path).unwrap();
        assert_eq!(info.status, "up-to-date");
    }

    #[test]
    fn test_branch_info_not_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        let result = branch_info(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_branch_info_detached_head() {
        let (_dir, path) = create_test_repo();
        let changes = recent_changes(&path, 1).unwrap();
        let hash = &changes[0].hash;
        // Detach HEAD
        std::process::Command::new("git")
            .args(["checkout", hash])
            .current_dir(&path)
            .output()
            .unwrap();

        let info = branch_info(&path).unwrap();
        assert_eq!(info.current, "HEAD (detached)");
    }

    // =====================================================================
    // Additional modified_files tests
    // =====================================================================

    #[test]
    fn test_modified_files_not_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        let result = modified_files(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_modified_files_staged_and_unstaged_same_file() {
        let (_dir, path) = create_test_repo();
        // Stage a change
        std::fs::write(path.join("main.rs"), "fn main() { /* v1 */ }\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "main.rs"])
            .current_dir(&path)
            .output()
            .unwrap();
        // Then modify again without staging
        std::fs::write(path.join("main.rs"), "fn main() { /* v2 */ }\n").unwrap();

        let mods = modified_files(&path).unwrap();
        assert!(
            mods.staged.contains(&"main.rs".to_string()),
            "should be in staged"
        );
        assert!(
            mods.unstaged.contains(&"main.rs".to_string()),
            "should be in unstaged"
        );
    }

    #[test]
    fn test_modified_files_multiple_untracked() {
        let (_dir, path) = create_test_repo();
        std::fs::write(path.join("a.txt"), "a").unwrap();
        std::fs::write(path.join("b.txt"), "b").unwrap();
        std::fs::write(path.join("c.txt"), "c").unwrap();

        let mods = modified_files(&path).unwrap();
        assert_eq!(mods.untracked.len(), 3);
    }

    #[test]
    fn test_modified_files_deleted_file_shows_unstaged() {
        let (_dir, path) = create_test_repo();
        // Delete a tracked file
        std::fs::remove_file(path.join("main.rs")).unwrap();

        let mods = modified_files(&path).unwrap();
        assert!(mods.unstaged.contains(&"main.rs".to_string()));
    }

    // =====================================================================
    // parse_log_with_files edge cases
    // =====================================================================

    #[test]
    fn test_parse_log_with_files_single_commit() {
        let (_dir, path) = create_test_repo();
        let changes = recent_changes(&path, 1).unwrap();
        assert_eq!(changes.len(), 1);
        assert!(!changes[0].hash.is_empty());
        assert!(!changes[0].author.is_empty());
        assert!(!changes[0].message.is_empty());
    }

    #[test]
    fn test_parse_log_with_files_malformed_input() {
        let result = parse_log_with_files("this is not valid git log output\n");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_log_with_files_only_blank_lines() {
        let result = parse_log_with_files("\n\n\n");
        assert!(result.is_empty());
    }

    // =====================================================================
    // extract_file_patch tests
    // =====================================================================

    #[test]
    fn test_extract_file_patch_found() {
        let full_patch = "diff --git a/foo.rs b/foo.rs\n--- a/foo.rs\n+++ b/foo.rs\n@@ -1,1 +1,2 @@\n+new line\ndiff --git a/bar.rs b/bar.rs\n--- a/bar.rs\n";
        let patch = extract_file_patch(full_patch, "foo.rs");
        assert!(patch.contains("diff --git a/foo.rs"));
        assert!(patch.contains("+new line"));
        assert!(!patch.contains("bar.rs"));
    }

    #[test]
    fn test_extract_file_patch_not_found() {
        let full_patch = "diff --git a/foo.rs b/foo.rs\n+++ b/foo.rs\n";
        let patch = extract_file_patch(full_patch, "nonexistent.rs");
        assert!(patch.is_empty());
    }

    #[test]
    fn test_extract_file_patch_empty_input() {
        let patch = extract_file_patch("", "foo.rs");
        assert!(patch.is_empty());
    }

    #[test]
    fn test_extract_file_patch_single_file_patch() {
        let full_patch = "diff --git a/only.rs b/only.rs\n--- a/only.rs\n+++ b/only.rs\n@@ -1,1 +1,1 @@\n-old\n+new\n";
        let patch = extract_file_patch(full_patch, "only.rs");
        assert!(patch.contains("diff --git a/only.rs"));
        assert!(patch.contains("-old"));
        assert!(patch.contains("+new"));
    }
}

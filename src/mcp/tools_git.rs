//! Git MCP tool handler implementations (9 tools).
//!
//! Contains the business logic for: blame, file_history, recent_changes,
//! commit_diff, symbol_history, branch_info, modified_files, hotspots,
//! and contributors.

use std::path::Path;

use crate::git;

use super::server::json_text;

// 14. codegraph_blame
pub fn handle_blame(project_root: &Path, file_path: &str) -> String {
    match git::blame::git_blame(project_root, file_path) {
        Ok(lines) => json_text(&serde_json::json!({
            "file": file_path,
            "lineCount": lines.len(),
            "lines": lines.iter().map(|l| serde_json::json!({
                "line": l.line_number, "author": l.author, "email": l.email,
                "date": l.date, "commit": l.commit_hash, "content": l.content,
            })).collect::<Vec<_>>(),
        })),
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 15. codegraph_file_history
pub fn handle_file_history(project_root: &Path, file_path: &str, limit: Option<usize>) -> String {
    match git::history::file_history(project_root, file_path, limit.unwrap_or(20)) {
        Ok(commits) => json_text(&serde_json::json!({
            "file": file_path,
            "commitCount": commits.len(),
            "commits": commits.iter().map(|c| serde_json::json!({
                "hash": c.hash, "author": c.author, "email": c.email,
                "date": c.date, "message": c.message,
            })).collect::<Vec<_>>(),
        })),
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 16. codegraph_recent_changes
pub fn handle_recent_changes(project_root: &Path, limit: Option<usize>) -> String {
    match git::history::recent_changes(project_root, limit.unwrap_or(20)) {
        Ok(commits) => json_text(&serde_json::json!({
            "commitCount": commits.len(),
            "commits": commits.iter().map(|c| serde_json::json!({
                "hash": c.hash, "author": c.author, "email": c.email,
                "date": c.date, "message": c.message,
            })).collect::<Vec<_>>(),
        })),
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 17. codegraph_commit_diff
pub fn handle_commit_diff(project_root: &Path, commit: &str) -> String {
    match git::history::commit_diff(project_root, commit) {
        Ok(diff) => json_text(&serde_json::json!({
            "commit": diff.commit,
            "fileCount": diff.files.len(),
            "files": diff.files.iter().map(|f| serde_json::json!({
                "path": f.path, "additions": f.additions, "deletions": f.deletions,
                "patch": f.patch,
            })).collect::<Vec<_>>(),
        })),
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 18. codegraph_symbol_history
pub fn handle_symbol_history(project_root: &Path, symbol: &str) -> String {
    match git::history::symbol_history(project_root, symbol) {
        Ok(commits) => json_text(&serde_json::json!({
            "symbol": symbol,
            "commitCount": commits.len(),
            "commits": commits.iter().map(|c| serde_json::json!({
                "hash": c.hash, "author": c.author, "email": c.email,
                "date": c.date, "message": c.message,
            })).collect::<Vec<_>>(),
        })),
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 19. codegraph_branch_info
pub fn handle_branch_info(project_root: &Path) -> String {
    match git::history::branch_info(project_root) {
        Ok(info) => json_text(&serde_json::json!({
            "current": info.current, "tracking": info.tracking,
            "ahead": info.ahead, "behind": info.behind, "status": info.status,
        })),
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 20. codegraph_modified_files
pub fn handle_modified_files(project_root: &Path) -> String {
    match git::history::modified_files(project_root) {
        Ok(mf) => json_text(&serde_json::json!({
            "staged": mf.staged, "unstaged": mf.unstaged, "untracked": mf.untracked,
            "totalChanges": mf.staged.len() + mf.unstaged.len() + mf.untracked.len(),
        })),
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 21. codegraph_hotspots
pub fn handle_hotspots(project_root: &Path, limit: Option<usize>) -> String {
    match git::analysis::hotspots(project_root, limit.unwrap_or(20)) {
        Ok(spots) => json_text(&serde_json::json!({
            "hotspotCount": spots.len(),
            "hotspots": spots.iter().map(|h| serde_json::json!({
                "file": h.file, "commitCount": h.commit_count,
                "lastModified": h.last_modified, "score": h.score,
            })).collect::<Vec<_>>(),
        })),
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

// 22. codegraph_contributors
pub fn handle_contributors(project_root: &Path, file_path: Option<&str>) -> String {
    match git::analysis::contributors(project_root, file_path) {
        Ok(contribs) => json_text(&serde_json::json!({
            "contributorCount": contribs.len(),
            "contributors": contribs.iter().map(|c| serde_json::json!({
                "name": c.name, "email": c.email, "commits": c.commits,
                "linesAdded": c.lines_added, "linesRemoved": c.lines_removed,
            })).collect::<Vec<_>>(),
        })),
        Err(e) => json_text(&serde_json::json!({"error": e.to_string()})),
    }
}

//! Git blame integration.
//!
//! Parses `git blame --porcelain` output into structured `BlameLine` records.

use std::path::Path;

use super::{run_git, validate_input, BlameLine};
use crate::error::CodeGraphError;

/// Run `git blame --porcelain` on a file and return structured blame data.
///
/// Each `BlameLine` contains the commit hash, author, email, ISO-8601 date,
/// line number, and the actual source line content.
pub fn git_blame(repo_path: &Path, file_path: &str) -> Result<Vec<BlameLine>, CodeGraphError> {
    validate_input(file_path, "file_path")?;

    let output = run_git(repo_path, &["blame", "--porcelain", "--", file_path])?;
    parse_blame_porcelain(&output)
}

/// Parse porcelain blame output.
///
/// Porcelain format emits blocks like:
/// ```text
/// <40-hex-hash> <orig-line> <final-line> [<group-lines>]
/// author <name>
/// author-mail <<email>>
/// author-time <unix-timestamp>
/// author-tz <tz>
/// committer ...
/// ...
/// summary <msg>
/// ...
/// \t<line-content>
/// ```
fn parse_blame_porcelain(output: &str) -> Result<Vec<BlameLine>, CodeGraphError> {
    let mut results = Vec::new();
    let mut commit_hash = String::new();
    let mut author = String::new();
    let mut email = String::new();
    let mut date = String::new();
    let mut line_number: usize = 0;

    for line in output.lines() {
        // Commit header: 40 hex chars followed by orig/final line numbers
        if line.len() >= 40
            && line.as_bytes().get(40) == Some(&b' ')
            && line[..40].bytes().all(|b| b.is_ascii_hexdigit())
        {
            commit_hash = line[..40].to_string();
            // final-line is the 3rd token (index 2) — but for the first line in
            // a group it's token[2], for continuation lines it's token[1].
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                line_number = parts[2].parse().unwrap_or(line_number + 1);
            } else if parts.len() == 2 {
                line_number = parts[1].parse().unwrap_or(line_number + 1);
            }
        } else if let Some(val) = line.strip_prefix("author ") {
            author = val.to_string();
        } else if let Some(val) = line.strip_prefix("author-mail ") {
            email = val.trim_matches(|c| c == '<' || c == '>').to_string();
        } else if let Some(val) = line.strip_prefix("author-time ") {
            // Convert unix timestamp to ISO-8601 via chrono.
            if let Ok(ts) = val.parse::<i64>() {
                date = chrono::DateTime::from_timestamp(ts, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| val.to_string());
            }
        } else if let Some(content) = line.strip_prefix('\t') {
            // Content line — push the accumulated record.
            results.push(BlameLine {
                line_number,
                commit_hash: commit_hash.clone(),
                author: author.clone(),
                email: email.clone(),
                date: date.clone(),
                content: content.to_string(),
            });
        }
    }

    Ok(results)
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Create a temporary git repo with one committed file.
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

        std::fs::write(
            path.join("hello.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();
        git(&["add", "hello.rs"]);
        git(&["commit", "-m", "initial commit"]);

        (dir, path)
    }

    #[test]
    fn test_blame_basic() {
        let (_dir, path) = create_test_repo();
        let blame = git_blame(&path, "hello.rs").unwrap();

        assert_eq!(blame.len(), 3);
        assert_eq!(blame[0].line_number, 1);
        assert_eq!(blame[0].author, "Test Author");
        assert_eq!(blame[0].email, "test@example.com");
        assert!(blame[0].content.contains("fn main"));
        assert!(!blame[0].commit_hash.is_empty());
        assert!(!blame[0].date.is_empty());
    }

    #[test]
    fn test_blame_line_numbers_sequential() {
        let (_dir, path) = create_test_repo();
        let blame = git_blame(&path, "hello.rs").unwrap();

        let numbers: Vec<usize> = blame.iter().map(|b| b.line_number).collect();
        assert_eq!(numbers, vec![1, 2, 3]);
    }

    #[test]
    fn test_blame_nonexistent_file() {
        let (_dir, path) = create_test_repo();
        let result = git_blame(&path, "nope.rs");
        assert!(result.is_err());
    }

    #[test]
    fn test_blame_not_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        let result = git_blame(dir.path(), "hello.rs");
        assert!(result.is_err());
    }

    #[test]
    fn test_blame_argument_injection() {
        let (_dir, path) = create_test_repo();
        let result = git_blame(&path, "--help");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot start with '-'"));
    }

    #[test]
    fn test_blame_null_byte_injection() {
        let (_dir, path) = create_test_repo();
        let result = git_blame(&path, "hello\0.rs");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("null bytes"));
    }

    #[test]
    fn test_parse_blame_porcelain_empty() {
        let result = parse_blame_porcelain("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_blame_multiple_commits() {
        let (_dir, path) = create_test_repo();

        // Add a second line via a new commit
        std::fs::write(
            path.join("hello.rs"),
            "fn main() {\n    println!(\"hello\");\n    println!(\"world\");\n}\n",
        )
        .unwrap();
        std::process::Command::new("git")
            .args(["add", "hello.rs"])
            .current_dir(&path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "add world line"])
            .current_dir(&path)
            .env("GIT_AUTHOR_NAME", "Other Author")
            .env("GIT_AUTHOR_EMAIL", "other@example.com")
            .env("GIT_COMMITTER_NAME", "Other Author")
            .env("GIT_COMMITTER_EMAIL", "other@example.com")
            .output()
            .unwrap();

        let blame = git_blame(&path, "hello.rs").unwrap();
        assert_eq!(blame.len(), 4);
        // The new line (line 3) should be by the second author
        let authors: Vec<&str> = blame.iter().map(|b| b.author.as_str()).collect();
        assert!(authors.contains(&"Other Author"));
    }

    // =====================================================================
    // Additional blame tests
    // =====================================================================

    #[test]
    fn test_blame_commit_hash_is_40_hex_chars() {
        let (_dir, path) = create_test_repo();
        let blame = git_blame(&path, "hello.rs").unwrap();
        for line in &blame {
            assert_eq!(line.commit_hash.len(), 40, "hash length should be 40");
            assert!(
                line.commit_hash.chars().all(|c| c.is_ascii_hexdigit()),
                "hash should be hex: {}",
                line.commit_hash
            );
        }
    }

    #[test]
    fn test_blame_date_format_is_valid() {
        let (_dir, path) = create_test_repo();
        let blame = git_blame(&path, "hello.rs").unwrap();
        for line in &blame {
            // Date should be in YYYY-MM-DD HH:MM:SS format
            assert!(
                line.date.len() >= 10,
                "date should be at least 10 chars: {}",
                line.date
            );
            assert!(
                line.date.contains('-'),
                "date should contain dash: {}",
                line.date
            );
        }
    }

    #[test]
    fn test_blame_content_matches_file() {
        let (_dir, path) = create_test_repo();
        let blame = git_blame(&path, "hello.rs").unwrap();
        let contents: Vec<&str> = blame.iter().map(|b| b.content.as_str()).collect();
        assert_eq!(contents[0], "fn main() {");
        assert!(contents[1].contains("println"));
        assert_eq!(contents[2], "}");
    }

    #[test]
    fn test_blame_single_line_file() {
        let (_dir, path) = create_test_repo();
        std::fs::write(path.join("single.rs"), "fn one_liner() {}\n").unwrap();
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
        git(&["add", "single.rs"]);
        git(&["commit", "-m", "add single line file"]);

        let blame = git_blame(&path, "single.rs").unwrap();
        assert_eq!(blame.len(), 1);
        assert_eq!(blame[0].line_number, 1);
        assert!(blame[0].content.contains("one_liner"));
    }

    #[test]
    fn test_blame_empty_file() {
        let (_dir, path) = create_test_repo();
        std::fs::write(path.join("empty.rs"), "").unwrap();
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
        git(&["add", "empty.rs"]);
        git(&["commit", "-m", "add empty file"]);

        let blame = git_blame(&path, "empty.rs").unwrap();
        assert!(blame.is_empty());
    }

    #[test]
    fn test_blame_subdirectory_file() {
        let (_dir, path) = create_test_repo();
        std::fs::create_dir_all(path.join("src/lib")).unwrap();
        std::fs::write(path.join("src/lib/util.rs"), "pub fn util() {}\n").unwrap();
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
        git(&["add", "."]);
        git(&["commit", "-m", "add subdir file"]);

        let blame = git_blame(&path, "src/lib/util.rs").unwrap();
        assert_eq!(blame.len(), 1);
        assert!(blame[0].content.contains("util"));
    }

    #[test]
    fn test_blame_multiline_file_preserves_order() {
        let (_dir, path) = create_test_repo();
        std::fs::write(path.join("multi.rs"), "line1\nline2\nline3\nline4\nline5\n").unwrap();
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
        git(&["add", "multi.rs"]);
        git(&["commit", "-m", "add multi"]);

        let blame = git_blame(&path, "multi.rs").unwrap();
        assert_eq!(blame.len(), 5);
        let numbers: Vec<usize> = blame.iter().map(|b| b.line_number).collect();
        assert_eq!(numbers, vec![1, 2, 3, 4, 5]);
        for (i, line) in blame.iter().enumerate() {
            assert_eq!(line.content, format!("line{}", i + 1));
        }
    }

    #[test]
    fn test_blame_all_same_author_for_single_commit() {
        let (_dir, path) = create_test_repo();
        let blame = git_blame(&path, "hello.rs").unwrap();
        let authors: Vec<&str> = blame.iter().map(|b| b.author.as_str()).collect();
        assert!(
            authors.iter().all(|a| *a == "Test Author"),
            "All lines should be by Test Author"
        );
    }

    #[test]
    fn test_blame_all_same_commit_for_single_commit() {
        let (_dir, path) = create_test_repo();
        let blame = git_blame(&path, "hello.rs").unwrap();
        let hashes: Vec<&str> = blame.iter().map(|b| b.commit_hash.as_str()).collect();
        let first = hashes[0];
        assert!(
            hashes.iter().all(|h| *h == first),
            "All lines should share the same commit hash"
        );
    }

    #[test]
    fn test_blame_email_stripped_of_angle_brackets() {
        let (_dir, path) = create_test_repo();
        let blame = git_blame(&path, "hello.rs").unwrap();
        for line in &blame {
            assert!(!line.email.contains('<'), "email should not contain <");
            assert!(!line.email.contains('>'), "email should not contain >");
        }
    }

    #[test]
    fn test_parse_blame_porcelain_malformed_input() {
        // Random garbage should produce empty results, not panic
        let result = parse_blame_porcelain("not a valid porcelain output\nrandom lines\n").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_blame_file_with_special_chars_in_name() {
        let (_dir, path) = create_test_repo();
        std::fs::write(path.join("hello world.rs"), "fn greet() {}\n").unwrap();
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
        git(&["add", "."]);
        git(&["commit", "-m", "add file with spaces"]);

        let blame = git_blame(&path, "hello world.rs").unwrap();
        assert_eq!(blame.len(), 1);
    }

    #[test]
    fn test_blame_file_with_unicode_content() {
        let (_dir, path) = create_test_repo();
        std::fs::write(
            path.join("unicode.rs"),
            "// Turkce karakter: calisma\nfn test() {}\n",
        )
        .unwrap();
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
        git(&["add", "unicode.rs"]);
        git(&["commit", "-m", "add unicode"]);

        let blame = git_blame(&path, "unicode.rs").unwrap();
        assert_eq!(blame.len(), 2);
        assert!(blame[0].content.contains("Turkce"));
    }

    #[test]
    fn test_validate_input_null_byte_middle() {
        let result = super::super::validate_input("file\0path", "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_input_double_dash() {
        let result = super::super::validate_input("--version", "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_input_valid_path() {
        let result = super::super::validate_input("src/main.rs", "test");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_input_path_with_dots() {
        let result = super::super::validate_input("../path/to/file.rs", "test");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_input_empty_string() {
        let result = super::super::validate_input("", "test");
        assert!(result.is_ok());
    }
}

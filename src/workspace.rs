//! Multi-repository workspace support for CodeGraph.
//!
//! A workspace groups multiple repositories under a single umbrella,
//! enabling cross-repo search and unified project management. Each repo
//! maintains its own `.codegraph/codegraph.db` index; the workspace
//! config (`.codegraph-workspace.yaml`) stores paths and metadata.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CodeGraphError, Result};
use crate::graph::search::{HybridSearch, SearchOptions, SearchResult};
use crate::graph::store::GraphStore;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Name of the workspace config file in the workspace root.
const WORKSPACE_CONFIG_FILE: &str = ".codegraph-workspace.yaml";

// ---------------------------------------------------------------------------
// Workspace types
// ---------------------------------------------------------------------------

/// A workspace that groups multiple repositories for cross-repo operations.
#[derive(Debug, Serialize, Deserialize)]
pub struct Workspace {
    /// Ordered list of repositories in this workspace.
    pub repos: Vec<RepoEntry>,
}

/// A single repository entry within a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoEntry {
    /// Human-readable name for this repository (e.g. "frontend", "api").
    pub name: String,
    /// Path to the repository root, relative to the workspace directory.
    pub path: String,
    /// Path to the CodeGraph database, relative to the workspace directory.
    pub db_path: String,
}

/// Search results from a single repository within a workspace search.
#[derive(Debug)]
pub struct WorkspaceSearchResult {
    /// Name of the repository these results came from.
    pub repo_name: String,
    /// Individual search results from this repository.
    pub results: Vec<SearchResult>,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl Workspace {
    /// Load an existing workspace config from `workspace_dir`.
    ///
    /// Reads `.codegraph-workspace.yaml` from the given directory.
    pub fn load(workspace_dir: &Path) -> Result<Self> {
        let config_path = workspace_dir.join(WORKSPACE_CONFIG_FILE);
        if !config_path.exists() {
            return Err(CodeGraphError::Other(format!(
                "No workspace config found at {}",
                config_path.display()
            )));
        }
        let contents = std::fs::read_to_string(&config_path)?;
        let workspace: Workspace = serde_yaml::from_str(&contents)
            .map_err(|e| CodeGraphError::Other(format!("Failed to parse workspace config: {e}")))?;
        Ok(workspace)
    }

    /// Save the workspace config to `workspace_dir`.
    ///
    /// Writes `.codegraph-workspace.yaml` in the given directory.
    pub fn save(&self, workspace_dir: &Path) -> Result<()> {
        let config_path = workspace_dir.join(WORKSPACE_CONFIG_FILE);
        let yaml = serde_yaml::to_string(self).map_err(|e| {
            CodeGraphError::Other(format!("Failed to serialize workspace config: {e}"))
        })?;
        std::fs::write(&config_path, yaml)?;
        Ok(())
    }

    /// Initialize a new empty workspace in `workspace_dir`.
    ///
    /// Creates the `.codegraph-workspace.yaml` file with an empty repo list.
    /// Returns an error if a workspace config already exists.
    pub fn init(workspace_dir: &Path) -> Result<Self> {
        let config_path = workspace_dir.join(WORKSPACE_CONFIG_FILE);
        if config_path.exists() {
            return Err(CodeGraphError::Other(format!(
                "Workspace already exists at {}",
                config_path.display()
            )));
        }
        std::fs::create_dir_all(workspace_dir)?;
        let workspace = Workspace { repos: Vec::new() };
        workspace.save(workspace_dir)?;
        Ok(workspace)
    }

    /// Add a repository to the workspace.
    ///
    /// The `repo_path` is stored relative to `workspace_dir`. The database
    /// path defaults to `<repo_path>/.codegraph/codegraph.db`.
    pub fn add_repo(&mut self, name: &str, repo_path: &Path, workspace_dir: &Path) -> Result<()> {
        // Check for duplicate names.
        if self.repos.iter().any(|r| r.name == name) {
            return Err(CodeGraphError::Other(format!(
                "Repository '{}' already exists in workspace",
                name
            )));
        }

        // Compute relative path from workspace dir.
        let relative_path = make_relative(repo_path, workspace_dir);
        let db_relative = format!("{}/.codegraph/codegraph.db", relative_path);

        self.repos.push(RepoEntry {
            name: name.to_string(),
            path: relative_path,
            db_path: db_relative,
        });
        Ok(())
    }

    /// Remove a repository from the workspace by name.
    ///
    /// Returns an error if the repository is not found.
    pub fn remove_repo(&mut self, name: &str) -> Result<()> {
        let before = self.repos.len();
        self.repos.retain(|r| r.name != name);
        if self.repos.len() == before {
            return Err(CodeGraphError::Other(format!(
                "Repository '{}' not found in workspace",
                name
            )));
        }
        Ok(())
    }

    /// Search across all repositories in the workspace.
    ///
    /// Opens each repo's database, runs FTS5 keyword search, and merges
    /// results sorted by score descending. Results are tagged with their
    /// repo name for disambiguation.
    pub fn search_all(
        &self,
        query: &str,
        limit: usize,
        workspace_dir: &Path,
    ) -> Result<Vec<WorkspaceSearchResult>> {
        let mut workspace_results = Vec::new();

        for repo in &self.repos {
            let db_path = resolve_path(&repo.db_path, workspace_dir);
            if !db_path.exists() {
                // Skip repos that haven't been indexed yet.
                continue;
            }

            let store = GraphStore::new(db_path.to_str().unwrap_or_default())?;
            let search = HybridSearch::new(&store.conn);
            let options = SearchOptions {
                limit: Some(limit),
                ..Default::default()
            };

            match search.search(query, &options) {
                Ok(results) => {
                    if !results.is_empty() {
                        workspace_results.push(WorkspaceSearchResult {
                            repo_name: repo.name.clone(),
                            results,
                        });
                    }
                }
                Err(e) => {
                    // Log but don't fail the entire search for one repo.
                    eprintln!("[workspace] search failed for repo '{}': {}", repo.name, e);
                }
            }
        }

        // Sort each repo's results are already sorted; sort repos by their
        // best score so the most relevant repo comes first.
        workspace_results.sort_by(|a, b| {
            let a_best = a.results.first().map(|r| r.score).unwrap_or(0.0);
            let b_best = b.results.first().map(|r| r.score).unwrap_or(0.0);
            b_best
                .partial_cmp(&a_best)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(workspace_results)
    }
}

// ---------------------------------------------------------------------------
// CLI command functions
// ---------------------------------------------------------------------------

/// Initialize a new workspace in the given directory.
pub fn cmd_workspace_init(dir: &Path) -> Result<()> {
    let ws = Workspace::init(dir)?;
    println!(
        "Initialized empty workspace at {}",
        dir.join(WORKSPACE_CONFIG_FILE).display()
    );
    println!("Repos: {}", ws.repos.len());
    Ok(())
}

/// Add a repository to the workspace.
pub fn cmd_workspace_add(dir: &Path, name: &str, repo_path: &Path) -> Result<()> {
    let mut ws = Workspace::load(dir)?;
    ws.add_repo(name, repo_path, dir)?;
    ws.save(dir)?;
    println!("Added repo '{}' -> {}", name, repo_path.display());
    Ok(())
}

/// Remove a repository from the workspace.
pub fn cmd_workspace_remove(dir: &Path, name: &str) -> Result<()> {
    let mut ws = Workspace::load(dir)?;
    ws.remove_repo(name)?;
    ws.save(dir)?;
    println!("Removed repo '{}'", name);
    Ok(())
}

/// List all repositories in the workspace.
pub fn cmd_workspace_list(dir: &Path) -> Result<()> {
    let ws = Workspace::load(dir)?;
    if ws.repos.is_empty() {
        println!("No repositories in workspace.");
    } else {
        println!("Workspace repositories ({}):", ws.repos.len());
        for repo in &ws.repos {
            let db_exists = resolve_path(&repo.db_path, dir).exists();
            let status = if db_exists { "indexed" } else { "not indexed" };
            println!("  {} -> {} [{}]", repo.name, repo.path, status);
        }
    }
    Ok(())
}

/// Search across all repositories in the workspace.
pub fn cmd_workspace_search(dir: &Path, query: &str, limit: usize) -> Result<()> {
    let ws = Workspace::load(dir)?;
    let results = ws.search_all(query, limit, dir)?;

    if results.is_empty() {
        println!("No results found across workspace.");
        return Ok(());
    }

    let total: usize = results.iter().map(|r| r.results.len()).sum();
    println!("Found {} results across {} repos:\n", total, results.len());

    for wsr in &results {
        println!("[{}]", wsr.repo_name);
        for sr in &wsr.results {
            println!(
                "  {:.4}  {} ({}) — {}",
                sr.score, sr.name, sr.kind, sr.file_path
            );
        }
        println!();
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Make `target` relative to `base` if possible, otherwise return the
/// target path as-is.
fn make_relative(target: &Path, base: &Path) -> String {
    // Try to strip the base prefix for a clean relative path.
    if let Ok(rel) = target.strip_prefix(base) {
        // Use ./ prefix for clarity.
        format!("./{}", rel.display())
    } else {
        target.display().to_string()
    }
}

/// Resolve a potentially relative path against a base directory.
fn resolve_path(path_str: &str, base: &Path) -> PathBuf {
    let p = Path::new(path_str);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a temporary workspace directory.
    fn setup_workspace() -> TempDir {
        TempDir::new().expect("failed to create temp dir")
    }

    // -- Workspace init -------------------------------------------------------

    #[test]
    fn init_creates_workspace_config() {
        let tmp = setup_workspace();
        let ws = Workspace::init(tmp.path()).unwrap();
        assert!(ws.repos.is_empty());
        assert!(tmp.path().join(WORKSPACE_CONFIG_FILE).exists());
    }

    #[test]
    fn init_fails_if_already_exists() {
        let tmp = setup_workspace();
        Workspace::init(tmp.path()).unwrap();
        let err = Workspace::init(tmp.path()).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    // -- Workspace load/save --------------------------------------------------

    #[test]
    fn load_roundtrips_with_save() {
        let tmp = setup_workspace();
        let mut ws = Workspace::init(tmp.path()).unwrap();
        let repo_dir = tmp.path().join("my-repo");
        fs::create_dir_all(&repo_dir).unwrap();
        ws.add_repo("my-repo", &repo_dir, tmp.path()).unwrap();
        ws.save(tmp.path()).unwrap();

        let loaded = Workspace::load(tmp.path()).unwrap();
        assert_eq!(loaded.repos.len(), 1);
        assert_eq!(loaded.repos[0].name, "my-repo");
    }

    #[test]
    fn load_fails_if_no_config() {
        let tmp = setup_workspace();
        let err = Workspace::load(tmp.path()).unwrap_err();
        assert!(err.to_string().contains("No workspace config"));
    }

    // -- add_repo -------------------------------------------------------------

    #[test]
    fn add_repo_stores_entry() {
        let tmp = setup_workspace();
        let mut ws = Workspace::init(tmp.path()).unwrap();
        let repo_dir = tmp.path().join("frontend");
        fs::create_dir_all(&repo_dir).unwrap();

        ws.add_repo("frontend", &repo_dir, tmp.path()).unwrap();
        assert_eq!(ws.repos.len(), 1);
        assert_eq!(ws.repos[0].name, "frontend");
        assert!(ws.repos[0].path.contains("frontend"));
        assert!(ws.repos[0].db_path.contains(".codegraph/codegraph.db"));
    }

    #[test]
    fn add_repo_rejects_duplicates() {
        let tmp = setup_workspace();
        let mut ws = Workspace::init(tmp.path()).unwrap();
        let repo_dir = tmp.path().join("api");
        fs::create_dir_all(&repo_dir).unwrap();

        ws.add_repo("api", &repo_dir, tmp.path()).unwrap();
        let err = ws.add_repo("api", &repo_dir, tmp.path()).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn add_multiple_repos() {
        let tmp = setup_workspace();
        let mut ws = Workspace::init(tmp.path()).unwrap();

        for name in &["frontend", "backend", "shared"] {
            let d = tmp.path().join(name);
            fs::create_dir_all(&d).unwrap();
            ws.add_repo(name, &d, tmp.path()).unwrap();
        }
        assert_eq!(ws.repos.len(), 3);
    }

    // -- remove_repo ----------------------------------------------------------

    #[test]
    fn remove_repo_removes_entry() {
        let tmp = setup_workspace();
        let mut ws = Workspace::init(tmp.path()).unwrap();
        let repo_dir = tmp.path().join("old-repo");
        fs::create_dir_all(&repo_dir).unwrap();
        ws.add_repo("old-repo", &repo_dir, tmp.path()).unwrap();
        assert_eq!(ws.repos.len(), 1);

        ws.remove_repo("old-repo").unwrap();
        assert!(ws.repos.is_empty());
    }

    #[test]
    fn remove_repo_fails_for_unknown() {
        let tmp = setup_workspace();
        let mut ws = Workspace::init(tmp.path()).unwrap();
        let err = ws.remove_repo("ghost").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn remove_repo_preserves_others() {
        let tmp = setup_workspace();
        let mut ws = Workspace::init(tmp.path()).unwrap();
        for name in &["a", "b", "c"] {
            let d = tmp.path().join(name);
            fs::create_dir_all(&d).unwrap();
            ws.add_repo(name, &d, tmp.path()).unwrap();
        }

        ws.remove_repo("b").unwrap();
        assert_eq!(ws.repos.len(), 2);
        assert!(ws.repos.iter().any(|r| r.name == "a"));
        assert!(ws.repos.iter().any(|r| r.name == "c"));
        assert!(!ws.repos.iter().any(|r| r.name == "b"));
    }

    // -- YAML serialization ---------------------------------------------------

    #[test]
    fn workspace_yaml_format() {
        let ws = Workspace {
            repos: vec![
                RepoEntry {
                    name: "frontend".to_string(),
                    path: "./frontend".to_string(),
                    db_path: "./frontend/.codegraph/codegraph.db".to_string(),
                },
                RepoEntry {
                    name: "backend".to_string(),
                    path: "./backend".to_string(),
                    db_path: "./backend/.codegraph/codegraph.db".to_string(),
                },
            ],
        };

        let yaml = serde_yaml::to_string(&ws).unwrap();
        assert!(yaml.contains("frontend"));
        assert!(yaml.contains("backend"));
        assert!(yaml.contains(".codegraph/codegraph.db"));

        // Round-trip parse.
        let parsed: Workspace = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.repos.len(), 2);
        assert_eq!(parsed.repos[0].name, "frontend");
        assert_eq!(parsed.repos[1].name, "backend");
    }

    // -- Path helpers ---------------------------------------------------------

    #[test]
    fn make_relative_strips_prefix() {
        let base = Path::new("/workspace");
        let target = Path::new("/workspace/frontend");
        assert_eq!(make_relative(target, base), "./frontend");
    }

    #[test]
    fn make_relative_keeps_absolute_if_disjoint() {
        let base = Path::new("/workspace");
        let target = Path::new("/other/repo");
        assert_eq!(make_relative(target, base), "/other/repo");
    }

    #[test]
    fn resolve_path_absolute_stays_absolute() {
        let result = resolve_path("/abs/path/db.sqlite", Path::new("/base"));
        assert_eq!(result, PathBuf::from("/abs/path/db.sqlite"));
    }

    #[test]
    fn resolve_path_relative_joins_base() {
        let result = resolve_path(
            "./frontend/.codegraph/codegraph.db",
            Path::new("/workspace"),
        );
        assert_eq!(
            result,
            PathBuf::from("/workspace/./frontend/.codegraph/codegraph.db")
        );
    }

    // -- cross-repo search with real databases --------------------------------

    #[test]
    fn search_all_skips_missing_databases() {
        let tmp = setup_workspace();
        let ws = Workspace {
            repos: vec![RepoEntry {
                name: "ghost".to_string(),
                path: "./ghost".to_string(),
                db_path: "./ghost/.codegraph/codegraph.db".to_string(),
            }],
        };

        // No DB file exists, so search should return empty (not error).
        let results = ws.search_all("test", 10, tmp.path()).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn search_all_across_two_repos() {
        use crate::types::{CodeNode, Language, NodeKind};

        let tmp = setup_workspace();

        // Create two repo databases with different content.
        let repo_a_dir = tmp.path().join("repo-a/.codegraph");
        let repo_b_dir = tmp.path().join("repo-b/.codegraph");
        fs::create_dir_all(&repo_a_dir).unwrap();
        fs::create_dir_all(&repo_b_dir).unwrap();

        let db_a_path = repo_a_dir.join("codegraph.db");
        let db_b_path = repo_b_dir.join("codegraph.db");

        // Populate repo A.
        {
            let store = GraphStore::new(db_a_path.to_str().unwrap()).unwrap();
            let node = CodeNode {
                id: "fn:auth:login".to_string(),
                name: "login".to_string(),
                qualified_name: Some("auth.login".to_string()),
                kind: NodeKind::Function,
                file_path: "auth.ts".to_string(),
                start_line: 1,
                end_line: 10,
                start_column: 0,
                end_column: 1,
                language: Language::TypeScript,
                body: Some("function login() {}".to_string()),
                documentation: Some("Login handler".to_string()),
                exported: Some(true),
            };
            store.upsert_node(&node).unwrap();
        }

        // Populate repo B.
        {
            let store = GraphStore::new(db_b_path.to_str().unwrap()).unwrap();
            let node = CodeNode {
                id: "fn:api:login_endpoint".to_string(),
                name: "login_endpoint".to_string(),
                qualified_name: Some("api.login_endpoint".to_string()),
                kind: NodeKind::Function,
                file_path: "api.py".to_string(),
                start_line: 5,
                end_line: 20,
                start_column: 0,
                end_column: 1,
                language: Language::Python,
                body: Some("def login_endpoint(): pass".to_string()),
                documentation: Some("API login endpoint".to_string()),
                exported: Some(true),
            };
            store.upsert_node(&node).unwrap();
        }

        let ws = Workspace {
            repos: vec![
                RepoEntry {
                    name: "repo-a".to_string(),
                    path: "./repo-a".to_string(),
                    db_path: format!("{}", db_a_path.display()),
                },
                RepoEntry {
                    name: "repo-b".to_string(),
                    path: "./repo-b".to_string(),
                    db_path: format!("{}", db_b_path.display()),
                },
            ],
        };

        let results = ws.search_all("login", 10, tmp.path()).unwrap();
        // Both repos should have results for "login".
        assert!(
            results.len() >= 1,
            "expected at least 1 repo with results, got {}",
            results.len()
        );

        let total: usize = results.iter().map(|r| r.results.len()).sum();
        assert!(
            total >= 2,
            "expected at least 2 total results, got {}",
            total
        );

        // Check that repo names are present.
        let repo_names: Vec<&str> = results.iter().map(|r| r.repo_name.as_str()).collect();
        assert!(
            repo_names.contains(&"repo-a"),
            "repo-a should be in results"
        );
        assert!(
            repo_names.contains(&"repo-b"),
            "repo-b should be in results"
        );
    }

    // -- cmd functions --------------------------------------------------------

    #[test]
    fn cmd_init_and_list() {
        let tmp = setup_workspace();
        cmd_workspace_init(tmp.path()).unwrap();
        cmd_workspace_list(tmp.path()).unwrap();
    }

    #[test]
    fn cmd_add_and_remove() {
        let tmp = setup_workspace();
        cmd_workspace_init(tmp.path()).unwrap();

        let repo_dir = tmp.path().join("my-lib");
        fs::create_dir_all(&repo_dir).unwrap();
        cmd_workspace_add(tmp.path(), "my-lib", &repo_dir).unwrap();

        let ws = Workspace::load(tmp.path()).unwrap();
        assert_eq!(ws.repos.len(), 1);

        cmd_workspace_remove(tmp.path(), "my-lib").unwrap();

        let ws = Workspace::load(tmp.path()).unwrap();
        assert!(ws.repos.is_empty());
    }

    #[test]
    fn cmd_search_with_no_results() {
        let tmp = setup_workspace();
        cmd_workspace_init(tmp.path()).unwrap();
        cmd_workspace_search(tmp.path(), "nonexistent", 10).unwrap();
    }
}

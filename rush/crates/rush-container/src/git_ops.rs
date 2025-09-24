//! Optimized Git operations using libgit2
//!
//! This module provides fast Git operations using libgit2-rs
//! for improved performance over command-line git.

use git2::{Repository, Status, StatusOptions, Oid, Tree, DiffOptions};
use rush_core::{Error, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use log::{debug, info, trace, warn};
use rayon::prelude::*;
use sha2::{Sha256, Digest};

/// Cache entry for git status
#[derive(Debug, Clone)]
struct StatusCacheEntry {
    /// The status result
    status: GitStatus,
    /// When this entry was cached
    cached_at: Instant,
}

/// Git status for a path or set of paths
#[derive(Debug, Clone)]
pub struct GitStatus {
    /// Is the working tree dirty
    pub is_dirty: bool,
    /// Number of modified files
    pub modified_count: usize,
    /// Number of untracked files
    pub untracked_count: usize,
    /// Modified files
    pub modified_files: Vec<PathBuf>,
    /// Untracked files
    pub untracked_files: Vec<PathBuf>,
}

/// Optimized Git operations
pub struct GitOperations {
    /// The Git repository
    repo: Repository,
    /// Status cache
    status_cache: Arc<RwLock<HashMap<String, StatusCacheEntry>>>,
    /// Cache TTL
    cache_ttl: Duration,
    /// Parallel processing enabled
    parallel_enabled: bool,
}

impl GitOperations {
    /// Open a repository at the given path
    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::open(path)
            .map_err(|e| Error::Internal(format!("Git: Failed to open repository: {}", e)))?;

        Ok(Self {
            repo,
            status_cache: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl: Duration::from_secs(5),
            parallel_enabled: true,
        })
    }

    /// Discover and open a repository from a path
    pub fn discover(path: &Path) -> Result<Self> {
        let repo = Repository::discover(path)
            .map_err(|e| Error::Internal(format!("Git: Failed to discover repository: {}", e)))?;

        Ok(Self {
            repo,
            status_cache: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl: Duration::from_secs(5),
            parallel_enabled: true,
        })
    }

    /// Check if paths are dirty (have uncommitted changes)
    pub async fn is_dirty_batch(&self, paths: &[PathBuf]) -> Result<bool> {
        let start = Instant::now();

        // Check cache first
        let cache_key = self.compute_cache_key(paths);
        if let Some(cached) = self.check_cache(&cache_key).await {
            debug!("Git status cache hit for {} paths", paths.len());
            return Ok(cached.is_dirty);
        }

        // Build pathspecs for libgit2
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .include_ignored(false);

        for path in paths {
            if let Some(path_str) = path.to_str() {
                opts.pathspec(path_str);
            }
        }

        // Get status
        let statuses = self.repo.statuses(Some(&mut opts))
            .map_err(|e| Error::Internal(format!("Git: Failed to get status: {}", e)))?;

        let mut modified_files = Vec::new();
        let mut untracked_files = Vec::new();

        for entry in statuses.iter() {
            let status = entry.status();
            if let Some(path) = entry.path() {
                if status.contains(Status::WT_MODIFIED) ||
                   status.contains(Status::INDEX_MODIFIED) {
                    modified_files.push(PathBuf::from(path));
                } else if status.contains(Status::WT_NEW) {
                    untracked_files.push(PathBuf::from(path));
                }
            }
        }

        let is_dirty = !modified_files.is_empty() || !untracked_files.is_empty();

        let git_status = GitStatus {
            is_dirty,
            modified_count: modified_files.len(),
            untracked_count: untracked_files.len(),
            modified_files,
            untracked_files,
        };

        // Update cache
        self.update_cache(cache_key, git_status.clone()).await;

        debug!("Git status check for {} paths took {:?}: dirty={}",
            paths.len(), start.elapsed(), is_dirty);

        Ok(is_dirty)
    }

    /// Get the most recent commit hash for paths
    pub async fn get_hash_parallel(&self, paths: &[PathBuf]) -> Result<String> {
        let start = Instant::now();

        if paths.is_empty() {
            return Ok(String::new());
        }

        // Get HEAD commit
        let head = self.repo.head()
            .map_err(|e| Error::Internal(format!("Git: Failed to get HEAD: {}", e)))?;

        let commit = head.peel_to_commit()
            .map_err(|e| Error::Internal(format!("Git: Failed to get commit: {}", e)))?;

        // If parallel is disabled, use sequential
        if !self.parallel_enabled || paths.len() < 10 {
            return self.get_hash_sequential(paths, &commit);
        }

        // Process paths in parallel
        let repo_path = self.repo.path().to_path_buf();
        let commit_id = commit.id();

        let hashes: Vec<Option<String>> = paths
            .par_iter()
            .map(|path| {
                // Each thread needs its own repository handle
                let thread_repo = Repository::open(&repo_path).ok()?;
                let thread_commit = thread_repo.find_commit(commit_id).ok()?;

                // Get the tree and look up the path
                let tree = thread_commit.tree().ok()?;
                let entry = tree.get_path(path).ok()?;
                Some(entry.id().to_string())
            })
            .collect();

        // Combine hashes
        let mut combined = Sha256::new();
        for hash in hashes.iter().filter_map(|h| h.as_ref()) {
            combined.update(hash.as_bytes());
        }

        let result = format!("{:x}", combined.finalize());

        debug!("Parallel git hash for {} paths took {:?}",
            paths.len(), start.elapsed());

        Ok(result)
    }

    /// Get hash sequentially (fallback)
    fn get_hash_sequential(&self, paths: &[PathBuf], commit: &git2::Commit) -> Result<String> {
        let mut combined = Sha256::new();

        for path in paths {
            if let Ok(hash) = self.get_path_hash(&self.repo, commit, path) {
                combined.update(hash.as_bytes());
            }
        }

        Ok(format!("{:x}", combined.finalize()))
    }

    /// Get the hash for a specific path
    fn get_path_hash(&self, repo: &Repository, commit: &git2::Commit, path: &Path) -> Result<String> {
        let tree = commit.tree()
            .map_err(|e| Error::Internal(format!("Git: Failed to get tree: {}", e)))?;

        let entry = tree.get_path(path)
            .map_err(|e| Error::Internal(format!("Git: Path not found in tree: {}", e)))?;

        Ok(entry.id().to_string())
    }

    /// Get the last commit that modified paths
    pub fn get_last_commit_for_paths(&self, paths: &[PathBuf]) -> Result<Option<String>> {
        let mut revwalk = self.repo.revwalk()
            .map_err(|e| Error::Internal(format!("Git: Failed to create revwalk: {}", e)))?;

        revwalk.push_head()
            .map_err(|e| Error::Internal(format!("Git: Failed to push HEAD: {}", e)))?;

        // Set sorting to time order
        revwalk.set_sorting(git2::Sort::TIME)
            .map_err(|e| Error::Internal(format!("Git: Failed to set sorting: {}", e)))?;

        // Walk through commits
        for oid in revwalk {
            let oid = oid.map_err(|e| Error::Internal(format!("Git: Failed to get OID: {}", e)))?;
            let commit = self.repo.find_commit(oid)
                .map_err(|e| Error::Internal(format!("Git: Failed to find commit: {}", e)))?;

            // Check if this commit touches any of our paths
            if self.commit_touches_paths(&commit, paths)? {
                return Ok(Some(oid.to_string()));
            }
        }

        Ok(None)
    }

    /// Check if a commit touches any of the given paths
    fn commit_touches_paths(&self, commit: &git2::Commit, paths: &[PathBuf]) -> Result<bool> {
        // Get parent tree (or empty tree for initial commit)
        let parent_tree = if commit.parent_count() > 0 {
            commit.parent(0)
                .map_err(|e| Error::Internal(format!("Git: Failed to get parent: {}", e)))?
                .tree()
                .map_err(|e| Error::Internal(format!("Git: Failed to get parent tree: {}", e)))?
        } else {
            // Create empty tree for initial commit comparison
            let empty_oid = Oid::from_str("4b825dc642cb6eb9a060e54bf8d69288fbee4904")
                .map_err(|e| Error::Internal(format!("Git: Failed to create empty OID: {}", e)))?;
            self.repo.find_tree(empty_oid)
                .map_err(|e| Error::Internal(format!("Git: Failed to find empty tree: {}", e)))?
        };

        let commit_tree = commit.tree()
            .map_err(|e| Error::Internal(format!("Git: Failed to get commit tree: {}", e)))?;

        // Create diff
        let mut diff_opts = DiffOptions::new();
        for path in paths {
            if let Some(path_str) = path.to_str() {
                diff_opts.pathspec(path_str);
            }
        }

        let diff = self.repo.diff_tree_to_tree(
            Some(&parent_tree),
            Some(&commit_tree),
            Some(&mut diff_opts)
        ).map_err(|e| Error::Internal(format!("Git: Failed to create diff: {}", e)))?;

        Ok(diff.stats()
            .map_err(|e| Error::Internal(format!("Git: Failed to get diff stats: {}", e)))?
            .files_changed() > 0)
    }

    /// Clear the status cache
    pub async fn clear_cache(&self) {
        let mut cache = self.status_cache.write().await;
        cache.clear();
        debug!("Cleared git status cache");
    }

    /// Compute cache key for paths
    fn compute_cache_key(&self, paths: &[PathBuf]) -> String {
        let mut hasher = Sha256::new();
        for path in paths {
            hasher.update(path.to_string_lossy().as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }

    /// Check cache for a key
    async fn check_cache(&self, key: &str) -> Option<GitStatus> {
        let cache = self.status_cache.read().await;
        cache.get(key)
            .filter(|entry| entry.cached_at.elapsed() < self.cache_ttl)
            .map(|entry| entry.status.clone())
    }

    /// Update cache with a new entry
    async fn update_cache(&self, key: String, status: GitStatus) {
        let mut cache = self.status_cache.write().await;
        cache.insert(key, StatusCacheEntry {
            status,
            cached_at: Instant::now(),
        });

        // Limit cache size
        if cache.len() > 1000 {
            // Remove oldest entries
            let mut entries: Vec<_> = cache.iter()
                .map(|(k, v)| (k.clone(), v.cached_at))
                .collect();
            entries.sort_by_key(|e| e.1);

            for (key, _) in entries.iter().take(cache.len() - 500) {
                cache.remove(key);
            }
        }
    }

    /// Get repository statistics
    pub fn get_stats(&self) -> Result<RepoStats> {
        let head = self.repo.head()
            .map_err(|e| Error::Internal(format!("Git: Failed to get HEAD: {}", e)))?;

        let commit = head.peel_to_commit()
            .map_err(|e| Error::Internal(format!("Git: Failed to get commit: {}", e)))?;

        let statuses = self.repo.statuses(None)
            .map_err(|e| Error::Internal(format!("Git: Failed to get statuses: {}", e)))?;

        let mut modified = 0;
        let mut untracked = 0;
        let mut staged = 0;

        for entry in statuses.iter() {
            let status = entry.status();
            if status.contains(Status::WT_MODIFIED) {
                modified += 1;
            }
            if status.contains(Status::WT_NEW) {
                untracked += 1;
            }
            if status.contains(Status::INDEX_NEW) ||
               status.contains(Status::INDEX_MODIFIED) {
                staged += 1;
            }
        }

        Ok(RepoStats {
            current_branch: head.shorthand().unwrap_or("unknown").to_string(),
            current_commit: commit.id().to_string(),
            modified_files: modified,
            untracked_files: untracked,
            staged_files: staged,
        })
    }
}

/// Repository statistics
#[derive(Debug, Clone)]
pub struct RepoStats {
    /// Current branch name
    pub current_branch: String,
    /// Current commit hash
    pub current_commit: String,
    /// Number of modified files
    pub modified_files: usize,
    /// Number of untracked files
    pub untracked_files: usize,
    /// Number of staged files
    pub staged_files: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn init_test_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();

        // Create initial commit
        let sig = repo.signature().unwrap();
        let tree_id = {
            let mut index = repo.index().unwrap();
            index.write_tree().unwrap()
        };
        let tree = repo.find_tree(tree_id).unwrap();

        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "Initial commit",
            &tree,
            &[],
        ).unwrap();

        (dir, repo)
    }

    #[tokio::test]
    async fn test_git_operations_basic() {
        let (dir, _repo) = init_test_repo();
        let git_ops = GitOperations::open(dir.path()).unwrap();

        // Check clean status
        let is_dirty = git_ops.is_dirty_batch(&[dir.path().to_path_buf()]).await.unwrap();
        assert!(!is_dirty);

        // Create a new file
        let test_file = dir.path().join("test.txt");
        fs::write(&test_file, "test content").unwrap();

        // Should be dirty now
        let is_dirty = git_ops.is_dirty_batch(&[test_file]).await.unwrap();
        assert!(is_dirty);
    }

    #[tokio::test]
    async fn test_cache_functionality() {
        let (dir, _repo) = init_test_repo();
        let git_ops = GitOperations::open(dir.path()).unwrap();

        let paths = vec![dir.path().to_path_buf()];

        // First call should miss cache
        let _result1 = git_ops.is_dirty_batch(&paths).await.unwrap();

        // Second call should hit cache (within TTL)
        let start = Instant::now();
        let _result2 = git_ops.is_dirty_batch(&paths).await.unwrap();
        let duration = start.elapsed();

        // Cache hit should be very fast
        assert!(duration < Duration::from_millis(10));
    }
}
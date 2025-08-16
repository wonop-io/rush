//! Kubernetes infrastructure management
//!
//! This module provides functionality for managing Kubernetes infrastructure resources,
//! including operations like cloning repositories, copying manifests, and committing changes.

use crate::context::KubernetesContext;
use log::{debug, info};
use rush_core::error::{Error, Result};
use rush_toolchain::ToolchainContext;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

/// Manages infrastructure repositories for Kubernetes deployments
#[derive(Debug)]
pub struct InfrastructureRepo {
    /// URL of the repository containing infrastructure code
    repository_url: String,
    /// Local path where the repository is cloned
    local_path: PathBuf,
    /// Environment (dev, staging, prod)
    environment: String,
    /// Product name
    product_name: String,
    /// Toolchain for executing Git commands
    toolchain: Arc<ToolchainContext>,
    /// Kubernetes context to use for operations
    k8s_context: Option<Arc<KubernetesContext>>,
}

impl InfrastructureRepo {
    /// Creates a new infrastructure repository manager
    ///
    /// # Arguments
    ///
    /// * `repository_url` - URL of the Git repository
    /// * `local_path` - Local path to clone the repository to
    /// * `environment` - Environment name (dev, staging, prod)
    /// * `product_name` - Name of the product
    /// * `toolchain` - Toolchain for executing Git commands
    pub fn new(
        repository_url: String,
        local_path: PathBuf,
        environment: String,
        product_name: String,
        toolchain: Arc<ToolchainContext>,
    ) -> Self {
        Self {
            repository_url,
            local_path,
            environment,
            product_name,
            toolchain,
            k8s_context: None,
        }
    }

    /// Sets the Kubernetes context
    pub fn with_context(mut self, context: Arc<KubernetesContext>) -> Self {
        self.k8s_context = Some(context);
        self
    }

    /// Checks out or clones the infrastructure repository
    ///
    /// If the repository already exists locally, it performs a git pull.
    /// Otherwise, it clones the repository.
    pub async fn checkout(&self) -> Result<()> {
        let _git = self.toolchain.git();

        if self.local_path.exists() {
            debug!("Infrastructure repository exists, updating via pull");

            // Reset any local changes
            self.execute_git_command(&[
                "-C",
                self.local_path.to_str().unwrap(),
                "reset",
                "--hard",
                "HEAD",
            ])?;

            // Clean untracked files
            self.execute_git_command(&["-C", self.local_path.to_str().unwrap(), "clean", "-fd"])?;

            // Pull latest changes
            self.execute_git_command(&["-C", self.local_path.to_str().unwrap(), "pull"])?;

            info!("Successfully updated infrastructure repository");
        } else {
            debug!("Cloning infrastructure repository");

            // Create parent directory if it doesn't exist
            if let Some(parent) = self.local_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| Error::Filesystem(format!("Failed to create directory: {e}")))?;
            }

            // Clone the repository
            self.execute_git_command(&[
                "clone",
                &self.repository_url,
                self.local_path.to_str().unwrap(),
            ])?;

            info!("Successfully cloned infrastructure repository");
        }

        Ok(())
    }

    /// Copies Kubernetes manifests from a source directory to the infrastructure repository
    ///
    /// # Arguments
    ///
    /// * `source_directory` - Directory containing manifests to copy
    pub async fn copy_manifests(&self, source_directory: &PathBuf) -> Result<()> {
        debug!(
            "Copying manifests from {} to infrastructure repository",
            source_directory.display()
        );

        let target_subdirectory = format!("products/{}/{}", self.product_name, self.environment);
        let target_directory = self.local_path.join(target_subdirectory);

        // Delete target directory if it exists
        if target_directory.exists() {
            fs::remove_dir_all(&target_directory).map_err(|e| {
                Error::Filesystem(format!("Failed to remove target directory: {e}"))
            })?;
        }

        // Create target directory
        fs::create_dir_all(&target_directory)
            .map_err(|e| Error::Filesystem(format!("Failed to create target directory: {e}")))?;

        // Copy manifests recursively
        self.copy_directory_recursively(source_directory, &target_directory)?;

        info!("Successfully copied manifests to infrastructure repository");
        Ok(())
    }

    /// Commits and pushes changes to the infrastructure repository
    ///
    /// # Arguments
    ///
    /// * `commit_message` - Git commit message
    pub async fn commit_and_push(&self, commit_message: &str) -> Result<()> {
        debug!("Committing and pushing changes to infrastructure repository");

        // Check if there are any changes to commit
        let status_output = self.execute_git_command(&[
            "-C",
            self.local_path.to_str().unwrap(),
            "status",
            "--porcelain",
        ])?;

        if status_output.trim().is_empty() {
            info!("No changes to commit in infrastructure repository");
            return Ok(());
        }

        // Add all changes
        self.execute_git_command(&["-C", self.local_path.to_str().unwrap(), "add", "."])?;

        // Commit changes
        self.execute_git_command(&[
            "-C",
            self.local_path.to_str().unwrap(),
            "commit",
            "-m",
            commit_message,
        ])?;

        // Push changes
        self.execute_git_command(&["-C", self.local_path.to_str().unwrap(), "push"])?;

        info!("Successfully committed and pushed changes to infrastructure repository");
        Ok(())
    }

    /// Executes a Git command
    ///
    /// # Arguments
    ///
    /// * `args` - Arguments to pass to git
    fn execute_git_command(&self, args: &[&str]) -> Result<String> {
        let output = Command::new(self.toolchain.git())
            .args(args)
            .output()
            .map_err(|e| Error::External(format!("Failed to execute git command: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::External(format!("Git command failed: {stderr}")));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Recursively copies a directory
    ///
    /// # Arguments
    ///
    /// * `source` - Source directory
    /// * `destination` - Destination directory
    fn copy_directory_recursively(&self, source: &Path, destination: &Path) -> Result<()> {
        if !source.is_dir() {
            return Err(Error::InvalidInput(format!(
                "Source is not a directory: {}",
                source.display()
            )));
        }

        if !destination.exists() {
            fs::create_dir_all(destination)
                .map_err(|e| Error::Filesystem(format!("Failed to create directory: {e}")))?;
        }

        for entry in fs::read_dir(source)
            .map_err(|e| Error::Filesystem(format!("Failed to read directory: {e}")))?
        {
            let entry = entry
                .map_err(|e| Error::Filesystem(format!("Failed to read directory entry: {e}")))?;
            let path = entry.path();
            let dest_path = destination.join(path.file_name().unwrap());

            if path.is_dir() {
                self.copy_directory_recursively(&path, &dest_path)?;
            } else {
                fs::copy(&path, &dest_path)
                    .map_err(|e| Error::Filesystem(format!("Failed to copy file: {e}")))?;
            }
        }

        Ok(())
    }

    /// Returns the local path of the infrastructure repository
    pub fn local_path(&self) -> &PathBuf {
        &self.local_path
    }

    /// Returns the product target directory within the infrastructure repository
    pub fn product_target_directory(&self) -> PathBuf {
        self.local_path
            .join("products")
            .join(&self.product_name)
            .join(&self.environment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::{Read, Write};
    use tempfile::TempDir;

    fn create_test_file(dir: &Path, name: &str, content: &str) -> Result<()> {
        let path = dir.join(name);
        let mut file = File::create(&path)
            .map_err(|e| Error::Filesystem(format!("Failed to create test file: {e}")))?;
        file.write_all(content.as_bytes())
            .map_err(|e| Error::Filesystem(format!("Failed to write to test file: {e}")))?;
        Ok(())
    }

    fn read_test_file(path: &Path) -> Result<String> {
        let mut file = File::open(path)
            .map_err(|e| Error::Filesystem(format!("Failed to open test file: {e}")))?;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| Error::Filesystem(format!("Failed to read test file: {e}")))?;
        Ok(content)
    }

    #[test]
    fn test_copy_directory_recursively() {
        let toolchain = Arc::new(ToolchainContext::default());
        let source_dir = TempDir::new().unwrap();
        let dest_dir = TempDir::new().unwrap();

        // Create test directory structure and files
        create_test_file(source_dir.path(), "test1.yaml", "test1 content").unwrap();
        let subdir_path = source_dir.path().join("subdir");
        fs::create_dir(&subdir_path).unwrap();
        create_test_file(&subdir_path, "test2.yaml", "test2 content").unwrap();

        let repo = InfrastructureRepo::new(
            "dummy-url".to_string(),
            dest_dir.path().to_path_buf(),
            "test-env".to_string(),
            "test-product".to_string(),
            toolchain,
        );

        // Test copy
        repo.copy_directory_recursively(source_dir.path(), dest_dir.path())
            .unwrap();

        // Verify files were copied
        let dest_test1 = dest_dir.path().join("test1.yaml");
        let dest_test2 = dest_dir.path().join("subdir").join("test2.yaml");

        assert!(dest_test1.exists());
        assert!(dest_test2.exists());
        assert_eq!(read_test_file(&dest_test1).unwrap(), "test1 content");
        assert_eq!(read_test_file(&dest_test2).unwrap(), "test2 content");
    }
}

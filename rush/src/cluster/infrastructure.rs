use crate::builder::Config;
use crate::toolchain::ToolchainContext;
use crate::utils::{run_command, run_command_in_window};
use colored::Colorize;
use glob::glob;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

pub struct InfrastructureRepo {
    repository_url: String,
    local_path: PathBuf, // Changed back to PathBuf
    environment: String,
    product_name: String,
    toolchain: Arc<ToolchainContext>,
}

impl InfrastructureRepo {
    pub fn new(config: Arc<Config>, toolchain: Arc<ToolchainContext>) -> Self {
        Self {
            repository_url: config.infrastructure_repository().to_string(),
            local_path: PathBuf::from(config.root_path()).join(".infra"), // Already using PathBuf
            environment: config.environment().to_string(),
            product_name: config.product_name().to_string(),
            toolchain,
        }
    }

    pub async fn checkout(&self) -> Result<String, String> {
        let git = self.toolchain.git();
        let window_size = 10; // Example window size, adjust as needed

        if self.local_path.exists() {
            // Directly using PathBuf's exists method
            let formatted_label = "git".white(); // Adjusted label for pull operation

            let args = vec![
                "-C",
                self.local_path.to_str().unwrap(),
                "reset",
                "HEAD",
                "--hard",
            ];
            run_command(/*window_size,*/ formatted_label.clone(), git, args).await?;

            let args = vec!["-C", self.local_path.to_str().unwrap(), "clean", "-fd"];
            run_command(/*window_size,*/ formatted_label.clone(), git, args).await?;

            let args = vec!["-C", self.local_path.to_str().unwrap(), "pull"]; // Adjusted args for pull operation using PathBuf
            run_command(/*window_size,*/ formatted_label, git, args).await
        } else {
            let formatted_label = "git".white(); // Label for clone operation
            let args = vec![
                "clone",
                &self.repository_url,
                self.local_path.to_str().unwrap(),
            ]; // Args for clone operation using PathBuf
            run_command_in_window(window_size, &formatted_label, git, args).await
        }
    }

    pub async fn copy_manifests(&self, source_directory: &PathBuf) -> Result<(), String> {
        let target_subdirectory = format!("products/{}/{}", self.product_name, self.environment);
        let target_directory = self.local_path.join(target_subdirectory); // Directly using PathBuf

        // Delete target directory if it exists
        if target_directory.exists() {
            fs::remove_dir_all(&target_directory).map_err(|e| e.to_string())?;
        }

        // Recreate target directory
        fs::create_dir_all(&target_directory).map_err(|e| e.to_string())?;

        // Use glob to find all .yaml files, including those in subdirectories
        let pattern = format!("{}/**/*", source_directory.to_str().unwrap());
        let paths = glob(&pattern)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .filter(|path| path.is_file());

        for path in paths {
            let canonical_source_directory =
                source_directory.canonicalize().map_err(|e| e.to_string())?;
            let canonical_path = path.canonicalize().map_err(|e| e.to_string())?;
            let relative_path = canonical_path
                .strip_prefix(canonical_source_directory)
                .unwrap();
            let destination = target_directory.join(relative_path);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }

            fs::copy(&canonical_path, &destination).map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    pub async fn commit_and_push(&self, commit_message: &str) -> Result<String, String> {
        let git = self.toolchain.git();
        let window_size = 10; // Example window size, adjust as needed
        let formatted_label_add = "git".white(); // Example label, adjust as needed
        let args_add = vec!["-C", self.local_path.to_str().unwrap(), "add", "."];

        run_command(/*window_size,*/ formatted_label_add, git, args_add).await?;

        let formatted_label_commit = "git".white(); // Example label, adjust as needed
        let args_commit = vec![
            "-C",
            self.local_path.to_str().unwrap(),
            "commit",
            "-m",
            commit_message,
        ];

        run_command(
            /*window_size, &*/ formatted_label_commit,
            git,
            args_commit,
        )
        .await?;

        let formatted_label_push = "git".white(); // Example label, adjust as needed
        let args_push = vec!["-C", self.local_path.to_str().unwrap(), "push"];

        run_command(/*window_size, &*/ formatted_label_push, git, args_push).await
    }
}

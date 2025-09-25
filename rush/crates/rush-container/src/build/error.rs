//! Error handling for the build process
//!
//! This module provides error handling functionality for build operations,
//! including error types and utilities for handling build failures.

use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use colored::Colorize;
use log::{debug, error, info, warn};
use tokio::time;

/// Errors that can occur during the build process
#[derive(Debug)]
pub enum BuildError {
    /// Error during compilation phase
    CompilationError {
        component_name: String,
        message: String,
    },
    /// Error when Docker image build fails
    DockerBuildError {
        component_name: String,
        message: String,
    },
    /// Error when a build script fails
    ScriptError {
        component_name: String,
        message: String,
        exit_code: Option<i32>,
    },
    /// Error when a required tool is not found
    ToolchainError { tool: String, message: String },
    /// Error when a required file is not found
    FileNotFoundError {
        path: PathBuf,
        component_name: String,
    },
    /// Generic build error
    GenericError {
        component_name: String,
        message: String,
    },
}

impl Display for BuildError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::CompilationError {
                component_name,
                message,
            } => {
                write!(
                    f,
                    "Compilation error for {}: {}",
                    component_name.bold(),
                    message
                )
            }
            BuildError::DockerBuildError {
                component_name,
                message,
            } => {
                write!(
                    f,
                    "Docker build error for {}: {}",
                    component_name.bold(),
                    message
                )
            }
            BuildError::ScriptError {
                component_name,
                message,
                exit_code,
            } => {
                if let Some(code) = exit_code {
                    write!(
                        f,
                        "Build script error for {} (exit code {}): {}",
                        component_name.bold(),
                        code,
                        message
                    )
                } else {
                    write!(
                        f,
                        "Build script error for {}: {}",
                        component_name.bold(),
                        message
                    )
                }
            }
            BuildError::ToolchainError { tool, message } => {
                write!(f, "Toolchain error for {}: {}", tool.bold(), message)
            }
            BuildError::FileNotFoundError {
                path,
                component_name,
            } => {
                write!(
                    f,
                    "Required file not found for {}: {}",
                    component_name.bold(),
                    path.display()
                )
            }
            BuildError::GenericError {
                component_name,
                message,
            } => {
                write!(f, "Error for {}: {}", component_name.bold(), message)
            }
        }
    }
}

impl std::error::Error for BuildError {}

/// Handles build errors and provides recovery mechanisms
#[deprecated(note = "Function appears unused - will be removed in next release")]
#[allow(dead_code)]
pub async fn handle_build_error<F>(
    error: BuildError,
    test_if_files_changed: F,
    timeout_duration: Option<Duration>,
) -> Result<(), BuildError>
where
    F: Fn() -> bool,
{
    // Extract component name for logging
    let component_name = match &error {
        BuildError::CompilationError { component_name, .. } => component_name,
        BuildError::DockerBuildError { component_name, .. } => component_name,
        BuildError::ScriptError { component_name, .. } => component_name,
        BuildError::ToolchainError { tool, .. } => tool,
        BuildError::FileNotFoundError { component_name, .. } => component_name,
        BuildError::GenericError { component_name, .. } => component_name,
    };

    // Log the error with appropriate formatting
    error!("Build failed for {}: {}", component_name, error);

    // Set up a check interval for file changes
    let mut check_interval = time::interval(Duration::from_millis(100));

    // Set up timeout (default to 5 minutes if not specified)
    let timeout_duration = timeout_duration.unwrap_or(Duration::from_secs(300));
    let timeout = time::sleep(timeout_duration);
    tokio::pin!(timeout);

    let start_time = Instant::now();
    debug!("Starting error recovery for {}", component_name);

    // Wait for file changes, timeout, or interruption
    loop {
        tokio::select! {
            _ = check_interval.tick() => {
                if test_if_files_changed() {
                    info!(
                        "File changes detected after {:?}. Attempting to rebuild {}...",
                        start_time.elapsed(),
                        component_name
                    );
                    return Ok(());
                }
            }
            _ = &mut timeout => {
                warn!(
                    "Build error recovery timeout reached for {} after {:?}",
                    component_name,
                    timeout_duration
                );
                return Err(error);
            }
            _ = tokio::signal::ctrl_c() => {
                info!(
                    "Termination signal received during build error recovery for {}",
                    component_name
                );
                return Err(BuildError::GenericError {
                    component_name: component_name.to_string(),
                    message: "Build process terminated by user".to_string(),
                });
            }
        }
    }
}

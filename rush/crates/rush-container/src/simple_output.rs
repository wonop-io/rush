//! Simplified output integration for containers using the new sink system
//!
//! This module properly captures stdout/stderr directly from spawned processes
//! instead of using docker logs, ensuring real-time output and color preservation.
//!
//! Now also supports interactive container execution with direct subprocess streaming.

use crate::DockerClient;
use log::{debug, error};
use rush_core::error::{Error, Result};
use rush_core::shutdown;
use rush_output::simple::{LogEntry, Sink};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

/// Simple output line structure for compatibility with simple_docker
#[derive(Debug, Clone)]
pub struct OutputLine {
    /// Component name that generated this line
    pub component: String,
    /// The actual line content
    pub line: String,
    /// Whether this came from stderr
    pub is_error: bool,
}

impl OutputLine {
    /// Convert to LogEntry for sink writing
    pub fn to_log_entry(&self) -> LogEntry {
        // Use docker origin since these are container outputs
        let mut entry = LogEntry::docker(&self.component, &self.line);
        entry.is_error = self.is_error;
        entry
    }
}

/// Extension trait for Sink to handle OutputLine
#[async_trait::async_trait]
pub trait SinkExt: Sink {
    /// Write an OutputLine to the sink
    async fn write_output_line(&mut self, line: OutputLine) -> Result<()>;
}

// Implement for all types that implement Sink
#[async_trait::async_trait]
impl<T: Sink + ?Sized> SinkExt for T {
    async fn write_output_line(&mut self, line: OutputLine) -> Result<()> {
        self.write(line.to_log_entry()).await
    }
}

/// Options for capturing process output
pub struct CaptureOptions {
    pub command: String,
    pub args: Vec<String>,
    pub component_name: String,
    pub sink: Arc<Mutex<Box<dyn Sink>>>,
    pub is_build: bool,
    pub respect_shutdown: bool,
    pub working_dir: Option<PathBuf>,
}

/// Capture output from any spawned process and forward to sink
///
/// This is the core function that properly captures stdout/stderr from a spawned
/// child process and forwards it to the configured sink.
pub async fn capture_process_output(
    command: &str,
    args: Vec<String>,
    component_name: String,
    sink: Arc<Mutex<Box<dyn Sink>>>,
    is_build: bool,
) -> Result<()> {
    // Delegate to the unified function with default options
    capture_output(CaptureOptions {
        command: command.to_string(),
        args,
        component_name,
        sink,
        is_build,
        respect_shutdown: false,
        working_dir: None,
    })
    .await
}

/// Unified output capture function with configurable options
pub async fn capture_output(options: CaptureOptions) -> Result<()> {
    let CaptureOptions {
        command,
        args,
        component_name,
        sink,
        is_build,
        respect_shutdown,
        working_dir,
    } = options;

    debug!(
        "Starting process: {} {:?} for component {}",
        command, args, component_name
    );

    let shutdown_token = if respect_shutdown {
        Some(shutdown::global_shutdown().cancellation_token())
    } else {
        None
    };

    let mut cmd = Command::new(&command);
    cmd.args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    
    // Force color output for common tools
    // Most tools check these environment variables when output is not a TTY
    cmd.env("FORCE_COLOR", "1")
        .env("CARGO_TERM_COLOR", "always")
        .env("RUST_LOG_STYLE", "always")
        .env("CLICOLOR_FORCE", "1")
        .env("COLORTERM", "truecolor")
        .env("TERM", std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string()))
        .env_remove("NO_COLOR");  // Make sure NO_COLOR is not set
    
    // Set working directory if provided
    if let Some(dir) = working_dir {
        debug!("Setting working directory to: {:?}", dir);
        cmd.current_dir(dir);
    }
    
    let mut child = cmd.spawn()
        .map_err(|e| Error::Docker(format!("Failed to spawn process {command} with args {:?}: {e}", args)))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Docker("Failed to capture stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::Docker("Failed to capture stderr".into()))?;

    let mut handles = vec![];

    // Handle stdout
    let component_clone = component_name.clone();
    let sink_clone = sink.clone();
    let shutdown_clone = shutdown_token.clone();
    let stdout_handle = tokio::spawn(async move {
        debug!("Starting stdout reader for {}", component_clone);
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();

        loop {
            if let Some(ref token) = shutdown_clone {
                tokio::select! {
                    result = reader.read_line(&mut line) => {
                        if result.unwrap_or(0) == 0 {
                            break;
                        }
                    }
                    _ = token.cancelled() => {
                        debug!("Stdout reader for {} shutting down", component_clone);
                        break;
                    }
                }
            } else if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                break;
            }

            // Strip trailing newlines but preserve the content otherwise
            // Don't strip \r as it might be used for progress indicators
            let content = line.trim_end_matches('\n');
            
            let entry = if is_build {
                LogEntry::script(&component_clone, content)
            } else {
                LogEntry::docker(&component_clone, content)
            };

            let mut sink_guard = sink_clone.lock().await;
            if let Err(e) = sink_guard.write(entry).await {
                if shutdown_clone.as_ref().is_none_or(|t| !t.is_cancelled()) {
                    error!("Failed to write stdout: {}", e);
                }
            }

            line.clear();
        }
        debug!("Stdout reader finished for {}", component_clone);
    });
    handles.push(stdout_handle);

    // Handle stderr
    let component_clone = component_name.clone();
    let sink_clone = sink.clone();
    let shutdown_clone = shutdown_token.clone();
    let stderr_handle = tokio::spawn(async move {
        debug!("Starting stderr reader for {}", component_clone);
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();

        loop {
            if let Some(ref token) = shutdown_clone {
                tokio::select! {
                    result = reader.read_line(&mut line) => {
                        if result.unwrap_or(0) == 0 {
                            break;
                        }
                    }
                    _ = token.cancelled() => {
                        debug!("Stderr reader for {} shutting down", component_clone);
                        break;
                    }
                }
            } else if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                break;
            }

            // Strip trailing newlines but preserve the content otherwise
            // Don't strip \r as it might be used for progress indicators
            let content = line.trim_end_matches('\n');
            
            let entry = if is_build {
                // For build scripts, stderr is often used for normal output (progress, warnings, etc.)
                // Don't automatically mark it as error - preserve the original formatting
                LogEntry::script(&component_clone, content)
            } else {
                // For Docker containers, stderr typically indicates errors
                LogEntry::docker(&component_clone, content).as_error()
            };

            let mut sink_guard = sink_clone.lock().await;
            if let Err(e) = sink_guard.write(entry).await {
                if shutdown_clone.as_ref().is_none_or(|t| !t.is_cancelled()) {
                    error!("Failed to write stderr: {}", e);
                }
            }

            line.clear();
        }
        debug!("Stderr reader finished for {}", component_clone);
    });
    handles.push(stderr_handle);

    // Wait for all readers to finish or shutdown
    if let Some(ref token) = shutdown_token {
        tokio::select! {
            _ = async {
                for handle in handles {
                    if let Err(e) = handle.await {
                        if !token.is_cancelled() {
                            error!("Reader task failed: {}", e);
                        }
                    }
                }
            } => {
                debug!("All readers finished for {}", component_name);
            }
            _ = token.cancelled() => {
                debug!("Shutting down capture for {}", component_name);
                // Kill the child process on shutdown
                let _ = child.kill().await;
                return Ok(());
            }
        }
    } else {
        for handle in handles {
            if let Err(e) = handle.await {
                error!("Reader task failed: {}", e);
            }
        }
    }

    // Wait for the process to complete
    let status = child
        .wait()
        .await
        .map_err(|e| Error::Docker(format!("Failed to wait for process: {e}")))?;

    if !status.success() {
        // Check if we should ignore this error during shutdown
        if let Some(token) = shutdown_token {
            if token.is_cancelled() {
                return Ok(());
            }

            // Common exit codes that indicate normal termination during shutdown
            let exit_code = status.code().unwrap_or(-1);
            if exit_code == 255 || exit_code == 1 || exit_code == 125 {
                debug!(
                    "{} process for {} exited with code {} (likely due to container stop)",
                    command, component_name, exit_code
                );
                return Ok(());
            }
        }

        return Err(Error::Docker(format!(
            "Process {command} failed with status: {status}"
        )));
    }

    Ok(())
}

/// Capture output from any spawned process with graceful shutdown handling
///
/// This version handles shutdown gracefully and doesn't report errors when
/// processes are terminated due to shutdown.
pub async fn capture_process_output_with_shutdown(
    command: &str,
    args: Vec<String>,
    component_name: String,
    sink: Arc<Mutex<Box<dyn Sink>>>,
    is_build: bool,
) -> Result<()> {
    // Delegate to the unified function with shutdown support
    capture_output(CaptureOptions {
        command: command.to_string(),
        args,
        component_name,
        sink,
        is_build,
        respect_shutdown: true,
        working_dir: None,
    })
    .await
}
/// Follow container logs from the beginning to ensure no output is missed
///
/// This uses docker logs --follow to get all output from container start,
/// avoiding the race condition where attach might miss early output.
/// Handles shutdown gracefully without logging errors when containers are stopped.
pub async fn follow_container_logs_from_start(
    _docker_client: Arc<dyn DockerClient>,
    container_id: &str,
    component_name: String,
    sink: Arc<Mutex<Box<dyn Sink>>>,
) -> Result<()> {
    debug!("Following logs for container {} from start", container_id);

    let args = vec![
        "logs".to_string(),
        "--follow".to_string(), // Follow log output
        "--since".to_string(),
        "0s".to_string(), // Get all logs from the beginning
        container_id.to_string(),
    ];

    // Use the graceful version that handles shutdown
    capture_process_output_with_shutdown(
        "docker",
        args,
        component_name.clone(),
        sink.clone(),
        false, // is_build = false for runtime containers
    )
    .await
}

/// Attach to an already running container and capture its output
/// Follow build output using the simplified sink system
///
/// This captures output directly from the build command process.
pub async fn follow_build_output_simple(
    component_name: String,
    build_command: Vec<String>,
    sink: Arc<Mutex<Box<dyn Sink>>>,
    working_dir: Option<PathBuf>,
) -> Result<()> {
    debug!(
        "Starting build command for {}: {:?} in dir: {:?}",
        component_name, build_command, working_dir
    );

    if build_command.is_empty() {
        return Err(Error::Docker("Build command is empty".to_string()));
    }

    // Use the generic capture function with working directory
    capture_output(CaptureOptions {
        command: build_command[0].clone(),
        args: build_command[1..].to_vec(),
        component_name,
        sink,
        is_build: true,
        respect_shutdown: false,
        working_dir,
    })
    .await
}

/// Capture Docker build output
///
/// This runs docker build and captures its output directly.
pub async fn capture_docker_build(
    tag: &str,
    dockerfile: &str,
    context_path: &str,
    component_name: String,
    sink: Arc<Mutex<Box<dyn Sink>>>,
    platform: Option<&str>,
) -> Result<()> {
    debug!("Building Docker image {} for {}", tag, component_name);

    let mut args = vec!["build".to_string()];

    if let Some(platform) = platform {
        args.push("--platform".to_string());
        args.push(platform.to_string());
    }

    args.extend(vec![
        "-t".to_string(),
        tag.to_string(),
        "-f".to_string(),
        dockerfile.to_string(),
        context_path.to_string(),
    ]);

    capture_process_output(
        "docker",
        args,
        component_name,
        sink,
        true, // is_build = true for docker build
    )
    .await
}

/// Write a system message to the sink
pub async fn write_system_message(
    sink: Arc<Mutex<Box<dyn Sink>>>,
    message: impl Into<String>,
) -> Result<()> {
    let entry = LogEntry::system(message);
    let mut sink_guard = sink.lock().await;
    sink_guard.write(entry).await
}

//! Simplified output integration for containers using the new sink system
//!
//! This module properly captures stdout/stderr directly from spawned processes
//! instead of using docker logs, ensuring real-time output and color preservation.

use crate::DockerClient;
use log::{debug, error, warn};
use rush_core::error::{Error, Result};
use rush_core::shutdown;
use rush_output::simple::{LogEntry, Sink};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

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
    debug!(
        "Starting process: {} {:?} for component {}",
        command, args, component_name
    );

    let mut child = Command::new(command)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Docker(format!("Failed to spawn process {}: {}", command, e)))?;

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
    let stdout_handle = tokio::spawn(async move {
        debug!("Starting stdout reader for {}", component_clone);
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();

        while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
            let entry = if is_build {
                LogEntry::build(&component_clone, &line)
            } else {
                LogEntry::runtime(&component_clone, &line)
            };

            let mut sink_guard = sink_clone.lock().await;
            if let Err(e) = sink_guard.write(entry).await {
                error!("Failed to write stdout: {}", e);
            }

            line.clear();
        }
        debug!("Stdout reader finished for {}", component_clone);
    });
    handles.push(stdout_handle);

    // Handle stderr
    let component_clone = component_name.clone();
    let sink_clone = sink.clone();
    let stderr_handle = tokio::spawn(async move {
        debug!("Starting stderr reader for {}", component_clone);
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();

        while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
            let entry = if is_build {
                LogEntry::build(&component_clone, &line).as_error()
            } else {
                LogEntry::runtime(&component_clone, &line).as_error()
            };

            let mut sink_guard = sink_clone.lock().await;
            if let Err(e) = sink_guard.write(entry).await {
                error!("Failed to write stderr: {}", e);
            }

            line.clear();
        }
        debug!("Stderr reader finished for {}", component_clone);
    });
    handles.push(stderr_handle);

    // Wait for all readers to finish
    for handle in handles {
        if let Err(e) = handle.await {
            error!("Reader task failed: {}", e);
        }
    }

    // Wait for the process to complete
    let status = child
        .wait()
        .await
        .map_err(|e| Error::Docker(format!("Failed to wait for process: {}", e)))?;

    if !status.success() {
        return Err(Error::Docker(format!(
            "Process {} failed with status: {}",
            command, status
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
    debug!(
        "Starting process with shutdown handling: {} {:?} for component {}",
        command, args, component_name
    );

    let shutdown_token = shutdown::global_shutdown().cancellation_token();

    let mut child = Command::new(command)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Docker(format!("Failed to spawn process {}: {}", command, e)))?;

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
            tokio::select! {
                result = reader.read_line(&mut line) => {
                    if result.unwrap_or(0) == 0 {
                        break;
                    }

                    let entry = if is_build {
                        LogEntry::build(&component_clone, &line)
                    } else {
                        LogEntry::runtime(&component_clone, &line)
                    };

                    let mut sink_guard = sink_clone.lock().await;
                    if let Err(e) = sink_guard.write(entry).await {
                        if !shutdown_clone.is_cancelled() {
                            error!("Failed to write stdout: {}", e);
                        }
                    }

                    line.clear();
                }
                _ = shutdown_clone.cancelled() => {
                    debug!("Stdout reader for {} shutting down", component_clone);
                    break;
                }
            }
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
            tokio::select! {
                result = reader.read_line(&mut line) => {
                    if result.unwrap_or(0) == 0 {
                        break;
                    }

                    let entry = if is_build {
                        LogEntry::build(&component_clone, &line).as_error()
                    } else {
                        LogEntry::runtime(&component_clone, &line).as_error()
                    };

                    let mut sink_guard = sink_clone.lock().await;
                    if let Err(e) = sink_guard.write(entry).await {
                        if !shutdown_clone.is_cancelled() {
                            error!("Failed to write stderr: {}", e);
                        }
                    }

                    line.clear();
                }
                _ = shutdown_clone.cancelled() => {
                    debug!("Stderr reader for {} shutting down", component_clone);
                    break;
                }
            }
        }
        debug!("Stderr reader finished for {}", component_clone);
    });
    handles.push(stderr_handle);

    // Wait for all readers to finish or shutdown
    for handle in handles {
        if let Err(e) = handle.await {
            if !shutdown_token.is_cancelled() {
                error!("Reader task failed: {}", e);
            }
        }
    }

    // If we're shutting down, kill the child process gracefully
    if shutdown_token.is_cancelled() {
        debug!(
            "Terminating {} process for {} due to shutdown",
            command, component_name
        );
        let _ = child.kill().await; // Ignore errors during shutdown
        return Ok(()); // Don't report error during shutdown
    }

    // Wait for the process to complete
    let status = child
        .wait()
        .await
        .map_err(|e| Error::Docker(format!("Failed to wait for process: {}", e)))?;

    // Check exit status, but ignore if we're shutting down
    if !status.success() {
        // Common exit codes that indicate normal termination during shutdown
        let exit_code = status.code().unwrap_or(-1);

        // Docker attach returns 255 when the container is stopped
        // Also ignore exit code 1 which can happen during container shutdown
        if exit_code == 255 || exit_code == 1 || exit_code == 125 {
            debug!(
                "{} process for {} exited with code {} (likely due to container stop)",
                command, component_name, exit_code
            );
            return Ok(());
        }

        return Err(Error::Docker(format!(
            "Process {} failed with status: {}",
            command, status
        )));
    }

    Ok(())
}

/// Follow container logs from the beginning to ensure no output is missed
///
/// This uses docker logs --follow to get all output from container start,
/// avoiding the race condition where attach might miss early output.
/// Handles shutdown gracefully without logging errors when containers are stopped.
pub async fn follow_container_logs_from_start(
    docker_client: Arc<dyn DockerClient>,
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
///
/// DEPRECATED: Use follow_container_logs_from_start instead to avoid missing startup logs.
/// This uses docker attach to get direct stream access to a running container.
/// Handles shutdown gracefully without logging errors when containers are stopped.
pub async fn attach_to_container(
    docker_client: Arc<dyn DockerClient>,
    container_id: &str,
    component_name: String,
    sink: Arc<Mutex<Box<dyn Sink>>>,
) -> Result<()> {
    // Use the new method that captures all logs from the start
    follow_container_logs_from_start(docker_client, container_id, component_name, sink).await
}

/// Follow build output using the simplified sink system
///
/// This captures output directly from the build command process.
pub async fn follow_build_output_simple(
    component_name: String,
    build_command: Vec<String>,
    sink: Arc<Mutex<Box<dyn Sink>>>,
) -> Result<()> {
    debug!(
        "Starting build command for {}: {:?}",
        component_name, build_command
    );

    if build_command.is_empty() {
        return Err(Error::Docker("Build command is empty".to_string()));
    }

    // Use the generic capture function
    capture_process_output(
        &build_command[0],
        build_command[1..].to_vec(),
        component_name,
        sink,
        true, // is_build = true for build commands
    )
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

// Legacy function kept for compatibility but now uses proper stream capture
pub async fn follow_container_logs_simple(
    docker_client: Arc<dyn DockerClient>,
    container_id: &str,
    component_name: String,
    sink: Arc<Mutex<Box<dyn Sink>>>,
    is_build: bool,
) -> Result<()> {
    // Use docker attach instead of docker logs for proper stream capture
    attach_to_container(docker_client, container_id, component_name, sink).await
}

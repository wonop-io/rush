//! Simplified output integration for containers using the new sink system
//! 
//! This module properly captures stdout/stderr directly from spawned processes
//! instead of using docker logs, ensuring real-time output and color preservation.

use crate::DockerClient;
use rush_core::error::{Error, Result};
use rush_output::simple::{LogEntry, Sink};
use std::sync::Arc;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use log::{debug, error};

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
    debug!("Starting process: {} {:?} for component {}", command, args, component_name);

    let mut child = Command::new(command)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Docker(format!("Failed to spawn process {}: {}", command, e)))?;

    let stdout = child.stdout.take()
        .ok_or_else(|| Error::Docker("Failed to capture stdout".into()))?;
    let stderr = child.stderr.take()
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
    let status = child.wait().await
        .map_err(|e| Error::Docker(format!("Failed to wait for process: {}", e)))?;

    if !status.success() {
        return Err(Error::Docker(format!(
            "Process {} failed with status: {}",
            command, status
        )));
    }

    Ok(())
}

/// Run a Docker container in foreground mode and capture its output
/// 
/// This runs the container without -d flag, allowing us to capture output directly
/// from the docker run process.
pub async fn run_container_foreground(
    docker_client: Arc<dyn DockerClient>,
    container_name: &str,
    image_name: &str,
    args: Vec<String>,
    component_name: String,
    sink: Arc<Mutex<Box<dyn Sink>>>,
) -> Result<()> {
    debug!("Running container {} in foreground", container_name);

    // Build the docker run command
    let mut docker_args = vec![
        "run".to_string(),
        "--rm".to_string(),           // Remove container when it exits
        "-t".to_string(),              // Allocate pseudo-TTY for colors
        "--name".to_string(),
        container_name.to_string(),
    ];
    
    // Add any additional arguments (env vars, ports, volumes, etc.)
    docker_args.extend(args);
    
    // Add the image name
    docker_args.push(image_name.to_string());

    // Run the container and capture output
    capture_process_output(
        "docker",
        docker_args,
        component_name,
        sink,
        false, // is_build = false for runtime containers
    ).await
}

/// Attach to an already running container and capture its output
/// 
/// This uses docker attach to get direct stream access to a running container.
pub async fn attach_to_container(
    docker_client: Arc<dyn DockerClient>,
    container_id: &str,
    component_name: String,
    sink: Arc<Mutex<Box<dyn Sink>>>,
) -> Result<()> {
    debug!("Attaching to container {} for output", container_id);

    let args = vec![
        "attach".to_string(),
        "--no-stdin".to_string(),  // Don't attach stdin
        container_id.to_string(),
    ];

    capture_process_output(
        "docker",
        args,
        component_name,
        sink,
        false, // is_build = false for runtime containers
    ).await
}

/// Follow build output using the simplified sink system
/// 
/// This captures output directly from the build command process.
pub async fn follow_build_output_simple(
    component_name: String,
    build_command: Vec<String>,
    sink: Arc<Mutex<Box<dyn Sink>>>,
) -> Result<()> {
    debug!("Starting build command for {}: {:?}", component_name, build_command);

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
    ).await
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
    ).await
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
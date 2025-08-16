//! Simplified output integration for containers using the new sink system

use crate::DockerClient;
use rush_core::error::{Error, Result};
use rush_output::simple::{LogEntry, Sink};
use std::sync::Arc;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use log::{debug, error};

/// Follow container logs using the simplified sink system
pub async fn follow_container_logs_simple(
    _docker_client: Arc<dyn DockerClient>,
    container_id: &str,
    component_name: String,
    sink: Arc<Mutex<Box<dyn Sink>>>,
    is_build: bool,
) -> Result<()> {
    debug!("Starting docker logs for container {}", container_id);

    // Use docker logs command to follow the container logs
    let mut child = Command::new("docker")
        .args(["logs", "-f", "--tail", "100", container_id])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Docker(format!("Failed to follow container logs: {e}")))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let mut handles = vec![];

    // Handle stdout
    if let Some(stdout) = stdout {
        let component_name_clone = component_name.clone();
        let sink_clone = sink.clone();
        
        let handle = tokio::spawn(async move {
            debug!("Starting stdout reader for {}", component_name_clone);
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();

            while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                let entry = if is_build {
                    LogEntry::build(&component_name_clone, &line)
                } else {
                    LogEntry::runtime(&component_name_clone, &line)
                };

                let mut sink_guard = sink_clone.lock().await;
                if let Err(e) = sink_guard.write(entry).await {
                    error!("Failed to write stdout: {}", e);
                }
                
                line.clear();
            }
            debug!("Stdout reader finished for {}", component_name_clone);
        });
        handles.push(handle);
    }

    // Handle stderr
    if let Some(stderr) = stderr {
        let component_name_clone = component_name.clone();
        let sink_clone = sink.clone();
        
        let handle = tokio::spawn(async move {
            debug!("Starting stderr reader for {}", component_name_clone);
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();

            while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                let entry = if is_build {
                    LogEntry::build(&component_name_clone, &line).as_error()
                } else {
                    LogEntry::runtime(&component_name_clone, &line).as_error()
                };

                let mut sink_guard = sink_clone.lock().await;
                if let Err(e) = sink_guard.write(entry).await {
                    error!("Failed to write stderr: {}", e);
                }
                
                line.clear();
            }
            debug!("Stderr reader finished for {}", component_name_clone);
        });
        handles.push(handle);
    }

    // Wait for all readers to finish
    for handle in handles {
        if let Err(e) = handle.await {
            error!("Reader task failed: {}", e);
        }
    }

    // Wait for the process to complete
    let status = child.wait().await
        .map_err(|e| Error::Docker(format!("Failed to wait for docker logs: {e}")))?;

    if !status.success() {
        return Err(Error::Docker(format!(
            "Docker logs command failed with status: {}",
            status
        )));
    }

    Ok(())
}

/// Follow build output using the simplified sink system
pub async fn follow_build_output_simple(
    component_name: String,
    build_command: Vec<String>,
    sink: Arc<Mutex<Box<dyn Sink>>>,
) -> Result<()> {
    debug!("Starting build command for {}: {:?}", component_name, build_command);

    if build_command.is_empty() {
        return Err(Error::Docker("Build command is empty".to_string()));
    }

    let mut child = Command::new(&build_command[0])
        .args(&build_command[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(std::env::current_dir()
            .map_err(|e| Error::Docker(format!("Failed to get current directory: {e}")))?)
        .spawn()
        .map_err(|e| Error::Docker(format!("Failed to run build command: {e}")))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let mut handles = vec![];

    // Handle stdout
    if let Some(stdout) = stdout {
        let component_name_clone = component_name.clone();
        let sink_clone = sink.clone();
        
        let handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();

            while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                let entry = LogEntry::build(&component_name_clone, &line);

                let mut sink_guard = sink_clone.lock().await;
                if let Err(e) = sink_guard.write(entry).await {
                    error!("Failed to write build stdout: {}", e);
                }
                
                line.clear();
            }
        });
        handles.push(handle);
    }

    // Handle stderr
    if let Some(stderr) = stderr {
        let component_name_clone = component_name.clone();
        let sink_clone = sink.clone();
        
        let handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();

            while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                // Build errors are still build phase, just marked as errors
                let entry = LogEntry::build(&component_name_clone, &line).as_error();

                let mut sink_guard = sink_clone.lock().await;
                if let Err(e) = sink_guard.write(entry).await {
                    error!("Failed to write build stderr: {}", e);
                }
                
                line.clear();
            }
        });
        handles.push(handle);
    }

    // Wait for all readers to finish
    for handle in handles {
        if let Err(e) = handle.await {
            error!("Build reader task failed: {}", e);
        }
    }

    // Wait for the process to complete
    let status = child.wait().await
        .map_err(|e| Error::Docker(format!("Failed to wait for build command: {e}")))?;

    if !status.success() {
        return Err(Error::Docker(format!(
            "Build command failed with status: {}",
            status
        )));
    }

    Ok(())
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
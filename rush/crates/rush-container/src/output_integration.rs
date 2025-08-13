use crate::DockerClient;
use rush_core::error::{Error, Result};
use rush_output::event::{CompileStage, ExecutionPhase, OutputEvent};
use rush_output::session::OutputSession;
use rush_output::{OutputSource, OutputStream};
use std::sync::Arc;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use log::error;

/// Helper function to follow container logs with an output session
pub async fn follow_container_logs_with_session(
    _docker_client: Arc<dyn DockerClient>,
    container_id: &str,
    component_name: String,
    session: Arc<Mutex<OutputSession>>,
) -> Result<()> {
    eprintln!("DEBUG: Starting docker logs command for container {container_id}");

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
        let session_clone = session.clone();
        let container_id_clone = container_id.to_string();
        
        let handle = tokio::spawn(async move {
            eprintln!("DEBUG: Starting stdout reader for container");
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();

            while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                let source = OutputSource::new(&component_name_clone, "container");
                let stream = OutputStream::stdout(line.as_bytes().to_vec());
                
                let event = OutputEvent::runtime(
                    source,
                    stream,
                    Some(container_id_clone.clone()),
                );

                let mut session_guard = session_clone.lock().await;
                if let Err(e) = session_guard.submit(event).await {
                    error!("Failed to submit stdout event: {}", e);
                }
                
                line.clear();
            }
            eprintln!("DEBUG: Stdout reader finished for container");
        });
        handles.push(handle);
    }

    // Handle stderr
    if let Some(stderr) = stderr {
        let component_name_clone = component_name.clone();
        let session_clone = session.clone();
        let container_id_clone = container_id.to_string();
        
        let handle = tokio::spawn(async move {
            eprintln!("DEBUG: Starting stderr reader for container");
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();

            while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                let source = OutputSource::new(&component_name_clone, "container");
                let stream = OutputStream::stderr(line.as_bytes().to_vec());
                
                let event = OutputEvent::runtime(
                    source,
                    stream,
                    Some(container_id_clone.clone()),
                );

                let mut session_guard = session_clone.lock().await;
                if let Err(e) = session_guard.submit(event).await {
                    error!("Failed to submit stderr event: {}", e);
                }
                
                line.clear();
            }
            eprintln!("DEBUG: Stderr reader finished for container");
        });
        handles.push(handle);
    }

    // Wait for both readers to finish
    for handle in handles {
        if let Err(e) = handle.await {
            error!("Error waiting for log reader: {}", e);
        }
    }

    // Wait for the docker logs process to exit
    let _ = child.wait().await;

    Ok(())
}

/// Submit a build phase event to the output session
pub async fn submit_build_event(
    session: &Arc<Mutex<OutputSession>>,
    component_name: &str,
    stage: CompileStage,
    message: String,
) -> Result<()> {
    let source = OutputSource::new(component_name, "build");
    let stream = OutputStream::stdout(message.into_bytes());
    
    let event = OutputEvent::compile_time(
        source,
        stage,
        component_name.to_string(),
        stream,
    );

    let mut session_guard = session.lock().await;
    session_guard.submit(event).await?;

    Ok(())
}

/// Submit a system event to the output session
pub async fn submit_system_event(
    session: &Arc<Mutex<OutputSession>>,
    subsystem: &str,
    message: String,
) -> Result<()> {
    let source = OutputSource::new("rush", "system");
    let stream = OutputStream::stdout(message.into_bytes());
    
    let event = OutputEvent::system(
        source,
        subsystem.to_string(),
        stream,
    );

    let mut session_guard = session.lock().await;
    session_guard.submit(event).await?;

    Ok(())
}
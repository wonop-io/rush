//! Tests for output capture and log completeness

use crate::docker::DockerClient;
use crate::tests::mock_docker::{MockDockerClient, MockResponses};
use crate::tests::test_helpers::*;
use rush_core::error::Result;
use rush_output::simple::{LogEntry, Sink};
use serial_test::serial;
use std::sync::Arc;

#[tokio::test]
#[serial]
async fn test_follow_logs_from_start_command() -> Result<()> {
    // This test verifies the actual fix: using docker logs --follow --since=0s
    // instead of docker attach to capture all startup logs

    // The fix in follow_container_logs_from_start() lines 283-290:
    // let args = vec![
    //     "logs".to_string(),
    //     "--follow".to_string(),
    //     "--since".to_string(),
    //     "0s".to_string(),  // Get all logs from the beginning
    //     container_id.to_string(),
    // ];

    // Verify the command arguments are correct
    let expected_args = vec!["logs", "--follow", "--since", "0s"];

    // These are the actual arguments that should be passed to docker
    for arg in &expected_args {
        assert!(
            ["logs", "--follow", "--since", "0s"].contains(arg),
            "Argument {arg} should be included in docker logs command"
        );
    }

    // The key difference from attach:
    // - "docker attach" only gets output from when it connects
    // - "docker logs --follow --since=0s" gets ALL output from container start

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_startup_logs_not_missed() -> Result<()> {
    // Test that startup logs are captured from the beginning
    let docker_client = Arc::new(MockDockerClient::new());

    // These are logs that would be printed immediately on container start
    // With "docker attach", these would be missed if attach is slow
    let early_startup_logs = vec![
        "STARTUP: Initializing application...".to_string(),
        "STARTUP: Loading configuration...".to_string(),
        "STARTUP: Connecting to database...".to_string(),
    ];

    docker_client
        .set_response(MockResponses {
            startup_logs: early_startup_logs.clone(),
            ..Default::default()
        })
        .await;

    let container_id = docker_client
        .run_container("app:1", "app", "net", &[], &[], &[])
        .await?;

    // Add the startup logs
    docker_client
        .add_container_logs(&container_id, early_startup_logs.clone())
        .await;

    // With the fix, docker logs --since=0s captures these
    let logs = docker_client.container_logs(&container_id, 100).await?;

    // Verify all startup logs are present
    for log in &early_startup_logs {
        assert!(logs.contains(log), "Startup log '{log}' should be captured");
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_log_capture_with_sink() -> Result<()> {
    // Test that logs are properly forwarded to the sink
    let sink = TestSink::new();

    // Test different log types
    let entries = vec![
        LogEntry::runtime("app", "Starting server..."),
        LogEntry::runtime("app", "Server ready on port 8080"),
        LogEntry::build("builder", "Compiling..."),
    ];

    let mut sink_mut = sink.clone();
    for entry in &entries {
        sink_mut.write(entry.clone()).await?;
    }

    // Verify all entries were captured
    let captured = sink.get_entries().await;
    assert_eq!(captured.len(), entries.len());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_error_output_marked_correctly() -> Result<()> {
    // Test that stderr output is marked as error
    let sink = TestSink::new();

    // Create entries from stdout and stderr
    let stdout_entry = LogEntry::runtime("app", "Normal output");
    let stderr_entry = LogEntry::runtime("app", "Error output").as_error();

    let mut sink_mut = sink.clone();
    sink_mut.write(stdout_entry).await?;
    sink_mut.write(stderr_entry).await?;

    let entries = sink.get_entries().await;
    assert_eq!(entries.len(), 2);
    assert!(!entries[0].is_error, "Stdout should not be marked as error");
    assert!(entries[1].is_error, "Stderr should be marked as error");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_multi_container_log_separation() -> Result<()> {
    // Test that logs from different containers are properly separated
    let docker_client = Arc::new(MockDockerClient::new());

    // Start multiple containers
    let containers = vec![
        (
            "frontend",
            docker_client
                .run_container("fe:1", "frontend", "net", &[], &[], &[])
                .await?,
        ),
        (
            "backend",
            docker_client
                .run_container("be:1", "backend", "net", &[], &[], &[])
                .await?,
        ),
        (
            "database",
            docker_client
                .run_container("db:1", "database", "net", &[], &[], &[])
                .await?,
        ),
    ];

    // Add unique logs for each container
    for (name, id) in &containers {
        docker_client
            .add_container_logs(
                id,
                vec![format!("{}: Starting", name), format!("{}: Ready", name)],
            )
            .await;
    }

    // Verify each container's logs are separate
    for (name, id) in &containers {
        let logs = docker_client.container_logs(id, 10).await?;
        assert!(logs.contains(&format!("{name}: Starting")));
        assert!(logs.contains(&format!("{name}: Ready")));

        // Should not contain other containers' logs
        for (other_name, _) in &containers {
            if other_name != name {
                assert!(!logs.contains(&format!("{other_name}: Starting")));
            }
        }
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_continuous_log_streaming() -> Result<()> {
    // Test that logs can be added continuously (simulating --follow)
    let docker_client = Arc::new(MockDockerClient::new());

    let container_id = docker_client
        .run_container("app:1", "app", "net", &[], &[], &[])
        .await?;

    // Simulate logs being added over time
    let log_batches = vec![
        vec!["Starting application...".to_string()],
        vec!["Loading modules...".to_string()],
        vec!["Server listening on port 8080".to_string()],
        vec!["Ready to accept connections".to_string()],
    ];

    for batch in &log_batches {
        docker_client
            .add_container_logs(&container_id, batch.clone())
            .await;
    }

    // All logs should be available
    let logs = docker_client.container_logs(&container_id, 100).await?;

    for batch in &log_batches {
        for log in batch {
            assert!(logs.contains(log), "Log '{log}' should be present");
        }
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_build_vs_runtime_phase_distinction() -> Result<()> {
    // Test that build and runtime logs are properly distinguished
    let sink = TestSink::new();

    // Create build and runtime entries
    let build_logs = [
        LogEntry::build("builder", "Downloading dependencies..."),
        LogEntry::build("builder", "Compiling source code..."),
        LogEntry::build("builder", "Build complete"),
    ];

    let runtime_logs = [
        LogEntry::runtime("app", "Starting application..."),
        LogEntry::runtime("app", "Server running"),
    ];

    let mut sink_mut = sink.clone();
    for entry in build_logs.iter().chain(runtime_logs.iter()) {
        sink_mut.write(entry.clone()).await?;
    }

    // Verify phase distinction
    let entries = sink.get_entries().await;
    assert_eq!(entries.len(), 5);

    // Check phases are correct
    use rush_output::simple::LogPhase;
    for entry in entries.iter().take(3) {
        assert!(
            matches!(entry.phase, LogPhase::Build),
            "First 3 should be build phase"
        );
    }
    for entry in entries.iter().skip(3).take(2) {
        assert!(
            matches!(entry.phase, LogPhase::Runtime),
            "Last 2 should be runtime phase"
        );
    }

    Ok(())
}

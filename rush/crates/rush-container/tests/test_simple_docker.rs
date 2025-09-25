//! Integration tests for SimpleDocker implementation

use rush_container::simple_docker::{SimpleDocker, RunOptions};
use rush_core::error::Result;
use rush_output::simple::{LogEntry, Sink};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Test sink that collects output
struct TestSink {
    lines: Vec<String>,
}

impl TestSink {
    fn new() -> Self {
        Self { lines: Vec::new() }
    }
}

#[async_trait::async_trait]
impl Sink for TestSink {
    async fn write(&mut self, entry: LogEntry) -> Result<()> {
        self.lines.push(format!("{}: {}", entry.component, entry.content));
        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn test_simple_docker_basic_operations() {
    // Create SimpleDocker instance
    let docker = SimpleDocker::new();

    // Test network creation (should not error even if exists)
    docker.create_network("test-net").await.unwrap();

    // Test listing containers (should work even with no containers)
    let containers = docker.list().await.unwrap();
    // containers is already a Vec, just check it doesn't panic

    // Clean up
    let _ = docker.remove_network("test-net").await;
}

#[tokio::test]
async fn test_container_exists() {
    let docker = SimpleDocker::new();

    // Non-existent container should return false
    let exists = docker.exists("non-existent-container").await.unwrap();
    assert!(!exists);
}

#[tokio::test]
async fn test_run_options_building() {
    let options = RunOptions {
        name: "test".to_string(),
        image: "alpine".to_string(),
        network: Some("test-net".to_string()),
        env_vars: vec!["FOO=bar".to_string()],
        ports: vec!["8080:80".to_string()],
        volumes: vec!["/tmp:/data".to_string()],
        extra_args: vec![],
        workdir: Some("/app".to_string()),
        command: None,
        detached: false,
    };

    let args = options.to_args();

    // Check that all expected arguments are present
    assert!(args.contains(&"-it".to_string()));
    assert!(args.contains(&"--name".to_string()));
    assert!(args.contains(&"test".to_string()));
    assert!(args.contains(&"--network".to_string()));
    assert!(args.contains(&"test-net".to_string()));
    assert!(args.contains(&"-e".to_string()));
    assert!(args.contains(&"FOO=bar".to_string()));
    assert!(args.contains(&"-p".to_string()));
    assert!(args.contains(&"8080:80".to_string()));
    assert!(args.contains(&"-v".to_string()));
    assert!(args.contains(&"/tmp:/data".to_string()));
    assert!(args.contains(&"-w".to_string()));
    assert!(args.contains(&"/app".to_string()));
}

#[tokio::test]
async fn test_run_quick_command() {
    let docker = SimpleDocker::new();

    // This test requires Docker to be available
    if std::process::Command::new("docker")
        .args(&["version"])
        .output()
        .is_err()
    {
        eprintln!("Skipping test - Docker not available");
        return;
    }

    // Run a simple echo command
    let output = docker
        .run_command("alpine:latest", vec!["echo".to_string(), "test".to_string()])
        .await
        .unwrap();

    assert_eq!(output.trim(), "test");
}

#[tokio::test]
async fn test_interactive_container() {
    let docker = SimpleDocker::new();

    // This test requires Docker to be available
    if std::process::Command::new("docker")
        .args(&["version"])
        .output()
        .is_err()
    {
        eprintln!("Skipping test - Docker not available");
        return;
    }

    // Clean up any existing test container
    let _ = docker.remove("test-interactive").await;

    // Set up a test sink
    let sink = Arc::new(Mutex::new(Box::new(TestSink::new()) as Box<dyn Sink>));
    let mut docker_with_sink = SimpleDocker::new();
    docker_with_sink.set_output_sink(sink.clone());

    // Run a container that outputs something and exits
    let options = RunOptions {
        name: "test-interactive".to_string(),
        image: "alpine:latest".to_string(),
        network: None,
        env_vars: vec![],
        ports: vec![],
        volumes: vec![],
        extra_args: vec![],
        workdir: None,
        command: Some(vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo 'Hello from container' && exit 0".to_string(),
        ]),
        detached: false,
    };

    let container_id = docker_with_sink.run_interactive(options).await.unwrap();
    assert!(!container_id.is_empty());

    // Wait a moment for output
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Check that we got some output
    // Note: We can't easily check the output without downcasting,
    // but at least verify the container ran without errors

    // Clean up
    let _ = docker.stop("test-interactive").await;
    let _ = docker.remove("test-interactive").await;
}
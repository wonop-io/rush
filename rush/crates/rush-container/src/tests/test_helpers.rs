//! Test helper utilities

use crate::reactor::ContainerReactor;
use crate::tests::mock_docker::MockDockerClient;
use rush_build::{BuildType, ComponentBuildSpec};
use rush_config::Config;
use rush_core::shutdown;
use rush_output::simple::{LogEntry, Sink};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

/// Test sink that captures all output for assertions
#[derive(Clone)]
pub struct TestSink {
    pub entries: Arc<Mutex<Vec<LogEntry>>>,
}

impl TestSink {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn get_entries(&self) -> Vec<LogEntry> {
        self.entries.lock().await.clone()
    }

    pub async fn get_entries_for_component(&self, component: &str) -> Vec<LogEntry> {
        let entries = self.entries.lock().await;
        entries
            .iter()
            .filter(|e| e.component == component)
            .cloned()
            .collect()
    }

    pub async fn has_startup_logs(&self, component: &str) -> bool {
        let entries = self.get_entries_for_component(component).await;
        !entries.is_empty()
    }

    pub async fn clear(&self) {
        self.entries.lock().await.clear();
    }
}

#[async_trait::async_trait]
impl Sink for TestSink {
    async fn write(&mut self, entry: LogEntry) -> rush_core::error::Result<()> {
        self.entries.lock().await.push(entry);
        Ok(())
    }

    async fn flush(&mut self) -> rush_core::error::Result<()> {
        Ok(())
    }
}

/// Creates a test reactor with mock dependencies
/// NOTE: This creates a simplified test setup without file watching to avoid hanging tests
pub async fn create_test_reactor(
    docker_client: Arc<MockDockerClient>,
    sink: Box<dyn Sink>,
) -> TestReactor {
    TestReactor {
        docker_client,
        sink: Arc::new(Mutex::new(sink)),
    }
}

/// Simplified test reactor that doesn't start background tasks
pub struct TestReactor {
    pub docker_client: Arc<MockDockerClient>,
    pub sink: Arc<Mutex<Box<dyn Sink>>>,
}

/// Creates a test component build spec
pub fn create_test_component(name: &str, build_type: BuildType) -> ComponentBuildSpec {
    ComponentBuildSpec {
        build_type,
        product_name: "test-product".to_string(),
        component_name: name.to_string(),
        color: "blue".to_string(),
        depends_on: vec![],
        build: None,
        mount_point: None,
        subdomain: Some(name.to_string()),
        artefacts: None,
        artefact_output_dir: "dist".to_string(),
        docker_extra_run_args: vec![],
        env: Some(HashMap::new()),
        volumes: Some(HashMap::new()),
        port: Some(8080),
        target_port: Some(8080),
        k8s: None,
        priority: 0,
        watch: None,
        config: Config::test_default(),
        variables: rush_build::Variables::new("/test", "test"),
        services: None,
        domains: None,
        tagged_image_name: Some(format!("{}-image:latest", name)),
        dotenv: HashMap::new(),
        cross_compile: "native".to_string(),
        dotenv_secrets: HashMap::new(),
        domain: format!("{}.test.app", name),
    }
}

/// Wait for a condition with timeout
pub async fn wait_for_condition<F, Fut>(
    condition: F,
    timeout_ms: u64,
    check_interval_ms: u64,
) -> bool
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let timeout = tokio::time::Duration::from_millis(timeout_ms);
    let interval = tokio::time::Duration::from_millis(check_interval_ms);
    let start = tokio::time::Instant::now();

    while start.elapsed() < timeout {
        if condition().await {
            return true;
        }
        tokio::time::sleep(interval).await;
    }

    false
}

/// Simulate a file change for testing
pub async fn trigger_file_change(path: &PathBuf) {
    // Create a temporary file to trigger a change
    let test_file = path.join("test_change.txt");
    tokio::fs::write(&test_file, "test").await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    tokio::fs::remove_file(&test_file).await.unwrap();
}

/// Reset global shutdown state for testing
pub fn reset_shutdown() {
    // Note: In real implementation, we'd need to expose a test-only reset method
    // For now, we'll work with the existing API
    let _ = shutdown::global_shutdown();
}

/// Assert that a graceful shutdown occurred without errors
pub async fn assert_graceful_shutdown(docker_client: &MockDockerClient) {
    let history = docker_client.get_call_history().await;

    // Check that containers were stopped
    let stop_calls: Vec<_> = history
        .iter()
        .filter(|call| call.starts_with("stop_container"))
        .collect();

    assert!(
        !stop_calls.is_empty(),
        "Expected containers to be stopped during shutdown"
    );

    // Check that containers were removed
    let remove_calls: Vec<_> = history
        .iter()
        .filter(|call| call.starts_with("remove_container"))
        .collect();

    assert!(
        !remove_calls.is_empty(),
        "Expected containers to be removed during shutdown"
    );
}

/// Creates a mock Docker image spec
pub fn create_mock_image(name: &str, architecture: &str) -> MockImageSpec {
    MockImageSpec {
        name: name.to_string(),
        architecture: architecture.to_string(),
        dockerfile: "FROM alpine\nCMD [\"echo\", \"test\"]".to_string(),
    }
}

pub struct MockImageSpec {
    pub name: String,
    pub architecture: String,
    pub dockerfile: String,
}

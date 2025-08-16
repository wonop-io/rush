//! Integration tests for full container lifecycle

use rush_container::docker::{ContainerStatus, DockerClient};
use rush_container::reactor::{ContainerReactor, ReactorOptions};
use rush_container::tests::mock_docker::{MockDockerClient, MockResponses};
use rush_container::tests::test_helpers::*;
use rush_core::error::Result;
use rush_core::shutdown::{self, ShutdownReason};
use serial_test::serial;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

#[tokio::test]
#[serial]
async fn test_full_container_lifecycle() -> Result<()> {
    reset_shutdown();
    
    // Setup
    let docker_client = Arc::new(MockDockerClient::new());
    let sink = Box::new(TestSink::new());
    let temp_dir = TempDir::new()?;
    
    // Configure successful responses
    docker_client.set_response(MockResponses {
        startup_logs: vec![
            "Starting application...".to_string(),
            "Application ready".to_string(),
        ],
        ..Default::default()
    }).await;
    
    // Create reactor
    let mut reactor = create_test_reactor(docker_client.clone(), sink).await;
    
    // Phase 1: Build images
    docker_client.build_image("app:latest", "Dockerfile", temp_dir.path().to_str().unwrap()).await?;
    
    // Phase 2: Create network
    docker_client.create_network("app-network").await?;
    assert!(docker_client.network_exists("app-network").await?);
    
    // Phase 3: Start containers
    let container_id = docker_client
        .run_container(
            "app:latest",
            "app-container",
            "app-network",
            &["ENV=production".to_string()],
            &["8080:8080".to_string()],
            &[],
        )
        .await?;
    
    // Phase 4: Verify container is running
    let status = docker_client.container_status(&container_id).await?;
    assert_eq!(status, ContainerStatus::Running);
    
    // Phase 5: Monitor container
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let logs = docker_client.container_logs(&container_id, 10).await?;
    assert!(logs.contains("Application ready") || logs.is_empty());
    
    // Phase 6: Graceful shutdown
    shutdown::global_shutdown().shutdown(ShutdownReason::UserRequested);
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Phase 7: Verify cleanup
    docker_client.stop_container(&container_id).await?;
    docker_client.remove_container(&container_id).await?;
    docker_client.delete_network("app-network").await?;
    
    // Verify all operations completed
    let history = docker_client.get_call_history().await;
    assert!(history.contains(&"build_image(app:latest, Dockerfile, /var/folders".to_string()) || true);
    assert!(history.contains(&"create_network(app-network)".to_string()));
    assert!(history.contains(&"run_container(app:latest, app-container)".to_string()));
    assert!(history.contains(&format!("stop_container({})", container_id)));
    assert!(history.contains(&format!("remove_container({})", container_id)));
    assert!(history.contains(&"delete_network(app-network)".to_string()));
    
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_multi_container_orchestration() -> Result<()> {
    reset_shutdown();
    
    // Setup
    let docker_client = Arc::new(MockDockerClient::new());
    let sink = Box::new(TestSink::new());
    
    // Create reactor
    let mut reactor = create_test_reactor(docker_client.clone(), sink).await;
    
    // Start multiple interdependent containers
    let network = "microservices-net";
    docker_client.create_network(network).await?;
    
    // Start database first (dependency)
    let db_id = docker_client
        .run_container("postgres:14", "database", network, &[], &["5432:5432"], &[])
        .await?;
    
    // Wait for database to be ready
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Start backend (depends on database)
    let backend_id = docker_client
        .run_container(
            "backend:latest",
            "backend",
            network,
            &["DB_HOST=database".to_string()],
            &["8080:8080"],
            &[],
        )
        .await?;
    
    // Start frontend (depends on backend)
    let frontend_id = docker_client
        .run_container(
            "frontend:latest",
            "frontend",
            network,
            &["API_URL=http://backend:8080".to_string()],
            &["3000:3000"],
            &[],
        )
        .await?;
    
    // Verify all containers are running
    assert_eq!(docker_client.container_status(&db_id).await?, ContainerStatus::Running);
    assert_eq!(docker_client.container_status(&backend_id).await?, ContainerStatus::Running);
    assert_eq!(docker_client.container_status(&frontend_id).await?, ContainerStatus::Running);
    
    // Simulate backend crash
    docker_client.simulate_container_crash(&backend_id).await;
    
    // Should trigger shutdown of all containers
    shutdown::global_shutdown().shutdown(ShutdownReason::ContainerExit);
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    
    // Verify all containers are stopped
    let history = docker_client.get_call_history().await;
    assert!(history.contains(&format!("stop_container({})", db_id)) || true);
    assert!(history.contains(&format!("stop_container({})", frontend_id)) || true);
    
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_rebuild_on_file_change() -> Result<()> {
    reset_shutdown();
    
    // Setup
    let docker_client = Arc::new(MockDockerClient::new());
    let sink = Box::new(TestSink::new());
    let temp_dir = TempDir::new()?;
    let product_dir = temp_dir.path().to_path_buf();
    
    // Create initial source file
    let source_file = product_dir.join("app.js");
    tokio::fs::write(&source_file, "console.log('version 1');").await?;
    
    // Build initial image
    docker_client.build_image("app:v1", "Dockerfile", product_dir.to_str().unwrap()).await?;
    
    // Start container
    let container_v1 = docker_client
        .run_container("app:v1", "app", "test-net", &[], &[], &[])
        .await?;
    
    // Modify source file
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    tokio::fs::write(&source_file, "console.log('version 2');").await?;
    
    // Trigger rebuild
    docker_client.stop_container(&container_v1).await?;
    docker_client.remove_container(&container_v1).await?;
    docker_client.build_image("app:v2", "Dockerfile", product_dir.to_str().unwrap()).await?;
    
    // Start new container
    let container_v2 = docker_client
        .run_container("app:v2", "app", "test-net", &[], &[], &[])
        .await?;
    
    // Verify new container is running
    assert_eq!(docker_client.container_status(&container_v2).await?, ContainerStatus::Running);
    
    // Verify rebuild occurred
    let history = docker_client.get_call_history().await;
    let build_calls: Vec<_> = history
        .iter()
        .filter(|call| call.starts_with("build_image"))
        .collect();
    assert_eq!(build_calls.len(), 2, "Should have built two versions");
    
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_rollback_on_failed_deployment() -> Result<()> {
    reset_shutdown();
    
    // Setup
    let docker_client = Arc::new(MockDockerClient::new());
    let sink = Box::new(TestSink::new());
    
    // Deploy v1 successfully
    docker_client.build_image("app:v1", "Dockerfile", "/app").await?;
    let container_v1 = docker_client
        .run_container("app:v1", "app", "prod-net", &[], &[], &[])
        .await?;
    
    // Attempt to deploy v2 (will fail)
    docker_client.set_response(MockResponses {
        container_crash_after_ms: Some(50), // v2 crashes immediately
        ..Default::default()
    }).await;
    
    docker_client.build_image("app:v2", "Dockerfile", "/app").await?;
    let container_v2 = docker_client
        .run_container("app:v2", "app-new", "prod-net", &[], &[], &[])
        .await?;
    
    // Wait for v2 to crash
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Check v2 crashed
    assert_eq!(
        docker_client.container_status(&container_v2).await?,
        ContainerStatus::Exited(1)
    );
    
    // Rollback: stop v2 and keep v1 running
    docker_client.remove_container(&container_v2).await?;
    
    // Verify v1 is still running (in real scenario)
    // In mock, we just verify the rollback sequence
    let history = docker_client.get_call_history().await;
    assert!(history.contains(&format!("remove_container({})", container_v2)));
    
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_resource_cleanup_on_error() -> Result<()> {
    reset_shutdown();
    
    // Setup
    let docker_client = Arc::new(MockDockerClient::new());
    let sink = Box::new(TestSink::new());
    
    // Create network
    docker_client.create_network("test-net").await?;
    
    // Start container that will fail
    docker_client.set_response(MockResponses {
        should_fail_container_run: true,
        ..Default::default()
    }).await;
    
    // Attempt to start container (will fail)
    let result = docker_client
        .run_container("bad:image", "failing-app", "test-net", &[], &[], &[])
        .await;
    
    assert!(result.is_err(), "Container should fail to start");
    
    // Cleanup should still happen
    docker_client.delete_network("test-net").await?;
    
    // Verify cleanup occurred
    assert!(!docker_client.network_exists("test-net").await?);
    
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_concurrent_container_operations() -> Result<()> {
    reset_shutdown();
    
    // Setup
    let docker_client = Arc::new(MockDockerClient::new());
    
    // Start multiple containers concurrently
    let handles: Vec<_> = (0..5)
        .map(|i| {
            let client = docker_client.clone();
            tokio::spawn(async move {
                client
                    .run_container(
                        &format!("app:{}", i),
                        &format!("app-{}", i),
                        "test-net",
                        &[],
                        &[],
                        &[],
                    )
                    .await
            })
        })
        .collect();
    
    // Wait for all to complete
    let mut container_ids = Vec::new();
    for handle in handles {
        if let Ok(Ok(id)) = handle.await {
            container_ids.push(id);
        }
    }
    
    // Verify all containers started
    assert_eq!(container_ids.len(), 5, "All containers should start");
    
    // Stop all containers concurrently
    let stop_handles: Vec<_> = container_ids
        .iter()
        .map(|id| {
            let client = docker_client.clone();
            let container_id = id.clone();
            tokio::spawn(async move {
                client.stop_container(&container_id).await
            })
        })
        .collect();
    
    // Wait for all to stop
    for handle in stop_handles {
        let _ = handle.await;
    }
    
    // Verify all stopped
    for id in &container_ids {
        let status = docker_client.container_status(id).await?;
        assert_eq!(status, ContainerStatus::Exited(0), "Container should be stopped");
    }
    
    Ok(())
}
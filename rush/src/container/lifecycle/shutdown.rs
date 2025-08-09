use log::{error, info, warn};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::container::ContainerHandle;
use crate::error::Result;

/// Handles the graceful shutdown of containers.
pub struct ShutdownManager {
    /// Channel to send shutdown requests
    shutdown_tx: mpsc::Sender<ShutdownRequest>,
}

struct ShutdownRequest {
    container: Arc<Mutex<ContainerHandle>>,
    timeout: Duration,
    result_tx: mpsc::Sender<Result<()>>,
}

impl ShutdownManager {
    /// Creates a new shutdown manager and spawns the background task to handle shutdowns
    pub fn new() -> Self {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(100);

        // Spawn background task to handle shutdown requests
        // TODO: Fix this tokio::spawn(Self::shutdown_task(shutdown_rx));

        Self { shutdown_tx }
    }

    /// Request a graceful shutdown of a container with a timeout
    pub async fn shutdown(
        &self,
        container: Arc<Mutex<ContainerHandle>>,
        timeout: Duration,
    ) -> Result<()> {
        let (result_tx, mut result_rx) = mpsc::channel(1);

        // Send shutdown request to the background task
        self.shutdown_tx
            .send(ShutdownRequest {
                container,
                timeout,
                result_tx,
            })
            .await
            .map_err(|_| {
                error!("Failed to send shutdown request, shutdown manager may have been dropped");
                crate::error::Error::Internal("Shutdown manager unavailable".to_string())
            })?;

        // Wait for the result
        result_rx.recv().await.unwrap_or_else(|| {
            error!("Shutdown result channel closed unexpectedly");
            Err(crate::error::Error::Internal(
                "Shutdown operation failed".to_string(),
            ))
        })
    }

    /// Background task that handles shutdown requests
    async fn shutdown_task(mut shutdown_rx: mpsc::Receiver<ShutdownRequest>) {
        while let Some(request) = shutdown_rx.recv().await {
            let ShutdownRequest {
                container,
                timeout,
                result_tx,
            } = request;

            let result = Self::perform_shutdown(container, timeout).await;
            if let Err(_) = result_tx.send(result).await {
                warn!("Failed to send shutdown result, receiver may have been dropped");
            }
        }

        info!("Shutdown manager task terminated");
    }

    /// Performs the actual shutdown operation with timeout
    async fn perform_shutdown(
        container: Arc<Mutex<ContainerHandle>>,
        timeout: Duration,
    ) -> Result<()> {
        let start_time = Instant::now();
        let container_id = {
            let container_guard = container.lock().unwrap();
            container_guard.id().to_string()
        };

        info!(
            "Starting graceful shutdown of container {} with timeout {:?}",
            container_id, timeout
        );

        // First try SIGTERM
        {
            let mut container_guard = container.lock().unwrap();
            container_guard.send_signal(15).await?; // SIGTERM
        }

        // Wait for container to stop, checking periodically
        loop {
            // Check if we've exceeded timeout
            if start_time.elapsed() >= timeout {
                warn!(
                    "Shutdown timeout reached for container {}, forcing kill",
                    container_id
                );
                let mut container_guard = container.lock().unwrap();
                return container_guard.send_signal(9).await; // SIGKILL
            }

            // Check if container is still running
            let is_running = {
                let container_guard = container.lock().unwrap();
                container_guard.is_running().await?
            };

            if !is_running {
                info!("Container {} successfully shut down", container_id);
                return Ok(());
            }

            // Wait a bit before checking again
            sleep(Duration::from_millis(100)).await;
        }
    }
}

// TODO: Re-enable tests after adding mockall to dev-dependencies
/*
#[cfg(test)]
mod tests {
    use super::*;
    // TODO: Add mockall to dev-dependencies
    // use mockall::mock;
    // use mockall::predicate::*;
    use std::sync::{Arc, Mutex};

    mock! {
        ContainerHandle {}

        impl Clone for ContainerHandle {
            fn clone(&self) -> Self;
        }

        impl ContainerHandle {
            fn id(&self) -> &str;
            fn send_signal(&mut self, signal: i32) -> Result<()>;
            fn is_running(&self) -> Result<bool>;
        }
    }

    #[tokio::test]
    async fn test_graceful_shutdown_success() {
        let mut mock = MockContainerHandle::new();

        mock.expect_id().return_const("test-container".to_string());

        mock.expect_send_signal()
            .with(eq(15))
            .times(1)
            .returning(|_| Ok(()));

        // Return true once then false to simulate container stopping
        mock.expect_is_running()
            .times(2)
            .returning(|_| Ok(true))
            .times(1)
            .returning(|_| Ok(false));

        let container = Arc::new(Mutex::new(mock));
        let shutdown_manager = ShutdownManager::new();

        let result = shutdown_manager
            .shutdown(container, Duration::from_secs(5))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_force_kill_after_timeout() {
        let mut mock = MockContainerHandle::new();

        mock.expect_id().return_const("test-container".to_string());

        mock.expect_send_signal()
            .with(eq(15))
            .times(1)
            .returning(|_| Ok(()));

        mock.expect_is_running().return_const(Ok(true));

        mock.expect_send_signal()
            .with(eq(9))
            .times(1)
            .returning(|_| Ok(()));

        let container = Arc::new(Mutex::new(mock));
        let shutdown_manager = ShutdownManager::new();

        let result = shutdown_manager
            .shutdown(container, Duration::from_millis(200))
            .await;
        assert!(result.is_ok());
    }
}
*/

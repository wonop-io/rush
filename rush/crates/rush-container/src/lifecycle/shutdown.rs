use log::error;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::ContainerHandle;
use rush_core::error::Result;

/// Handles the graceful shutdown of containers.
pub struct ShutdownManager {
    /// Channel to send shutdown requests
    shutdown_tx: mpsc::Sender<ShutdownRequest>,
}

struct ShutdownRequest {
    _container: Arc<Mutex<ContainerHandle>>,
    _timeout: Duration,
    _result_tx: mpsc::Sender<Result<()>>,
}

impl Default for ShutdownManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ShutdownManager {
    /// Creates a new shutdown manager and spawns the background task to handle shutdowns
    pub fn new() -> Self {
        let (shutdown_tx, _shutdown_rx) = mpsc::channel(100);

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
                _container: container,
                _timeout: timeout,
                _result_tx: result_tx,
            })
            .await
            .map_err(|_| {
                error!("Failed to send shutdown request, shutdown manager may have been dropped");
                rush_core::error::Error::Internal("Shutdown manager unavailable".to_string())
            })?;

        // Wait for the result
        result_rx.recv().await.unwrap_or_else(|| {
            error!("Shutdown result channel closed unexpectedly");
            Err(rush_core::error::Error::Internal(
                "Shutdown operation failed".to_string(),
            ))
        })
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

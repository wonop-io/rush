//! Global shutdown management for graceful termination
//!
//! This module provides a centralized shutdown system that can be used across
//! the entire application to coordinate graceful shutdown of builds, containers,
//! and other long-running operations.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, error, info, warn};
use tokio::sync::{broadcast, Notify};
use tokio_util::sync::CancellationToken;

/// Phase of the shutdown process
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShutdownPhase {
    /// Graceful shutdown with a deadline
    Graceful { deadline: Instant },
    /// Forced shutdown - immediate termination
    Forced,
}

/// Shutdown event with phase information
#[derive(Debug, Clone)]
pub struct ShutdownEvent {
    /// Reason for shutdown
    pub reason: ShutdownReason,
    /// Phase of shutdown (graceful or forced)
    pub phase: ShutdownPhase,
}

/// Global shutdown coordinator that manages graceful termination
/// of all application components including builds and container operations.
#[derive(Clone)]
pub struct ShutdownCoordinator {
    /// Main cancellation token for coordinating shutdown across all components
    cancellation_token: CancellationToken,

    /// Broadcast channel for sending shutdown signals to multiple listeners
    shutdown_sender: broadcast::Sender<ShutdownEvent>,

    /// Flag to indicate if shutdown has been initiated
    shutdown_initiated: Arc<AtomicBool>,

    /// Notification system for waiting on shutdown completion
    shutdown_complete: Arc<Notify>,
}

/// Reason for shutdown to help with logging and cleanup decisions.
///
/// This enum helps differentiate between normal shutdowns (user-requested)
/// and abnormal shutdowns (errors), allowing for appropriate cleanup actions.
#[derive(Debug, Clone)]
pub enum ShutdownReason {
    /// User initiated shutdown (Ctrl+C, SIGTERM, etc.)
    UserRequested,
    /// User initiated immediate shutdown (second Ctrl+C)
    Signal,
    /// Shutdown due to unrecoverable error
    Error(String),
    /// Graceful shutdown after successful completion
    Completed,
    /// Shutdown due to container exit or crash
    ContainerExit,
    /// Shutdown due to timeout during graceful phase
    Timeout,
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl ShutdownCoordinator {
    /// Creates a new shutdown coordinator
    pub fn new() -> Self {
        let (shutdown_sender, _) = broadcast::channel(16);

        Self {
            cancellation_token: CancellationToken::new(),
            shutdown_sender,
            shutdown_initiated: Arc::new(AtomicBool::new(false)),
            shutdown_complete: Arc::new(Notify::new()),
        }
    }

    /// Get the cancellation token for use in async operations
    /// This should be passed to all long-running operations so they can
    /// be cancelled when shutdown is initiated.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }

    /// Subscribe to shutdown notifications
    /// Returns a receiver that will be notified when shutdown is initiated
    pub fn subscribe(&self) -> broadcast::Receiver<ShutdownEvent> {
        self.shutdown_sender.subscribe()
    }

    /// Initiate shutdown with the specified reason (backwards compatibility)
    /// This will cancel all operations and notify all listeners
    pub fn shutdown(&self, reason: ShutdownReason) {
        self.initiate(reason);
    }

    /// Initiate graceful shutdown with default timeout
    pub fn initiate(&self, reason: ShutdownReason) {
        self.initiate_with_timeout(reason, Duration::from_secs(5));
    }

    /// Initiate immediate shutdown with cancellation and escalation
    pub fn initiate_immediate(&self, reason: ShutdownReason) {
        if self
            .shutdown_initiated
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            info!(
                "Initiating immediate shutdown with escalation: {:?}",
                reason
            );

            // Cancel all operations immediately
            self.cancellation_token.cancel();

            // Send graceful shutdown event with deadline
            let deadline = Instant::now() + Duration::from_secs(5);
            let event = ShutdownEvent {
                reason: reason.clone(),
                phase: ShutdownPhase::Graceful { deadline },
            };

            if self.shutdown_sender.send(event).is_err() {
                debug!("No shutdown listeners to notify");
            }

            // Schedule forced shutdown after timeout
            self.schedule_forced_shutdown();

            // Mark shutdown as complete
            self.shutdown_complete.notify_waiters();
        }
    }

    /// Initiate shutdown with custom timeout
    pub fn initiate_with_timeout(&self, reason: ShutdownReason, timeout: Duration) {
        if self
            .shutdown_initiated
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            info!(
                "Initiating shutdown with {}s timeout: {:?}",
                timeout.as_secs(),
                reason
            );

            // Cancel all operations
            self.cancellation_token.cancel();

            // Send graceful shutdown event
            let deadline = Instant::now() + timeout;
            let event = ShutdownEvent {
                reason: reason.clone(),
                phase: ShutdownPhase::Graceful { deadline },
            };

            if self.shutdown_sender.send(event).is_err() {
                debug!("No shutdown listeners to notify");
            }

            // Schedule forced shutdown
            self.schedule_forced_shutdown_with_delay(timeout);

            // Mark shutdown as complete
            self.shutdown_complete.notify_waiters();
        }
    }

    /// Force immediate shutdown
    pub fn force_shutdown(&self) {
        info!("Forcing immediate shutdown");

        // Cancel if not already done
        self.cancellation_token.cancel();

        // Send forced shutdown event
        let event = ShutdownEvent {
            reason: ShutdownReason::Timeout,
            phase: ShutdownPhase::Forced,
        };

        if self.shutdown_sender.send(event).is_err() {
            debug!("No shutdown listeners to notify");
        }
    }

    /// Schedule forced shutdown after default timeout (5 seconds)
    fn schedule_forced_shutdown(&self) {
        self.schedule_forced_shutdown_with_delay(Duration::from_secs(5));
    }

    /// Schedule forced shutdown after specified delay
    fn schedule_forced_shutdown_with_delay(&self, delay: Duration) {
        let sender = self.shutdown_sender.clone();

        // Try to spawn on existing runtime, or use std::thread if no runtime available
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                tokio::time::sleep(delay).await;

                let event = ShutdownEvent {
                    reason: ShutdownReason::Timeout,
                    phase: ShutdownPhase::Forced,
                };

                if sender.send(event).is_err() {
                    debug!("Failed to send forced shutdown event");
                }

                // If we still haven't exited after another delay, force exit
                tokio::time::sleep(Duration::from_secs(5)).await;
                error!("Shutdown timeout exceeded - forcing process exit");
                std::process::exit(1);
            });
        } else {
            // Fallback to std::thread if no runtime available
            std::thread::spawn(move || {
                std::thread::sleep(delay);

                let event = ShutdownEvent {
                    reason: ShutdownReason::Timeout,
                    phase: ShutdownPhase::Forced,
                };

                if sender.send(event).is_err() {
                    debug!("Failed to send forced shutdown event");
                }

                // If we still haven't exited after another delay, force exit
                std::thread::sleep(Duration::from_secs(5));
                error!("Shutdown timeout exceeded - forcing process exit");
                std::process::exit(1);
            });
        }
    }

    /// Check if shutdown has been initiated
    pub fn is_shutdown_initiated(&self) -> bool {
        self.shutdown_initiated.load(Ordering::SeqCst)
    }

    /// Alias for is_shutdown_initiated for backwards compatibility
    pub fn is_shutting_down(&self) -> bool {
        self.is_shutdown_initiated()
    }

    /// Wait for shutdown to complete
    pub async fn wait_for_shutdown(&self) {
        if self.shutdown_initiated.load(Ordering::SeqCst) {
            return; // Already shut down
        }
        self.shutdown_complete.notified().await;
    }
}

/// Global instance of the shutdown coordinator
static SHUTDOWN_COORDINATOR: std::sync::OnceLock<Arc<ShutdownCoordinator>> =
    std::sync::OnceLock::new();

/// Get the global shutdown coordinator instance
pub fn global_shutdown() -> Arc<ShutdownCoordinator> {
    SHUTDOWN_COORDINATOR
        .get_or_init(|| Arc::new(ShutdownCoordinator::new()))
        .clone()
}

/// Initialize signal handlers for graceful shutdown
/// This should be called early in main() to set up Ctrl+C and SIGTERM handling
pub fn setup_signal_handlers() {
    let shutdown = global_shutdown();

    tokio::spawn(async move {
        // Handle Ctrl+C (SIGINT)
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                info!("Received interrupt signal (Ctrl+C)");
                shutdown.shutdown(ShutdownReason::UserRequested);
            }
            Err(err) => {
                warn!("Failed to listen for interrupt signal: {}", err);
            }
        }
    });

    // Handle SIGTERM on Unix systems
    #[cfg(unix)]
    {
        let shutdown = global_shutdown();
        tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};

            let mut sigterm = match signal(SignalKind::terminate()) {
                Ok(signal) => signal,
                Err(err) => {
                    warn!("Failed to register SIGTERM handler: {}", err);
                    return;
                }
            };

            sigterm.recv().await;
            info!("Received SIGTERM signal");
            shutdown.shutdown(ShutdownReason::UserRequested);
        });
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::{timeout, Duration};

    use super::*;

    #[tokio::test]
    async fn test_shutdown_coordination() {
        let coordinator = ShutdownCoordinator::new();

        // Test that shutdown is not initially initiated
        assert!(!coordinator.is_shutdown_initiated());

        // Test subscription before shutdown
        let mut receiver = coordinator.subscribe();

        // Test cancellation token
        let token = coordinator.cancellation_token();
        assert!(!token.is_cancelled());

        // Initiate shutdown
        coordinator.shutdown(ShutdownReason::UserRequested);

        // Test that shutdown is now initiated
        assert!(coordinator.is_shutdown_initiated());
        assert!(token.is_cancelled());

        // Test that subscribers are notified
        let event = receiver.recv().await.unwrap();
        match event.reason {
            ShutdownReason::UserRequested => {}
            _ => panic!("Expected UserRequested shutdown reason"),
        }

        // Test that we can wait for shutdown (should complete immediately since shutdown was called)
        let wait_result =
            timeout(Duration::from_millis(100), coordinator.wait_for_shutdown()).await;
        assert!(
            wait_result.is_ok(),
            "Wait for shutdown should complete quickly after shutdown is initiated"
        );
    }

    #[tokio::test]
    async fn test_multiple_shutdown_calls() {
        let coordinator = ShutdownCoordinator::new();

        // Multiple shutdown calls should be idempotent
        coordinator.shutdown(ShutdownReason::UserRequested);
        coordinator.shutdown(ShutdownReason::Error("test".to_string()));

        assert!(coordinator.is_shutdown_initiated());
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let coordinator = ShutdownCoordinator::new();

        // Create multiple subscribers
        let mut receivers = vec![];
        for _ in 0..5 {
            receivers.push(coordinator.subscribe());
        }

        // Initiate shutdown
        coordinator.shutdown(ShutdownReason::Error(String::from("SIGTERM")));

        // All subscribers should receive the shutdown reason
        for mut receiver in receivers {
            let event = timeout(Duration::from_secs(1), receiver.recv())
                .await
                .expect("Should receive within timeout")
                .expect("Should receive value");

            match event.reason {
                ShutdownReason::Error(sig) => assert_eq!(sig, "SIGTERM"),
                _ => panic!("Wrong shutdown reason"),
            }
        }
    }

    #[tokio::test]
    async fn test_cancellation_token_persistence() {
        let token = {
            let coordinator = ShutdownCoordinator::new();
            let token = coordinator.cancellation_token();
            assert!(!token.is_cancelled());
            coordinator.shutdown(ShutdownReason::UserRequested);
            assert!(token.is_cancelled());
            token
        };
        // Token should remain cancelled even after coordinator is dropped
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn test_concurrent_shutdown_calls() {
        let coordinator = ShutdownCoordinator::new();

        // Spawn multiple tasks that try to shutdown concurrently
        let mut handles = vec![];
        for i in 0..10 {
            let coord = coordinator.clone();
            handles.push(tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(i)).await;
                coord.shutdown(ShutdownReason::UserRequested);
            }));
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Should be shutdown exactly once
        assert!(coordinator.is_shutdown_initiated());

        // Cancellation token should be cancelled
        assert!(coordinator.cancellation_token().is_cancelled());
    }

    #[tokio::test]
    async fn test_shutdown_with_error_reason() {
        let coordinator = ShutdownCoordinator::new();
        let mut receiver = coordinator.subscribe();

        let error_msg = "Critical system error";
        coordinator.shutdown(ShutdownReason::Error(error_msg.to_string()));

        let event = receiver.recv().await.unwrap();
        match event.reason {
            ShutdownReason::Error(msg) => assert_eq!(msg, error_msg),
            _ => panic!("Expected Error shutdown reason"),
        }
    }

    #[tokio::test]
    async fn test_wait_for_shutdown_immediate() {
        let coordinator = ShutdownCoordinator::new();

        // Shutdown first
        coordinator.shutdown(ShutdownReason::UserRequested);

        // Wait should complete immediately
        let result = timeout(Duration::from_millis(100), coordinator.wait_for_shutdown()).await;
        assert!(
            result.is_ok(),
            "Should complete immediately when already shutdown"
        );
    }

    #[tokio::test]
    async fn test_wait_for_shutdown_delayed() {
        let coordinator = ShutdownCoordinator::new();
        let coord2 = coordinator.clone();

        // Spawn task that shuts down after delay
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            coord2.shutdown(ShutdownReason::Signal);
        });

        // Wait should complete after shutdown
        let result = timeout(Duration::from_secs(1), coordinator.wait_for_shutdown()).await;
        assert!(
            result.is_ok(),
            "Should complete after shutdown is initiated"
        );
    }

    #[tokio::test]
    async fn test_phased_shutdown() {
        let coordinator = ShutdownCoordinator::new();
        let mut receiver = coordinator.subscribe();

        // Initiate immediate shutdown
        coordinator.initiate_immediate(ShutdownReason::UserRequested);

        // Should receive graceful phase first
        let event = receiver.recv().await.unwrap();
        match event.phase {
            ShutdownPhase::Graceful { deadline } => {
                // Deadline should be about 5 seconds from now
                let remaining = deadline.saturating_duration_since(Instant::now());
                assert!(remaining <= Duration::from_secs(5));
                assert!(remaining >= Duration::from_secs(4));
            }
            _ => panic!("Expected graceful phase first"),
        }

        // Wait for forced phase
        tokio::time::sleep(Duration::from_secs(6)).await;
        let event = receiver.recv().await.unwrap();
        match event.phase {
            ShutdownPhase::Forced => {}
            _ => panic!("Expected forced phase after timeout"),
        }
    }
}

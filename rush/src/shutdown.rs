//! Global shutdown management for graceful termination
//! 
//! This module provides a centralized shutdown system that can be used across
//! the entire application to coordinate graceful shutdown of builds, containers,
//! and other long-running operations.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{broadcast, Notify};
use tokio_util::sync::CancellationToken;
use log::{info, warn, debug};

/// Global shutdown coordinator that manages graceful termination
/// of all application components including builds and container operations.
pub struct ShutdownCoordinator {
    /// Main cancellation token for coordinating shutdown across all components
    cancellation_token: CancellationToken,
    
    /// Broadcast channel for sending shutdown signals to multiple listeners
    shutdown_sender: broadcast::Sender<ShutdownReason>,
    
    /// Flag to indicate if shutdown has been initiated
    shutdown_initiated: AtomicBool,
    
    /// Notification system for waiting on shutdown completion
    shutdown_complete: Arc<Notify>,
}

/// Reason for shutdown to help with logging and cleanup decisions
#[derive(Debug, Clone)]
pub enum ShutdownReason {
    /// User initiated shutdown (Ctrl+C, SIGTERM, etc.)
    UserRequested,
    /// Shutdown due to unrecoverable error
    Error(String),
    /// Graceful shutdown after successful completion
    Completed,
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
            shutdown_initiated: AtomicBool::new(false),
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
    pub fn subscribe(&self) -> broadcast::Receiver<ShutdownReason> {
        self.shutdown_sender.subscribe()
    }
    
    /// Initiate shutdown with the specified reason
    /// This will cancel all operations and notify all listeners
    pub fn shutdown(&self, reason: ShutdownReason) {
        if self.shutdown_initiated.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            info!("Initiating graceful shutdown: {:?}", reason);
            
            // Cancel all operations
            self.cancellation_token.cancel();
            
            // Notify all listeners
            if let Err(_) = self.shutdown_sender.send(reason.clone()) {
                debug!("No shutdown listeners to notify");
            }
            
            // Mark shutdown as complete
            self.shutdown_complete.notify_waiters();
        }
    }
    
    /// Check if shutdown has been initiated
    pub fn is_shutdown_initiated(&self) -> bool {
        self.shutdown_initiated.load(Ordering::SeqCst)
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
static mut SHUTDOWN_COORDINATOR: Option<Arc<ShutdownCoordinator>> = None;
static INIT: std::sync::Once = std::sync::Once::new();

/// Get the global shutdown coordinator instance
pub fn global_shutdown() -> Arc<ShutdownCoordinator> {
    unsafe {
        INIT.call_once(|| {
            SHUTDOWN_COORDINATOR = Some(Arc::new(ShutdownCoordinator::new()));
        });
        SHUTDOWN_COORDINATOR.as_ref().unwrap().clone()
    }
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
    use super::*;
    use tokio::time::{timeout, Duration};
    
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
        let reason = receiver.recv().await.unwrap();
        match reason {
            ShutdownReason::UserRequested => {},
            _ => panic!("Expected UserRequested shutdown reason"),
        }
        
        // Test that we can wait for shutdown (should complete immediately since shutdown was called)
        let wait_result = timeout(Duration::from_millis(100), coordinator.wait_for_shutdown()).await;
        assert!(wait_result.is_ok(), "Wait for shutdown should complete quickly after shutdown is initiated");
    }
    
    #[tokio::test]
    async fn test_multiple_shutdown_calls() {
        let coordinator = ShutdownCoordinator::new();
        
        // Multiple shutdown calls should be idempotent
        coordinator.shutdown(ShutdownReason::UserRequested);
        coordinator.shutdown(ShutdownReason::Error("test".to_string()));
        
        assert!(coordinator.is_shutdown_initiated());
    }
}
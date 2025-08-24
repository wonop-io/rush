//! Error types for the Container Reactor
//!
//! This module defines error types specific to reactor operations,
//! providing detailed error information for debugging and recovery.

use thiserror::Error;

/// Errors that can occur during reactor operations
#[derive(Debug, Error)]
pub enum ReactorError {
    /// Docker operation failed
    #[error("Docker operation failed: {0}")]
    Docker(String),
    
    /// Build operation failed
    #[error("Build failed for component {component}: {error}")]
    Build {
        component: String,
        error: String,
    },
    
    /// Container startup failed
    #[error("Failed to start container {container}: {error}")]
    ContainerStart {
        container: String,
        error: String,
    },
    
    /// Container health check failed
    #[error("Health check failed for {container}: {reason}")]
    HealthCheck {
        container: String,
        reason: String,
    },
    
    /// File watching error
    #[error("File watch error: {0}")]
    FileWatch(String),
    
    /// Network setup failed
    #[error("Network setup failed: {0}")]
    Network(String),
    
    /// Secret injection failed
    #[error("Failed to inject secrets: {0}")]
    SecretInjection(String),
    
    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    /// State transition error
    #[error("Invalid state transition: {0}")]
    StateTransition(String),
    
    /// Timeout occurred
    #[error("Operation timed out: {0}")]
    Timeout(String),
    
    /// Shutdown error
    #[error("Shutdown error: {0}")]
    Shutdown(String),
    
    /// Generic I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    /// Other errors
    #[error("{0}")]
    Other(String),
}

impl ReactorError {
    /// Create a Docker error
    pub fn docker(msg: impl Into<String>) -> Self {
        Self::Docker(msg.into())
    }
    
    /// Create a build error
    pub fn build(component: impl Into<String>, error: impl Into<String>) -> Self {
        Self::Build {
            component: component.into(),
            error: error.into(),
        }
    }
    
    /// Create a container start error
    pub fn container_start(container: impl Into<String>, error: impl Into<String>) -> Self {
        Self::ContainerStart {
            container: container.into(),
            error: error.into(),
        }
    }
    
    /// Create a health check error
    pub fn health_check(container: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::HealthCheck {
            container: container.into(),
            reason: reason.into(),
        }
    }
    
    /// Create a file watch error
    pub fn file_watch(msg: impl Into<String>) -> Self {
        Self::FileWatch(msg.into())
    }
    
    /// Create a network error
    pub fn network(msg: impl Into<String>) -> Self {
        Self::Network(msg.into())
    }
    
    /// Create a secret injection error
    pub fn secret_injection(msg: impl Into<String>) -> Self {
        Self::SecretInjection(msg.into())
    }
    
    /// Create a configuration error
    pub fn configuration(msg: impl Into<String>) -> Self {
        Self::Configuration(msg.into())
    }
    
    /// Create a state transition error
    pub fn state_transition(msg: impl Into<String>) -> Self {
        Self::StateTransition(msg.into())
    }
    
    /// Create a timeout error
    pub fn timeout(msg: impl Into<String>) -> Self {
        Self::Timeout(msg.into())
    }
    
    /// Create a shutdown error
    pub fn shutdown(msg: impl Into<String>) -> Self {
        Self::Shutdown(msg.into())
    }
    
    /// Create an other error
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
    
    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::Docker(_) => false,
            Self::Build { .. } => true,
            Self::ContainerStart { .. } => true,
            Self::HealthCheck { .. } => true,
            Self::FileWatch(_) => true,
            Self::Network(_) => false,
            Self::SecretInjection(_) => false,
            Self::Configuration(_) => false,
            Self::StateTransition(_) => false,
            Self::Timeout(_) => true,
            Self::Shutdown(_) => false,
            Self::Io(_) => false,
            Self::Other(_) => false,
        }
    }
    
    /// Get the component name if this error is component-specific
    pub fn component(&self) -> Option<&str> {
        match self {
            Self::Build { component, .. } => Some(component),
            Self::ContainerStart { container, .. } => Some(container),
            Self::HealthCheck { container, .. } => Some(container),
            _ => None,
        }
    }
}

/// Result type for reactor operations
pub type ReactorResult<T> = Result<T, ReactorError>;

/// Convert from rush_core::error::Error to ReactorError
impl From<rush_core::error::Error> for ReactorError {
    fn from(err: rush_core::error::Error) -> Self {
        match err {
            rush_core::error::Error::Docker(msg) => Self::Docker(msg),
            rush_core::error::Error::Build(msg) => Self::Other(format!("Build error: {}", msg)),
            rush_core::error::Error::Config(msg) => Self::Configuration(msg),
            rush_core::error::Error::Io(e) => Self::Io(e),
            _ => Self::Other(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = ReactorError::docker("Failed to connect");
        assert_eq!(err.to_string(), "Docker operation failed: Failed to connect");
        
        let err = ReactorError::build("frontend", "webpack failed");
        assert_eq!(err.to_string(), "Build failed for component frontend: webpack failed");
        assert_eq!(err.component(), Some("frontend"));
    }

    #[test]
    fn test_error_recoverability() {
        assert!(!ReactorError::docker("test").is_recoverable());
        assert!(ReactorError::build("test", "error").is_recoverable());
        assert!(ReactorError::health_check("test", "unhealthy").is_recoverable());
        assert!(!ReactorError::configuration("bad config").is_recoverable());
        assert!(ReactorError::timeout("took too long").is_recoverable());
    }

    #[test]
    fn test_component_extraction() {
        let err = ReactorError::build("backend", "failed");
        assert_eq!(err.component(), Some("backend"));
        
        let err = ReactorError::container_start("frontend", "port conflict");
        assert_eq!(err.component(), Some("frontend"));
        
        let err = ReactorError::docker("connection lost");
        assert_eq!(err.component(), None);
    }
}
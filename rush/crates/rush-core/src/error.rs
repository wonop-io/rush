use std::io;
use std::path::PathBuf;
use thiserror::Error;

/// Represents all possible errors in the Rush application
#[derive(Debug, Error)]
pub enum Error {
    /// Input/output error
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),
    
    /// Setup error
    #[error("Setup error: {0}")]
    Setup(String),
    
    /// Docker operation error
    #[error("Docker error: {0}")]
    Docker(String),
    
    /// Build error
    #[error("Build error: {0}")]
    Build(String),
    
    /// Deployment error
    #[error("Deployment error: {0}")]
    Deploy(String),
    
    /// Container error
    #[error("Container error: {0}")]
    Container(String),
    
    /// Kubernetes error
    #[error("Kubernetes error: {0}")]
    Kubernetes(String),
    
    /// Vault error
    #[error("Vault error: {0}")]
    Vault(String),
    
    /// File system error with path context
    #[error("File system error at '{path}': {message}")]
    FileSystem { 
        path: PathBuf, 
        message: String 
    },
    
    /// Filesystem error (legacy)
    #[error("Filesystem error: {0}")]
    Filesystem(String),
    
    /// Template error
    #[error("Template error: {0}")]
    Template(String),
    
    /// Terminated error
    #[error("Terminated: {0}")]
    Terminated(String),
    
    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
    
    /// External error (typically tool)
    #[error("External error: {0}")]
    External(String),
    
    /// Launch failed error
    #[error("Launch failed: {0}")]
    LaunchFailed(String),
    
    /// Input validation error
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),
    
    // Service-specific errors (consolidated from rush-local-services)
    
    /// Service not found
    #[error("Service '{0}' not found")]
    ServiceNotFound(String),
    
    /// Service already running
    #[error("Service '{0}' is already running")]
    ServiceAlreadyRunning(String),
    
    /// Service health check failed
    #[error("Service '{0}' failed health check: {1}")]
    HealthCheckFailed(String, String),
    
    /// Service dependency failed
    #[error("Dependency '{0}' failed to start: {1}")]
    DependencyFailed(String, String),
    
    /// Hook execution error
    #[error("Hook error: {0}")]
    Hook(String),
    
    /// Audit logging error
    #[error("Audit error: {0}")]
    Audit(String),
    
    /// Network error
    #[error("Network error: {0}")]
    Network(String),
    
    /// Command execution error
    #[error("Command error: {0}")]
    Command(String),
    
    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    /// Async task error
    #[error("Async error: {0}")]
    Async(String),
    
    /// Generic error for other cases
    #[error("Error: {0}")]
    Other(String),
    
    /// Transparent wrapper for anyhow errors
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

// Maintain backward compatibility with string conversions
impl From<String> for Error {
    fn from(err: String) -> Self {
        Error::Other(err)
    }
}

impl From<&str> for Error {
    fn from(err: &str) -> Self {
        Error::Other(err.to_string())
    }
}

// Manual implementation of PartialEq to handle non-comparable types
impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Error::Config(a), Error::Config(b)) => a == b,
            (Error::Setup(a), Error::Setup(b)) => a == b,
            (Error::Docker(a), Error::Docker(b)) => a == b,
            (Error::Build(a), Error::Build(b)) => a == b,
            (Error::Deploy(a), Error::Deploy(b)) => a == b,
            (Error::Container(a), Error::Container(b)) => a == b,
            (Error::Kubernetes(a), Error::Kubernetes(b)) => a == b,
            (Error::Vault(a), Error::Vault(b)) => a == b,
            (Error::FileSystem { path: p1, message: m1 }, Error::FileSystem { path: p2, message: m2 }) => {
                p1 == p2 && m1 == m2
            }
            (Error::Filesystem(a), Error::Filesystem(b)) => a == b,
            (Error::Template(a), Error::Template(b)) => a == b,
            (Error::Terminated(a), Error::Terminated(b)) => a == b,
            (Error::Internal(a), Error::Internal(b)) => a == b,
            (Error::External(a), Error::External(b)) => a == b,
            (Error::LaunchFailed(a), Error::LaunchFailed(b)) => a == b,
            (Error::InvalidInput(a), Error::InvalidInput(b)) => a == b,
            (Error::Validation(a), Error::Validation(b)) => a == b,
            (Error::ServiceNotFound(a), Error::ServiceNotFound(b)) => a == b,
            (Error::ServiceAlreadyRunning(a), Error::ServiceAlreadyRunning(b)) => a == b,
            (Error::HealthCheckFailed(a1, a2), Error::HealthCheckFailed(b1, b2)) => {
                a1 == b1 && a2 == b2
            }
            (Error::DependencyFailed(a1, a2), Error::DependencyFailed(b1, b2)) => {
                a1 == b1 && a2 == b2
            }
            (Error::Hook(a), Error::Hook(b)) => a == b,
            (Error::Audit(a), Error::Audit(b)) => a == b,
            (Error::Network(a), Error::Network(b)) => a == b,
            (Error::Command(a), Error::Command(b)) => a == b,
            (Error::Serialization(a), Error::Serialization(b)) => a == b,
            (Error::Configuration(a), Error::Configuration(b)) => a == b,
            (Error::Async(a), Error::Async(b)) => a == b,
            (Error::Other(a), Error::Other(b)) => a == b,
            // IO errors and Anyhow errors cannot be reliably compared
            _ => false,
        }
    }
}

/// Result type for Rush operations
pub type Result<T> = std::result::Result<T, Error>;
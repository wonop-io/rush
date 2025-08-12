use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Docker error: {0}")]
    Docker(String),
    
    #[error("Service '{0}' not found")]
    ServiceNotFound(String),
    
    #[error("Service '{0}' is already running")]
    ServiceAlreadyRunning(String),
    
    #[error("Service '{0}' failed health check: {1}")]
    HealthCheckFailed(String, String),
    
    #[error("Dependency '{0}' failed to start: {1}")]
    DependencyFailed(String, String),
    
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

// Implement PartialEq manually to handle the anyhow::Error case
impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Error::Docker(a), Error::Docker(b)) => a == b,
            (Error::ServiceNotFound(a), Error::ServiceNotFound(b)) => a == b,
            (Error::ServiceAlreadyRunning(a), Error::ServiceAlreadyRunning(b)) => a == b,
            (Error::HealthCheckFailed(a1, a2), Error::HealthCheckFailed(b1, b2)) => a1 == b1 && a2 == b2,
            (Error::DependencyFailed(a1, a2), Error::DependencyFailed(b1, b2)) => a1 == b1 && a2 == b2,
            (Error::Configuration(a), Error::Configuration(b)) => a == b,
            // IO errors and Other errors are not easily comparable, so we return false
            _ => false,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
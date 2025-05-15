use std::fmt;
use std::io;
use std::path::PathBuf;

/// Represents all possible errors in the Rush application
#[derive(Debug)]
pub enum Error {
    /// Input/output error
    Io(io::Error),
    /// Configuration error
    Config(String),
    /// Setup error
    Setup(String),
    /// Docker operation error
    Docker(String),
    /// Build error
    Build(String),
    /// Deployment error
    Deploy(String),
    /// Container error
    Container(String),
    /// Kubernetes error
    Kubernetes(String),
    /// Vault error
    Vault(String),
    /// File system error
    FileSystem { path: PathBuf, message: String },
    /// Template error
    Template(String),
    /// Terminated error
    Terminated(String),
    /// Internal error
    Internal(String),
    /// Launch failed error
    LaunchFailed(String),
    /// Unknown error
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(err) => write!(f, "I/O error: {}", err),
            Error::Config(msg) => write!(f, "Configuration error: {}", msg),
            Error::Setup(msg) => write!(f, "Setup error: {}", msg),
            Error::Docker(msg) => write!(f, "Docker error: {}", msg),
            Error::Build(msg) => write!(f, "Build error: {}", msg),
            Error::Deploy(msg) => write!(f, "Deployment error: {}", msg),
            Error::Container(msg) => write!(f, "Container error: {}", msg),
            Error::Kubernetes(msg) => write!(f, "Kubernetes error: {}", msg),
            Error::Vault(msg) => write!(f, "Vault error: {}", msg),
            Error::FileSystem { path, message } => {
                write!(f, "File system error at '{}': {}", path.display(), message)
            }
            Error::Template(msg) => write!(f, "Template error: {}", msg),
            Error::Terminated(msg) => write!(f, "Terminated: {}", msg),
            Error::Internal(msg) => write!(f, "Internal error: {}", msg),
            Error::LaunchFailed(msg) => write!(f, "Launch failed: {}", msg),
            Error::Other(msg) => write!(f, "Error: {}", msg),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

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

/// Result type for Rush operations
pub type Result<T> = std::result::Result<T, Error>;

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::error::Result;
use super::{OutputDirector, OutputSource, OutputStream};

/// A thread-safe wrapper for OutputDirector that can be shared across tasks
pub struct SharedOutputDirector {
    inner: Arc<Mutex<Box<dyn OutputDirector>>>,
}

impl SharedOutputDirector {
    /// Create a new shared output director
    pub fn new(director: Box<dyn OutputDirector>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(director)),
        }
    }

    /// Clone the shared reference
    pub fn clone_ref(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }

    /// Write output through the shared director
    pub async fn write_output(&self, source: &OutputSource, stream: &OutputStream) -> Result<()> {
        eprintln!("DEBUG: SharedOutputDirector writing output for source: {}", source.name);
        let mut director = self.inner.lock().await;
        director.write_output(source, stream).await
    }

    /// Flush the shared director
    pub async fn flush(&self) -> Result<()> {
        let mut director = self.inner.lock().await;
        director.flush().await
    }
}

impl Clone for SharedOutputDirector {
    fn clone(&self) -> Self {
        self.clone_ref()
    }
}
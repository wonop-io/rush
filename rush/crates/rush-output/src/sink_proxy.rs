//! Sink proxy for bridging between different sink ownership models
//!
//! This module provides a proxy that allows using an Arc<Mutex<Box<dyn Sink>>>
//! where a Box<dyn Sink> is expected.

use std::sync::Arc;

use async_trait::async_trait;
use rush_core::error::Result;
use tokio::sync::Mutex;

use crate::simple::{LogEntry, Sink};

/// A proxy sink that forwards to an Arc<Mutex<Box<dyn Sink>>>
pub struct SinkProxy {
    inner: Arc<Mutex<Box<dyn Sink>>>,
}

impl SinkProxy {
    /// Create a new sink proxy
    pub fn new(inner: Arc<Mutex<Box<dyn Sink>>>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl Sink for SinkProxy {
    async fn write(&mut self, entry: LogEntry) -> Result<()> {
        let mut sink_guard = self.inner.lock().await;
        sink_guard.write(entry).await
    }

    async fn flush(&mut self) -> Result<()> {
        let mut sink_guard = self.inner.lock().await;
        sink_guard.flush().await
    }

    async fn close(&mut self) -> Result<()> {
        let mut sink_guard = self.inner.lock().await;
        sink_guard.close().await
    }
}

//! Output sink integration for local services
//!
//! This module provides helpers for integrating OutputSink with local services.

use std::sync::Arc;

use rush_output::simple::{LogEntry, Sink};
use tokio::sync::Mutex;

/// Helper struct for managing output sink in services
#[derive(Clone)]
pub struct ServiceOutput {
    /// Service name for log entries
    service_name: String,

    /// Optional output sink
    sink: Option<Arc<Mutex<Box<dyn Sink>>>>,
}

impl ServiceOutput {
    /// Create a new ServiceOutput
    pub fn new(service_name: String) -> Self {
        Self {
            service_name,
            sink: None,
        }
    }

    /// Set the output sink
    pub fn set_sink(&mut self, sink: Arc<Mutex<Box<dyn Sink>>>) {
        self.sink = Some(sink);
    }

    /// Check if a sink is set
    pub fn has_sink(&self) -> bool {
        self.sink.is_some()
    }

    /// Log an info message
    pub async fn info(&self, message: impl Into<String>) {
        self.log(message.into(), false).await;
    }

    /// Log an error message
    pub async fn error(&self, message: impl Into<String>) {
        self.log(message.into(), true).await;
    }

    /// Log a message through the output sink or fallback to log crate
    async fn log(&self, message: String, is_error: bool) {
        if let Some(ref sink) = self.sink {
            // Use the output sink
            let entry = if is_error {
                LogEntry::docker(&self.service_name, &message).as_error()
            } else {
                LogEntry::docker(&self.service_name, &message)
            };

            let mut sink = sink.lock().await;
            let _ = sink.write(entry).await;
        } else {
            // Fallback to log crate
            if is_error {
                log::error!("[{}] {}", self.service_name, message);
            } else {
                log::info!("[{}] {}", self.service_name, message);
            }
        }
    }

    /// Create a line handler for processing output lines
    pub fn line_handler(
        &self,
    ) -> impl Fn(String) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Clone
    {
        let output = self.clone();
        move |line: String| {
            let output = output.clone();
            Box::pin(async move {
                output.info(line).await;
            })
        }
    }
}

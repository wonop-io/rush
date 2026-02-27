//! Bridge between the `log` crate and our output sink system
//!
//! This module provides a custom logger implementation that forwards
//! all log messages to our output sink.

use std::cell::RefCell;
use std::sync::Arc;

use log::{Level, Log, Metadata, Record};
use tokio::runtime::Handle;
use tokio::sync::Mutex;

use crate::simple::{LogEntry, Sink};

thread_local! {
    /// Thread-local flag to prevent recursive logging
    static IN_LOG: RefCell<bool> = const { RefCell::new(false) };
}

/// A logger that forwards all log messages to a sink
pub struct SinkLogger {
    sink: Arc<Mutex<Box<dyn Sink>>>,
    max_level: Level,
}

impl SinkLogger {
    /// Create a new sink logger
    pub fn new(sink: Arc<Mutex<Box<dyn Sink>>>) -> Self {
        Self {
            sink,
            max_level: Level::Trace,
        }
    }

    /// Create a new sink logger with a specific max level
    pub fn with_level(sink: Arc<Mutex<Box<dyn Sink>>>, max_level: Level) -> Self {
        Self { sink, max_level }
    }

    /// Initialize this logger as the global logger
    pub fn init(self) -> Result<(), log::SetLoggerError> {
        let max_level = self.max_level;
        log::set_boxed_logger(Box::new(self))?;
        log::set_max_level(max_level.to_level_filter());
        Ok(())
    }
}

impl Log for SinkLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.max_level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        // Prevent recursive logging
        IN_LOG.with(|in_log| {
            if *in_log.borrow() {
                // Already logging, skip to prevent recursion
                return;
            }

            // Set flag to indicate we're logging
            *in_log.borrow_mut() = true;

            // Ensure we reset the flag when done
            let _guard = scopeguard::guard((), |_| {
                IN_LOG.with(|in_log| *in_log.borrow_mut() = false);
            });

            // Format the log message
            let content = if let Some(module) = record.module_path() {
                format!("[{}] {}", module, record.args())
            } else {
                format!("{}", record.args())
            };

            // Determine component from target or module
            let component = record
                .target()
                .split("::")
                .next()
                .unwrap_or("system")
                .to_string();

            // Create log entry based on level
            let mut entry = LogEntry::system(content);
            entry.component = component;

            // Mark errors and warnings
            entry.is_error = matches!(record.level(), Level::Error | Level::Warn);

            // Clone sink for async operation
            let sink = self.sink.clone();

            // Try to use current tokio runtime, or spawn blocking if not in async context
            if let Ok(handle) = Handle::try_current() {
                // We're in an async context
                handle.spawn(async move {
                    let mut sink_guard = sink.lock().await;
                    // Ignore write errors in logger to prevent panic
                    let _ = sink_guard.write(entry).await;
                });
            } else {
                // We're not in an async context, try to enter runtime if available
                // For now, we'll just drop the message if we can't send it
                // In production, you might want to queue these messages
                std::thread::spawn(move || {
                    // Try to create a small runtime just for this operation
                    if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        rt.block_on(async {
                            let mut sink_guard = sink.lock().await;
                            let _ = sink_guard.write(entry).await;
                        });
                    }
                });
            }
        });
    }

    fn flush(&self) {
        // Clone sink for async operation
        let sink = self.sink.clone();

        // Try to flush
        if let Ok(handle) = Handle::try_current() {
            handle.spawn(async move {
                let mut sink_guard = sink.lock().await;
                let _ = sink_guard.flush().await;
            });
        }
    }
}

/// Initialize the global logger with a sink
pub fn init_with_sink(
    sink: Arc<Mutex<Box<dyn Sink>>>,
    level: Level,
) -> Result<(), log::SetLoggerError> {
    SinkLogger::with_level(sink, level).init()
}

/// Initialize the global logger with a sink at debug level
pub fn init_with_sink_debug(sink: Arc<Mutex<Box<dyn Sink>>>) -> Result<(), log::SetLoggerError> {
    init_with_sink(sink, Level::Debug)
}

/// Initialize the global logger with a sink at info level
pub fn init_with_sink_info(sink: Arc<Mutex<Box<dyn Sink>>>) -> Result<(), log::SetLoggerError> {
    init_with_sink(sink, Level::Info)
}

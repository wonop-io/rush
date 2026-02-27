//! File watching for containers
//!
//! This module provides functionality to monitor file changes in container contexts,
//! allowing the system to detect when files have been modified and trigger rebuilds.

pub mod coordinator;
pub mod handler;
mod processor;
mod setup;
pub mod smart_watcher;

pub use coordinator::{CoordinatorBuilder, CoordinatorConfig, WatchResult, WatcherCoordinator};
pub use handler::{ChangeBatch, FileChangeHandler, HandlerConfig};
pub use processor::ChangeProcessor;
pub use setup::{create_component_matcher, setup_file_watcher, WatcherConfig};
pub use smart_watcher::{ComponentWatcher, FileChangeEvent, SmartWatcher, SmartWatcherConfig};

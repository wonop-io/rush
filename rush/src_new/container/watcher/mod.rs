//! File watching for containers
//!
//! This module provides functionality to monitor file changes in container contexts,
//! allowing the system to detect when files have been modified and trigger rebuilds.

mod processor;
mod setup;

pub use processor::ChangeProcessor;
pub use setup::{create_component_matcher, setup_file_watcher, WatcherConfig};

//! Setup for file watchers in containers
//!
//! This module provides functionality to set up file watchers that monitor
//! for changes in the container context, triggering rebuilds when necessary.

use std::path::{Path, PathBuf};

use log::{debug, error, info, trace};
use notify::{
    Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use rush_core::error::Result;
use rush_utils::PathMatcher;

use crate::watcher::processor::ChangeProcessor;

/// Configuration for file watching
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    /// Base directory to watch
    pub root_dir: PathBuf,
    /// Specific paths to watch if any (empty means watch everything)
    pub watch_paths: Vec<PathBuf>,
    /// Duration to debounce file changes
    pub debounce_ms: u64,
    /// Whether to use gitignore patterns
    pub use_gitignore: bool,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            root_dir: PathBuf::from("."),
            watch_paths: Vec::new(),
            debounce_ms: 300,
            use_gitignore: true,
        }
    }
}

/// Sets up a file watcher for the specified directory
///
/// Returns a change processor that can be used to check for and handle file changes
///
/// # Arguments
///
/// * `config` - Configuration for the watcher
pub fn setup_file_watcher(config: WatcherConfig) -> Result<(RecommendedWatcher, ChangeProcessor)> {
    debug!("Setting up file watcher for {:?}", config.root_dir);

    // Create change processor
    let processor = ChangeProcessor::new(&config.root_dir, config.debounce_ms);

    // Create a channel to receive file system events
    let (tx, rx) = std::sync::mpsc::channel();

    // Create watcher with default configuration
    let mut watcher = RecommendedWatcher::new(tx, NotifyConfig::default()).map_err(|e| {
        rush_core::error::Error::Setup(format!("Failed to create file watcher: {e}"))
    })?;

    // Watch paths
    if config.watch_paths.is_empty() {
        // Watch the entire root directory recursively
        watcher
            .watch(&config.root_dir, RecursiveMode::Recursive)
            .map_err(|e| {
                rush_core::error::Error::Setup(format!(
                    "Failed to watch directory {}: {}",
                    config.root_dir.display(),
                    e
                ))
            })?;

        info!("Watching root directory: {}", config.root_dir.display());
    } else {
        // Watch specific paths
        for path in &config.watch_paths {
            let full_path = if path.is_absolute() {
                path.clone()
            } else {
                config.root_dir.join(path)
            };

            watcher
                .watch(&full_path, RecursiveMode::Recursive)
                .map_err(|e| {
                    rush_core::error::Error::Setup(format!(
                        "Failed to watch path {}: {}",
                        full_path.display(),
                        e
                    ))
                })?;

            debug!("Watching path: {}", full_path.display());
        }
    }

    // Spawn a thread to process file events
    let processor_clone = processor.clone();
    std::thread::spawn(move || {
        debug!("File watcher thread started");
        while let Ok(event) = rx.recv() {
            match event {
                Ok(event) => {
                    trace!("File event: {:?}", event);
                    process_file_event(event, &processor_clone);
                }
                Err(e) => {
                    error!("File watcher error: {}", e);
                }
            }
        }
        debug!("File watcher thread exited");
    });

    info!("File watcher setup complete");
    Ok((watcher, processor))
}

/// Process a file system event and update the change processor
///
/// # Arguments
///
/// * `event` - The file system event
/// * `processor` - The change processor to update
fn process_file_event(event: Event, processor: &ChangeProcessor) {
    // Only process file modifications, creations, and removals
    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
            for path in event.paths {
                debug!(
                    "File system event detected: {} (kind: {:?})",
                    path.display(),
                    event.kind
                );
                processor.add_change(path);
            }
        }
        _ => {
            // Ignore other event types
            trace!("Ignoring event type: {:?}", event.kind);
        }
    }
}

/// Creates a path matcher for a component's context
///
/// # Arguments
///
/// * `component_name` - Name of the component
/// * `component_root` - Root directory of the component
/// * `patterns` - Glob patterns to match
pub fn create_component_matcher(
    component_name: &str,
    component_root: &Path,
    patterns: Vec<String>,
) -> PathMatcher {
    debug!(
        "Creating path matcher for component {} with {} patterns",
        component_name,
        patterns.len()
    );

    PathMatcher::new(component_root, patterns)
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_file_watcher_setup() {
        let temp_dir = TempDir::new().unwrap();
        let config = WatcherConfig {
            root_dir: temp_dir.path().to_path_buf(),
            watch_paths: Vec::new(),
            debounce_ms: 100,
            use_gitignore: true,
        };

        let result = setup_file_watcher(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_component_matcher() {
        let temp_dir = TempDir::new().unwrap();
        let matcher = create_component_matcher(
            "test-component",
            temp_dir.path(),
            vec!["*.rs".to_string(), "*.toml".to_string()],
        );

        let rs_file = temp_dir.path().join("test.rs");
        let js_file = temp_dir.path().join("test.js");
        let toml_file = temp_dir.path().join("Cargo.toml");

        // Create test files
        File::create(&rs_file).unwrap();
        File::create(&js_file).unwrap();
        File::create(&toml_file).unwrap();

        assert!(matcher.matches(&rs_file));
        assert!(matcher.matches(&toml_file));
        assert!(!matcher.matches(&js_file));
    }
}

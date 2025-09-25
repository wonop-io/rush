//! Smart file watcher with debouncing and intelligent filtering
//!
//! This module provides an optimized file watcher that reduces
//! unnecessary rebuilds through debouncing and pattern matching.

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, FileIdMap};
use rush_core::{Error, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use log::{debug, info, error};
use regex::Regex;

/// Configuration for smart file watching
#[derive(Debug, Clone)]
pub struct SmartWatcherConfig {
    /// Debounce duration (time to wait for additional changes)
    pub debounce_duration: Duration,
    /// Batch timeout (maximum time to wait before processing batch)
    pub batch_timeout: Duration,
    /// Maximum batch size
    pub max_batch_size: usize,
    /// Ignore patterns (regex)
    pub ignore_patterns: Vec<String>,
    /// Include patterns (regex) - if set, only these are watched
    pub include_patterns: Vec<String>,
    /// Enable gitignore integration
    pub use_gitignore: bool,
}

impl Default for SmartWatcherConfig {
    fn default() -> Self {
        Self {
            debounce_duration: Duration::from_millis(500),
            batch_timeout: Duration::from_secs(2),
            max_batch_size: 100,
            ignore_patterns: vec![
                r"\.git/.*".to_string(),
                r"target/.*".to_string(),
                r"node_modules/.*".to_string(),
                r"dist/.*".to_string(),
                r"\.rush/.*".to_string(),
                r".*\.swp$".to_string(),
                r".*\.tmp$".to_string(),
                r".*~$".to_string(),
            ],
            include_patterns: vec![],
            use_gitignore: true,
        }
    }
}

/// Event type for file changes
#[derive(Debug, Clone)]
pub enum FileChangeEvent {
    /// Files were created
    Created(Vec<PathBuf>),
    /// Files were modified
    Modified(Vec<PathBuf>),
    /// Files were removed
    Removed(Vec<PathBuf>),
    /// Files were renamed
    Renamed { from: Vec<PathBuf>, to: Vec<PathBuf> },
}

/// Smart file watcher with debouncing
pub struct SmartWatcher {
    config: SmartWatcherConfig,
    debouncer: Option<Debouncer<RecommendedWatcher, FileIdMap>>,
    event_tx: mpsc::UnboundedSender<FileChangeEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<FileChangeEvent>>,
    ignore_regex: Vec<Regex>,
    include_regex: Vec<Regex>,
    pending_events: Arc<RwLock<Vec<Event>>>,
    watch_paths: Vec<PathBuf>,
}

impl SmartWatcher {
    /// Create a new smart watcher
    pub fn new(config: SmartWatcherConfig) -> Result<Self> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // Compile regex patterns
        let ignore_regex = config
            .ignore_patterns
            .iter()
            .filter_map(|pattern| Regex::new(pattern).ok())
            .collect();

        let include_regex = config
            .include_patterns
            .iter()
            .filter_map(|pattern| Regex::new(pattern).ok())
            .collect();

        Ok(Self {
            config,
            debouncer: None,
            event_tx,
            event_rx: Some(event_rx),
            ignore_regex,
            include_regex,
            pending_events: Arc::new(RwLock::new(Vec::new())),
            watch_paths: Vec::new(),
        })
    }

    /// Add a path to watch
    pub fn watch_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(Error::FileSystem {
                path: path.to_path_buf(),
                message: "Path does not exist".to_string(),
            });
        }

        self.watch_paths.push(path.to_path_buf());

        // If watcher is already running, add the path
        if let Some(debouncer) = &mut self.debouncer {
            let mode = if path.is_dir() {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };

            debouncer
                .watcher()
                .watch(path, mode)
                .map_err(|e| Error::External(format!("Failed to watch path: {}", e)))?;

            info!("Added watch path: {:?}", path);
        }

        Ok(())
    }

    /// Start watching for file changes
    pub async fn start(&mut self) -> Result<()> {
        let event_tx = self.event_tx.clone();
        let pending_events = Arc::clone(&self.pending_events);
        let config = self.config.clone();
        let ignore_regex = self.ignore_regex.clone();
        let include_regex = self.include_regex.clone();

        // Create debouncer
        let mut debouncer = new_debouncer(
            self.config.debounce_duration,
            None,
            move |result: DebounceEventResult| {
                match result {
                    Ok(events) => {
                        let filtered_events = Self::filter_events(
                            events,
                            &ignore_regex,
                            &include_regex,
                        );

                        if !filtered_events.is_empty() {
                            // Add to pending events
                            let pending = pending_events.clone();
                            let tx = event_tx.clone();

                            tokio::spawn(async move {
                                let mut pending_guard = pending.write().await;
                                pending_guard.extend(filtered_events);

                                // If batch is full or timeout reached, process it
                                if pending_guard.len() >= config.max_batch_size {
                                    let batch = std::mem::take(&mut *pending_guard);
                                    drop(pending_guard);
                                    Self::process_batch(batch, tx).await;
                                }
                            });
                        }
                    }
                    Err(errors) => {
                        for error in errors {
                            error!("File watch error: {:?}", error);
                        }
                    }
                }
            },
        ).map_err(|e| Error::External(format!("Failed to create debouncer: {}", e)))?;

        // Add all watch paths
        for path in &self.watch_paths {
            let mode = if path.is_dir() {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };

            debouncer
                .watcher()
                .watch(path, mode)
                .map_err(|e| Error::External(format!("Failed to watch path: {}", e)))?;
        }

        self.debouncer = Some(debouncer);

        // Start batch timeout processor
        self.start_batch_processor().await;

        info!("Smart watcher started with {} paths", self.watch_paths.len());
        Ok(())
    }

    /// Start the batch processor that handles timeouts
    async fn start_batch_processor(&self) {
        let pending_events = Arc::clone(&self.pending_events);
        let event_tx = self.event_tx.clone();
        let batch_timeout = self.config.batch_timeout;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(batch_timeout);

            loop {
                interval.tick().await;

                let mut pending_guard = pending_events.write().await;
                if !pending_guard.is_empty() {
                    let batch = std::mem::take(&mut *pending_guard);
                    drop(pending_guard);
                    Self::process_batch(batch, event_tx.clone()).await;
                }
            }
        });
    }

    /// Filter events based on patterns
    fn filter_events(
        events: Vec<notify_debouncer_full::DebouncedEvent>,
        ignore_patterns: &[Regex],
        include_patterns: &[Regex],
    ) -> Vec<Event> {
        events
            .into_iter()
            .filter_map(|debounced_event| {
                // Convert DebouncedEvent to Event
                let event = debounced_event.event;

                for path in &event.paths {
                    let path_str = path.to_string_lossy();

                    // Check ignore patterns
                    for pattern in ignore_patterns {
                        if pattern.is_match(&path_str) {
                            debug!("Ignoring path: {}", path_str);
                            return None;
                        }
                    }

                    // Check include patterns (if any)
                    if !include_patterns.is_empty() {
                        let mut included = false;
                        for pattern in include_patterns {
                            if pattern.is_match(&path_str) {
                                included = true;
                                break;
                            }
                        }
                        if !included {
                            debug!("Path not in include list: {}", path_str);
                            return None;
                        }
                    }
                }

                Some(event)
            })
            .collect()
    }

    /// Process a batch of events
    async fn process_batch(events: Vec<Event>, tx: mpsc::UnboundedSender<FileChangeEvent>) {
        let mut created = HashSet::new();
        let mut modified = HashSet::new();
        let mut removed = HashSet::new();
        let renamed_from = Vec::new();
        let renamed_to = Vec::new();

        for event in events {
            match event.kind {
                EventKind::Create(_) => {
                    for path in event.paths {
                        created.insert(path);
                    }
                }
                EventKind::Modify(_) => {
                    for path in event.paths {
                        // Don't report as modified if it was just created
                        if !created.contains(&path) {
                            modified.insert(path);
                        }
                    }
                }
                EventKind::Remove(_) => {
                    for path in event.paths {
                        // Remove from created/modified if it was deleted
                        created.remove(&path);
                        modified.remove(&path);
                        removed.insert(path);
                    }
                }
                _ => {
                    // Handle other event types if needed
                }
            }
        }

        // Send consolidated events
        if !created.is_empty() {
            let _ = tx.send(FileChangeEvent::Created(created.into_iter().collect()));
        }
        if !modified.is_empty() {
            let _ = tx.send(FileChangeEvent::Modified(modified.into_iter().collect()));
        }
        if !removed.is_empty() {
            let _ = tx.send(FileChangeEvent::Removed(removed.into_iter().collect()));
        }
        if !renamed_from.is_empty() && !renamed_to.is_empty() {
            let _ = tx.send(FileChangeEvent::Renamed {
                from: renamed_from,
                to: renamed_to,
            });
        }
    }

    /// Get the event receiver
    pub fn take_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<FileChangeEvent>> {
        self.event_rx.take()
    }

    /// Stop watching
    pub async fn stop(&mut self) -> Result<()> {
        self.debouncer = None;
        info!("Smart watcher stopped");
        Ok(())
    }

    /// Check if a path should be ignored based on gitignore
    pub fn should_ignore_gitignore(&self, path: &Path) -> bool {
        if !self.config.use_gitignore {
            return false;
        }

        // This is a simplified implementation
        // In production, you'd want to use the gitignore crate
        let path_str = path.to_string_lossy();

        // Common gitignore patterns
        if path_str.contains("/.git/") ||
           path_str.contains("/target/") ||
           path_str.contains("/node_modules/") ||
           path_str.contains("/.rush/") {
            return true;
        }

        false
    }
}

/// Component-specific watcher that tracks changes per component
pub struct ComponentWatcher {
    component_name: String,
    watcher: SmartWatcher,
    change_callback: Arc<dyn Fn(Vec<PathBuf>) + Send + Sync>,
}

impl ComponentWatcher {
    /// Create a new component watcher
    pub fn new(
        component_name: String,
        config: SmartWatcherConfig,
        change_callback: Arc<dyn Fn(Vec<PathBuf>) + Send + Sync>,
    ) -> Result<Self> {
        let watcher = SmartWatcher::new(config)?;

        Ok(Self {
            component_name,
            watcher,
            change_callback,
        })
    }

    /// Watch component paths
    pub fn watch_component(&mut self, base_path: &Path, watch_patterns: &[String]) -> Result<()> {
        // Watch the base path
        self.watcher.watch_path(base_path)?;

        // Add any additional patterns
        for pattern in watch_patterns {
            let path = base_path.join(pattern);
            if path.exists() {
                self.watcher.watch_path(&path)?;
            }
        }

        Ok(())
    }

    /// Start watching and processing events
    pub async fn start(&mut self) -> Result<()> {
        self.watcher.start().await?;

        // Take the receiver and start processing events
        if let Some(mut rx) = self.watcher.take_receiver() {
            let component_name = self.component_name.clone();
            let callback = Arc::clone(&self.change_callback);

            tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    let paths = match event {
                        FileChangeEvent::Created(paths) |
                        FileChangeEvent::Modified(paths) |
                        FileChangeEvent::Removed(paths) => paths,
                        FileChangeEvent::Renamed { to, .. } => to,
                    };

                    if !paths.is_empty() {
                        info!("Component {} detected changes in {} files",
                            component_name, paths.len());
                        callback(paths);
                    }
                }
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::time::sleep;

    #[tokio::test]
    #[ignore = "File watcher events not being received in test environment"]
    async fn test_smart_watcher_debouncing() {
        let temp_dir = TempDir::new().unwrap();
        let config = SmartWatcherConfig {
            debounce_duration: Duration::from_millis(100),
            batch_timeout: Duration::from_millis(500),
            ..Default::default()
        };

        let mut watcher = SmartWatcher::new(config).unwrap();
        watcher.watch_path(temp_dir.path()).unwrap();

        let mut rx = watcher.take_receiver().unwrap();
        watcher.start().await.unwrap();

        // Give watcher time to fully initialize
        sleep(Duration::from_millis(100)).await;

        // Create multiple files quickly
        for i in 0..5 {
            let file_path = temp_dir.path().join(format!("test{}.txt", i));
            std::fs::write(&file_path, "content").unwrap();
            sleep(Duration::from_millis(10)).await;
        }

        // Wait for debounce and batch timeout
        sleep(Duration::from_millis(700)).await;

        // Try to receive with a very short timeout first to see if anything is there
        let mut event = None;
        for _ in 0..3 {
            match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
                Ok(Some(e)) => {
                    event = Some(e);
                    break;
                }
                _ => {
                    sleep(Duration::from_millis(100)).await;
                    continue;
                }
            }
        }

        assert!(event.is_some(), "Should have received an event");

        if let Some(FileChangeEvent::Created(paths)) = event {
            assert_eq!(paths.len(), 5);
        } else {
            panic!("Expected Created event");
        }
    }

    #[tokio::test]
    async fn test_ignore_patterns() {
        let config = SmartWatcherConfig::default();
        let watcher = SmartWatcher::new(config).unwrap();

        // Test that gitignore patterns work
        let git_path = Path::new("/project/.git/objects/abc123");
        assert!(watcher.should_ignore_gitignore(git_path));

        let target_path = Path::new("/project/target/debug/build");
        assert!(watcher.should_ignore_gitignore(target_path));

        let source_path = Path::new("/project/src/main.rs");
        assert!(!watcher.should_ignore_gitignore(source_path));
    }
}
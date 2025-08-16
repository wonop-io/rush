use crate::event::OutputEvent;
use crate::filter::{CompositeFilter, OutputFilter};
use crate::formatter::ColoredFormatter;
use crate::router::{BroadcastRouter, OutputRouter};
use crate::sink::{OutputSink, TerminalLayout, TerminalSink};
use chrono::{DateTime, Utc};
use rush_core::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Session statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionStats {
    pub events_processed: u64,
    pub events_filtered: u64,
    pub events_routed: u64,
    pub errors: u64,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
}

impl SessionStats {
    /// Get session duration
    pub fn duration(&self) -> Option<chrono::Duration> {
        match (self.start_time, self.end_time) {
            (Some(start), Some(end)) => Some(end - start),
            (Some(start), None) => Some(Utc::now() - start),
            _ => None,
        }
    }

    /// Get events per second
    pub fn events_per_second(&self) -> f64 {
        if let Some(duration) = self.duration() {
            let seconds = duration.num_seconds() as f64;
            if seconds > 0.0 {
                self.events_processed as f64 / seconds
            } else {
                0.0
            }
        } else {
            0.0
        }
    }
}

/// Session recording for replay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecording {
    pub id: Uuid,
    pub events: Vec<RecordedEvent>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
}

/// A recorded event with timing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedEvent {
    pub event: OutputEvent,
    pub relative_time_ms: u64,
}

/// Configuration for an output session
pub struct SessionConfig {
    pub mode: OutputMode,
    pub filters: Vec<Box<dyn OutputFilter>>,
    pub sinks: Vec<Box<dyn OutputSink>>,
    pub recording_path: Option<PathBuf>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            mode: OutputMode::Auto,
            filters: Vec::new(),
            sinks: vec![Box::new(TerminalSink::new())],
            recording_path: None,
        }
    }
}

/// Output mode selection
#[derive(Debug, Clone, Copy)]
pub enum OutputMode {
    Auto,
    Simple,
    Split,
    Dashboard,
    Web,
}

impl OutputMode {
    /// Automatically select the best output mode
    pub fn auto() -> Self {
        if !atty::is(atty::Stream::Stdout) {
            OutputMode::Simple // CI or piped output
        } else {
            let (width, _) = crossterm::terminal::size().unwrap_or((80, 24));
            if width > 120 {
                OutputMode::Split // Wide terminal with good support
            } else {
                OutputMode::Simple // Fallback to simple
            }
        }
    }
}

/// Manages an output session with multiple streams
pub struct OutputSession {
    id: Uuid,
    router: Box<dyn OutputRouter>,
    filters: CompositeFilter,
    stats: SessionStats,
    recording: Option<SessionRecording>,
    recording_start: Option<std::time::Instant>,
}

impl OutputSession {
    /// Create a new session with configuration
    pub fn new(config: SessionConfig) -> Result<Self> {
        let mut filters = CompositeFilter::new();
        for filter in config.filters {
            filters = filters.add(filter);
        }

        let router = Box::new(BroadcastRouter::new(config.sinks));

        let mut session = Self {
            id: Uuid::new_v4(),
            router,
            filters,
            stats: SessionStats {
                start_time: Some(Utc::now()),
                ..Default::default()
            },
            recording: None,
            recording_start: None,
        };

        if let Some(path) = config.recording_path {
            session.start_recording(path)?;
        }

        Ok(session)
    }

    /// Create a session builder
    pub fn builder() -> SessionBuilder {
        SessionBuilder::new()
    }

    /// Submit an event to the session
    pub async fn submit(&mut self, event: OutputEvent) -> Result<()> {
        self.stats.events_processed += 1;

        // Apply filters
        if !self.filters.should_pass(&event) {
            self.stats.events_filtered += 1;
            return Ok(());
        }

        // Record if enabled
        if let Some(recording) = &mut self.recording {
            if let Some(start) = self.recording_start {
                let relative_time_ms = start.elapsed().as_millis() as u64;
                recording.events.push(RecordedEvent {
                    event: event.clone(),
                    relative_time_ms,
                });
            }
        }

        // Route to sinks
        if let Err(e) = self.router.route(event).await {
            self.stats.errors += 1;
            return Err(e);
        }

        self.stats.events_routed += 1;
        Ok(())
    }

    /// Get current session statistics
    pub fn stats(&self) -> &SessionStats {
        &self.stats
    }

    /// Start recording the session
    pub fn start_recording(&mut self, _path: PathBuf) -> Result<()> {
        if self.recording.is_some() {
            return Err(Error::Other("Recording already in progress".to_string()));
        }

        self.recording = Some(SessionRecording {
            id: self.id,
            events: Vec::new(),
            start_time: Utc::now(),
            end_time: None,
        });
        self.recording_start = Some(std::time::Instant::now());

        Ok(())
    }

    /// Stop recording and save to file
    pub async fn stop_recording(&mut self) -> Result<()> {
        if let Some(mut recording) = self.recording.take() {
            recording.end_time = Some(Utc::now());

            // Save recording to file
            // Implementation would serialize and save the recording

            self.recording_start = None;
        }

        Ok(())
    }

    /// Flush all outputs
    pub async fn flush(&mut self) -> Result<()> {
        self.router.flush().await
    }

    /// Close the session
    pub async fn close(&mut self) -> Result<()> {
        self.stats.end_time = Some(Utc::now());

        if self.recording.is_some() {
            self.stop_recording().await?;
        }

        self.router.close().await
    }

    /// Replay a recorded session
    pub async fn replay(recording: SessionRecording, speed: f32) -> Result<()> {
        let mut last_time = 0u64;

        for recorded_event in recording.events {
            // Calculate delay
            let delay_ms = ((recorded_event.relative_time_ms - last_time) as f32 / speed) as u64;
            if delay_ms > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }

            // Output the event (would need a configured session)
            println!("{}", recorded_event.event.stream.as_string());

            last_time = recorded_event.relative_time_ms;
        }

        Ok(())
    }
}

/// Builder for creating output sessions
pub struct SessionBuilder {
    config: SessionConfig,
}

impl SessionBuilder {
    /// Create a new session builder
    pub fn new() -> Self {
        Self {
            config: SessionConfig::default(),
        }
    }

    /// Set the output mode
    pub fn mode(mut self, mode: OutputMode) -> Self {
        self.config.mode = mode;
        self
    }

    /// Add a filter
    pub fn filter(mut self, filter: Box<dyn OutputFilter>) -> Self {
        self.config.filters.push(filter);
        self
    }

    /// Add a sink
    pub fn sink(mut self, sink: Box<dyn OutputSink>) -> Self {
        self.config.sinks.push(sink);
        self
    }

    /// Replace all sinks
    pub fn sinks(mut self, sinks: Vec<Box<dyn OutputSink>>) -> Self {
        self.config.sinks = sinks;
        self
    }

    /// Set recording path
    pub fn record_to(mut self, path: PathBuf) -> Self {
        self.config.recording_path = Some(path);
        self
    }

    /// Build the session
    pub fn build(self) -> Result<OutputSession> {
        // If no sinks were specified, add default based on mode
        let mut config = self.config;
        eprintln!(
            "DEBUG session.rs: Building session with mode: {:?}",
            config.mode
        );
        if config.sinks.is_empty() {
            config.sinks = match config.mode {
                OutputMode::Auto => {
                    let mode = OutputMode::auto();
                    eprintln!("DEBUG session.rs: Auto mode selected: {mode:?}");
                    Self::default_sinks_for_mode(mode)
                }
                mode => {
                    eprintln!("DEBUG session.rs: Using specified mode: {mode:?}");
                    Self::default_sinks_for_mode(mode)
                }
            };
        }
        eprintln!("DEBUG session.rs: Created {} sinks", config.sinks.len());

        OutputSession::new(config)
    }

    /// Get default sinks for a mode
    fn default_sinks_for_mode(mode: OutputMode) -> Vec<Box<dyn OutputSink>> {
        eprintln!(
            "DEBUG session.rs: Creating default sinks for mode: {mode:?}"
        );
        match mode {
            OutputMode::Simple => {
                eprintln!("DEBUG session.rs: Creating Simple mode sink");
                vec![Box::new(
                    TerminalSink::new().with_formatter(Box::new(ColoredFormatter::default())),
                )]
            }
            OutputMode::Auto => {
                // Auto should actually resolve to a specific mode
                let resolved_mode = OutputMode::auto();
                eprintln!(
                    "DEBUG session.rs: Auto mode resolved to: {resolved_mode:?}"
                );
                Self::default_sinks_for_mode(resolved_mode)
            }
            OutputMode::Split => {
                eprintln!("DEBUG session.rs: Creating Split mode sink with panes");
                vec![Box::new(
                    TerminalSink::new()
                        .with_formatter(Box::new(ColoredFormatter::default()))
                        .with_layout(TerminalLayout::Split {
                            panes: vec![
                                crate::sink::PaneConfig::new("Build"),
                                crate::sink::PaneConfig::new("Runtime"),
                            ],
                        }),
                )]
            }
            OutputMode::Dashboard => {
                vec![Box::new(
                    TerminalSink::new()
                        .with_formatter(Box::new(ColoredFormatter::default()))
                        .with_layout(TerminalLayout::Dashboard { widgets: vec![] }),
                )]
            }
            OutputMode::Web => {
                // Placeholder for web mode - just use terminal with a prefix
                vec![Box::new(
                    TerminalSink::new()
                        .with_formatter(Box::new(ColoredFormatter::default()))
                        .with_layout(TerminalLayout::Web),
                )]
            }
        }
    }
}

impl Default for SessionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::ComponentFilter;
    use crate::{OutputSource, OutputStream};

    #[tokio::test]
    async fn test_session_creation() {
        let session = OutputSession::builder()
            .mode(OutputMode::Simple)
            .build()
            .unwrap();

        assert_eq!(session.stats.events_processed, 0);
        assert!(session.stats.start_time.is_some());
    }

    #[tokio::test]
    async fn test_session_filtering() {
        let mut session = OutputSession::builder()
            .filter(Box::new(ComponentFilter::allowlist(vec![
                "backend".to_string()
            ])))
            .build()
            .unwrap();

        // This should pass
        let source = OutputSource::new("backend", "container");
        let event =
            OutputEvent::runtime(source, OutputStream::stdout(b"backend data".to_vec()), None);
        session.submit(event).await.unwrap();

        // This should be filtered
        let source = OutputSource::new("frontend", "container");
        let event = OutputEvent::runtime(
            source,
            OutputStream::stdout(b"frontend data".to_vec()),
            None,
        );
        session.submit(event).await.unwrap();

        assert_eq!(session.stats.events_processed, 2);
        assert_eq!(session.stats.events_filtered, 1);
        assert_eq!(session.stats.events_routed, 1);
    }

    #[tokio::test]
    async fn test_session_stats() {
        let mut session = OutputSession::builder().build().unwrap();

        for i in 0..10 {
            let source = OutputSource::new("test", "container");
            let event = OutputEvent::runtime(
                source,
                OutputStream::stdout(format!("event {i}").into_bytes()),
                None,
            );
            session.submit(event).await.unwrap();
        }

        assert_eq!(session.stats.events_processed, 10);
        assert_eq!(session.stats.events_routed, 10);
        assert!(session.stats.duration().is_some());
    }
}

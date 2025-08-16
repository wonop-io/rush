use crate::event::{ExecutionPhase, LogLevel, OutputEvent};
use chrono::{DateTime, Utc};
use regex::Regex;
use std::collections::HashSet;

/// Trait for filtering output events
pub trait OutputFilter: Send + Sync {
    /// Check if an event should pass through the filter
    fn should_pass(&self, event: &OutputEvent) -> bool;

    /// Get a description of this filter
    fn description(&self) -> String;
}

/// Combines two filters with AND logic
pub struct AndFilter<A, B> {
    left: A,
    right: B,
}

impl<A, B> AndFilter<A, B> {
    pub fn new(left: A, right: B) -> Self {
        Self { left, right }
    }
}

impl<A, B> OutputFilter for AndFilter<A, B>
where
    A: OutputFilter,
    B: OutputFilter,
{
    fn should_pass(&self, event: &OutputEvent) -> bool {
        self.left.should_pass(event) && self.right.should_pass(event)
    }

    fn description(&self) -> String {
        format!(
            "{} AND {}",
            self.left.description(),
            self.right.description()
        )
    }
}

/// Combines two filters with OR logic
pub struct OrFilter<A, B> {
    left: A,
    right: B,
}

impl<A, B> OrFilter<A, B> {
    pub fn new(left: A, right: B) -> Self {
        Self { left, right }
    }
}

impl<A, B> OutputFilter for OrFilter<A, B>
where
    A: OutputFilter,
    B: OutputFilter,
{
    fn should_pass(&self, event: &OutputEvent) -> bool {
        self.left.should_pass(event) || self.right.should_pass(event)
    }

    fn description(&self) -> String {
        format!(
            "{} OR {}",
            self.left.description(),
            self.right.description()
        )
    }
}

/// Filter by component name
#[derive(Clone)]
pub struct ComponentFilter {
    components: HashSet<String>,
    include: bool, // true for allowlist, false for denylist
}

impl ComponentFilter {
    /// Create an allowlist filter (only these components pass)
    pub fn allowlist(components: Vec<String>) -> Self {
        Self {
            components: components.into_iter().collect(),
            include: true,
        }
    }

    /// Create a denylist filter (all except these components pass)
    pub fn denylist(components: Vec<String>) -> Self {
        Self {
            components: components.into_iter().collect(),
            include: false,
        }
    }

    /// Create a filter that allows all components
    pub fn all() -> Self {
        Self {
            components: HashSet::new(),
            include: false, // Empty denylist = allow all
        }
    }
}

impl OutputFilter for ComponentFilter {
    fn should_pass(&self, event: &OutputEvent) -> bool {
        let contains = self.components.contains(&event.source.name);
        if self.include {
            contains // Allowlist: pass if in list
        } else {
            !contains // Denylist: pass if not in list
        }
    }

    fn description(&self) -> String {
        if self.components.is_empty() {
            "All components".to_string()
        } else {
            let list_type = if self.include { "Include" } else { "Exclude" };
            format!("{}: {:?}", list_type, self.components)
        }
    }
}

/// Filter by execution phase
#[derive(Clone)]
pub struct PhaseFilter {
    compile_time: bool,
    runtime: bool,
    system: bool,
}

impl PhaseFilter {
    /// Create a filter that allows all phases
    pub fn all() -> Self {
        Self {
            compile_time: true,
            runtime: true,
            system: true,
        }
    }

    /// Create a filter for compile-time only
    pub fn compile_time() -> Self {
        Self {
            compile_time: true,
            runtime: false,
            system: false,
        }
    }

    /// Create a filter for runtime only
    pub fn runtime() -> Self {
        Self {
            compile_time: false,
            runtime: true,
            system: false,
        }
    }

    /// Create a custom phase filter
    pub fn new(compile_time: bool, runtime: bool, system: bool) -> Self {
        Self {
            compile_time,
            runtime,
            system,
        }
    }
}

impl OutputFilter for PhaseFilter {
    fn should_pass(&self, event: &OutputEvent) -> bool {
        match &event.phase {
            ExecutionPhase::CompileTime { .. } => self.compile_time,
            ExecutionPhase::Runtime { .. } => self.runtime,
            ExecutionPhase::System { .. } => self.system,
        }
    }

    fn description(&self) -> String {
        let mut phases = Vec::new();
        if self.compile_time {
            phases.push("compile");
        }
        if self.runtime {
            phases.push("runtime");
        }
        if self.system {
            phases.push("system");
        }
        format!("Phases: {}", phases.join(", "))
    }
}

/// Filter by log level
#[derive(Clone)]
pub struct LevelFilter {
    min_level: LogLevel,
}

impl LevelFilter {
    /// Create a new level filter
    pub fn new(min_level: LogLevel) -> Self {
        Self { min_level }
    }

    /// Create a filter that allows all levels
    pub fn all() -> Self {
        Self {
            min_level: LogLevel::Trace,
        }
    }
}

impl OutputFilter for LevelFilter {
    fn should_pass(&self, event: &OutputEvent) -> bool {
        if let Some(level) = event.metadata.level {
            level >= self.min_level
        } else {
            true // Pass events without log levels
        }
    }

    fn description(&self) -> String {
        format!("Min level: {:?}", self.min_level)
    }
}

/// Mode for pattern matching
#[derive(Clone, Debug)]
pub enum PatternMode {
    /// All patterns must match
    All,
    /// At least one pattern must match
    Any,
    /// No patterns must match
    None,
}

/// Pattern-based filter for content
#[derive(Clone)]
pub struct PatternFilter {
    patterns: Vec<Regex>,
    mode: PatternMode,
}

impl PatternFilter {
    /// Create a new pattern filter
    pub fn new(patterns: Vec<String>, mode: PatternMode) -> Result<Self, regex::Error> {
        let compiled: Result<Vec<_>, _> = patterns.iter().map(|p| Regex::new(p)).collect();
        Ok(Self {
            patterns: compiled?,
            mode,
        })
    }

    /// Create a filter that matches any of the patterns
    pub fn any(patterns: Vec<String>) -> Result<Self, regex::Error> {
        Self::new(patterns, PatternMode::Any)
    }

    /// Create a filter that matches all patterns
    pub fn all(patterns: Vec<String>) -> Result<Self, regex::Error> {
        Self::new(patterns, PatternMode::All)
    }

    /// Create a filter that excludes all patterns
    pub fn none(patterns: Vec<String>) -> Result<Self, regex::Error> {
        Self::new(patterns, PatternMode::None)
    }
}

impl OutputFilter for PatternFilter {
    fn should_pass(&self, event: &OutputEvent) -> bool {
        let text = event.stream.as_string();
        let matches: Vec<bool> = self.patterns.iter().map(|p| p.is_match(&text)).collect();

        match self.mode {
            PatternMode::All => matches.iter().all(|&m| m),
            PatternMode::Any => matches.iter().any(|&m| m),
            PatternMode::None => !matches.iter().any(|&m| m),
        }
    }

    fn description(&self) -> String {
        format!(
            "Pattern filter ({:?}): {} patterns",
            self.mode,
            self.patterns.len()
        )
    }
}

/// Time-based filter
#[derive(Clone)]
pub struct TimeFilter {
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
}

impl TimeFilter {
    /// Create a filter for events after a specific time
    pub fn after(start: DateTime<Utc>) -> Self {
        Self {
            start: Some(start),
            end: None,
        }
    }

    /// Create a filter for events before a specific time
    pub fn before(end: DateTime<Utc>) -> Self {
        Self {
            start: None,
            end: Some(end),
        }
    }

    /// Create a filter for events within a time range
    pub fn between(start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        Self {
            start: Some(start),
            end: Some(end),
        }
    }
}

impl OutputFilter for TimeFilter {
    fn should_pass(&self, event: &OutputEvent) -> bool {
        let passes_start = self.start.is_none_or(|s| event.timestamp >= s);
        let passes_end = self.end.is_none_or(|e| event.timestamp <= e);
        passes_start && passes_end
    }

    fn description(&self) -> String {
        match (self.start, self.end) {
            (Some(s), Some(e)) => format!("Between {s} and {e}"),
            (Some(s), None) => format!("After {s}"),
            (None, Some(e)) => format!("Before {e}"),
            (None, None) => "All times".to_string(),
        }
    }
}

/// Composite filter that combines multiple filters
pub struct CompositeFilter {
    filters: Vec<Box<dyn OutputFilter>>,
}

impl CompositeFilter {
    /// Create a new composite filter
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
        }
    }

    /// Add a filter to the composite
    pub fn add(mut self, filter: Box<dyn OutputFilter>) -> Self {
        self.filters.push(filter);
        self
    }

    /// Check if the composite has any filters
    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }
}

impl Default for CompositeFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFilter for CompositeFilter {
    fn should_pass(&self, event: &OutputEvent) -> bool {
        // All filters must pass (AND logic)
        for filter in &self.filters {
            if !filter.should_pass(event) {
                return false;
            }
        }
        true
    }

    fn description(&self) -> String {
        if self.filters.is_empty() {
            "No filters".to_string()
        } else {
            format!("{} filters combined", self.filters.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OutputSource, OutputStream};

    #[test]
    fn test_component_filter_allowlist() {
        let filter =
            ComponentFilter::allowlist(vec!["backend".to_string(), "frontend".to_string()]);

        let source = OutputSource::new("backend", "container");
        let event = OutputEvent::runtime(source, OutputStream::stdout(b"test".to_vec()), None);

        assert!(filter.should_pass(&event));

        let source = OutputSource::new("database", "container");
        let event = OutputEvent::runtime(source, OutputStream::stdout(b"test".to_vec()), None);

        assert!(!filter.should_pass(&event));
    }

    #[test]
    fn test_phase_filter() {
        let filter = PhaseFilter::compile_time();

        let source = OutputSource::new("test", "container");
        let event = OutputEvent::compile_time(
            source.clone(),
            crate::event::CompileStage::Compilation,
            "test".to_string(),
            OutputStream::stdout(b"compiling".to_vec()),
        );

        assert!(filter.should_pass(&event));

        let event = OutputEvent::runtime(source, OutputStream::stdout(b"running".to_vec()), None);

        assert!(!filter.should_pass(&event));
    }

    #[test]
    fn test_pattern_filter() {
        let filter = PatternFilter::any(vec!["error".to_string(), "warning".to_string()]).unwrap();

        let source = OutputSource::new("test", "container");
        let event = OutputEvent::runtime(
            source.clone(),
            OutputStream::stdout(b"An error occurred".to_vec()),
            None,
        );

        assert!(filter.should_pass(&event));

        let event = OutputEvent::runtime(
            source,
            OutputStream::stdout(b"Everything is fine".to_vec()),
            None,
        );

        assert!(!filter.should_pass(&event));
    }
}

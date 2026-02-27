use rush_config::loader::DevOutputConfig;
use rush_core::error::Result;

use crate::event::LogLevel;
use crate::filter::{ComponentFilter, LevelFilter, OutputFilter, PhaseFilter};
use crate::formatter::{ColorTheme, ColoredFormatter, OutputFormatter};
use crate::session::{OutputMode, OutputSession, SessionBuilder};
use crate::sink::{FileSink, OutputSink, TerminalSink};

/// Create an output session from configuration
pub fn create_session_from_config(config: &DevOutputConfig) -> Result<OutputSession> {
    let mut builder = SessionBuilder::new();

    // Set output mode
    let mode = parse_output_mode(&config.mode);
    builder = builder.mode(mode);

    // Add component filters
    if let Some(filters) = create_component_filter(&config.components) {
        builder = builder.filter(filters);
    }

    // Add phase filter
    let phase_filter = create_phase_filter(&config.phases);
    builder = builder.filter(Box::new(phase_filter));

    // Add level filter
    let level_filter = create_level_filter(&config.log_level);
    builder = builder.filter(Box::new(level_filter));

    // Create sinks based on mode and configuration
    let mut sinks: Vec<Box<dyn OutputSink>> = Vec::new();

    // Terminal sink
    let terminal_sink = create_terminal_sink(mode, &config.colors);
    sinks.push(Box::new(terminal_sink));

    // File sink if configured
    if let Some(file_config) = &config.file_log {
        if file_config.enabled {
            let file_sink = FileSink::new(&file_config.path)?;
            sinks.push(Box::new(file_sink));
        }
    }

    builder = builder.sinks(sinks);

    builder.build()
}

/// Parse output mode from string
fn parse_output_mode(mode: &str) -> OutputMode {
    match mode.to_lowercase().as_str() {
        "auto" => OutputMode::Auto,
        "simple" => OutputMode::Simple,
        "split" => OutputMode::Split,
        "dashboard" => OutputMode::Dashboard,
        "web" => OutputMode::Web,
        _ => OutputMode::Auto,
    }
}

/// Create component filter from configuration
fn create_component_filter(
    config: &rush_config::loader::ComponentFilterConfig,
) -> Option<Box<dyn OutputFilter>> {
    if let Some(include) = &config.include {
        Some(Box::new(ComponentFilter::allowlist(include.clone())))
    } else if let Some(exclude) = &config.exclude {
        Some(Box::new(ComponentFilter::denylist(exclude.clone())))
    } else {
        None
    }
}

/// Create phase filter from configuration
fn create_phase_filter(config: &rush_config::loader::PhaseFilterConfig) -> PhaseFilter {
    PhaseFilter::new(config.show_build, config.show_runtime, config.show_system)
}

/// Create level filter from configuration
fn create_level_filter(level_str: &str) -> LevelFilter {
    let level = match level_str.to_lowercase().as_str() {
        "trace" => LogLevel::Trace,
        "debug" => LogLevel::Debug,
        "info" => LogLevel::Info,
        "warn" | "warning" => LogLevel::Warn,
        "error" => LogLevel::Error,
        _ => LogLevel::Info,
    };
    LevelFilter::new(level)
}

/// Create terminal sink based on mode and color configuration
fn create_terminal_sink(
    mode: OutputMode,
    color_config: &rush_config::loader::ColorConfig,
) -> TerminalSink {
    let formatter = create_formatter(color_config);

    let mut sink = TerminalSink::new().with_formatter(formatter);

    // Set layout based on mode
    match mode {
        OutputMode::Split => {
            sink = sink.with_layout(crate::sink::TerminalLayout::Split {
                panes: vec![
                    crate::sink::PaneConfig::new("Build"),
                    crate::sink::PaneConfig::new("Runtime"),
                ],
            });
        }
        OutputMode::Dashboard => {
            sink = sink.with_layout(crate::sink::TerminalLayout::Dashboard { widgets: vec![] });
        }
        _ => {}
    }

    sink
}

/// Create formatter based on color configuration
fn create_formatter(color_config: &rush_config::loader::ColorConfig) -> Box<dyn OutputFormatter> {
    let color_enabled = match color_config.enabled.as_str() {
        "auto" => atty::is(atty::Stream::Stdout),
        "true" | "yes" | "1" => true,
        _ => false,
    };

    if color_enabled {
        let theme = match color_config.theme.as_str() {
            "monokai" => ColorTheme::monokai(),
            "dracula" => ColorTheme::dracula(),
            _ => ColorTheme::default(),
        };
        Box::new(ColoredFormatter::new().with_theme(theme))
    } else {
        Box::new(crate::formatter::PlainFormatter::new())
    }
}

/// Apply development output configuration from rushd.yaml
pub async fn apply_dev_output_config(config: &DevOutputConfig) -> Result<OutputSession> {
    create_session_from_config(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_output_mode() {
        assert!(matches!(parse_output_mode("auto"), OutputMode::Auto));
        assert!(matches!(parse_output_mode("simple"), OutputMode::Simple));
        assert!(matches!(parse_output_mode("split"), OutputMode::Split));
        assert!(matches!(
            parse_output_mode("dashboard"),
            OutputMode::Dashboard
        ));
        assert!(matches!(parse_output_mode("web"), OutputMode::Web));
        assert!(matches!(parse_output_mode("invalid"), OutputMode::Auto));
    }

    #[test]
    fn test_create_level_filter() {
        let _filter = create_level_filter("debug");
        // The filter is created successfully - test passes if no panic
    }

    #[tokio::test]
    async fn test_create_session_from_default_config() {
        let config = DevOutputConfig::default();
        let session = create_session_from_config(&config);
        assert!(session.is_ok());
    }
}

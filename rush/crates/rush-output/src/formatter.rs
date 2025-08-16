use crate::event::{ExecutionPhase, LogLevel, OutputEvent};
use chrono::Local;
use colored::*;
use serde_json;
use std::collections::HashMap;

/// Trait for formatting output events
pub trait OutputFormatter: Send + Sync {
    /// Format an event for display
    fn format(&self, event: &OutputEvent) -> String;

    /// Format with specific width constraints
    fn format_width(&self, event: &OutputEvent, width: usize) -> String {
        let full = self.format(event);
        if full.len() <= width {
            full
        } else {
            format!("{}...", &full[..width.saturating_sub(3)])
        }
    }
}

/// Plain text formatter
#[derive(Clone)]
pub struct PlainFormatter {
    template: String,
    timestamp_format: String,
    show_phase: bool,
}

impl Default for PlainFormatter {
    fn default() -> Self {
        Self {
            template: "{timestamp} {source} | {content}".to_string(),
            timestamp_format: "%H:%M:%S%.3f".to_string(),
            show_phase: false,
        }
    }
}

impl PlainFormatter {
    /// Create a new plain formatter
    pub fn new() -> Self {
        Self::default()
    }

    /// Set whether to show execution phase
    pub fn with_phase(mut self, show: bool) -> Self {
        self.show_phase = show;
        self
    }

    /// Set the timestamp format
    pub fn with_timestamp_format(mut self, format: impl Into<String>) -> Self {
        self.timestamp_format = format.into();
        self
    }
}

impl OutputFormatter for PlainFormatter {
    fn format(&self, event: &OutputEvent) -> String {
        let timestamp = event
            .timestamp
            .with_timezone(&Local)
            .format(&self.timestamp_format);

        let source = &event.source.name;
        let content_str = event.stream.as_str();
        let content = content_str.trim_end();

        if self.show_phase {
            let phase = match &event.phase {
                ExecutionPhase::CompileTime { stage, .. } => format!("[{}]", stage.as_str()),
                ExecutionPhase::Runtime { .. } => "[Runtime]".to_string(),
                ExecutionPhase::System { subsystem } => format!("[System:{}]", subsystem),
            };

            format!("{} {} {} | {}", timestamp, phase, source, content)
        } else {
            format!("{} {} | {}", timestamp, source, content)
        }
    }
}

/// JSON formatter
#[derive(Clone)]
pub struct JsonFormatter {
    pretty: bool,
    include_metadata: bool,
}

impl JsonFormatter {
    /// Create a new JSON formatter
    pub fn new(pretty: bool) -> Self {
        Self {
            pretty,
            include_metadata: true,
        }
    }

    /// Create a pretty JSON formatter
    pub fn pretty() -> Self {
        Self::new(true)
    }

    /// Create a compact JSON formatter
    pub fn compact() -> Self {
        Self::new(false)
    }

    /// Set whether to include metadata
    pub fn with_metadata(mut self, include: bool) -> Self {
        self.include_metadata = include;
        self
    }
}

impl OutputFormatter for JsonFormatter {
    fn format(&self, event: &OutputEvent) -> String {
        if self.include_metadata {
            if self.pretty {
                serde_json::to_string_pretty(event)
                    .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize: {}\"}}", e))
            } else {
                serde_json::to_string(event)
                    .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize: {}\"}}", e))
            }
        } else {
            // Simplified output without full metadata
            let output = serde_json::json!({
                "timestamp": event.timestamp.to_rfc3339(),
                "source": event.source.name,
                "content": event.stream.as_string(),
            });

            if self.pretty {
                serde_json::to_string_pretty(&output).unwrap()
            } else {
                serde_json::to_string(&output).unwrap()
            }
        }
    }
}

/// Color theme for terminal output
#[derive(Clone)]
pub struct ColorTheme {
    pub timestamp: Color,
    pub source: Color,
    pub compile_time: Color,
    pub runtime: Color,
    pub system: Color,
    pub error: Color,
    pub warning: Color,
    pub info: Color,
    pub debug: Color,
}

impl Default for ColorTheme {
    fn default() -> Self {
        Self {
            timestamp: Color::BrightBlack,
            source: Color::Cyan,
            compile_time: Color::Yellow,
            runtime: Color::Green,
            system: Color::Magenta,
            error: Color::Red,
            warning: Color::Yellow,
            info: Color::Blue,
            debug: Color::BrightBlack,
        }
    }
}

impl ColorTheme {
    /// Get the monokai theme
    pub fn monokai() -> Self {
        Self {
            timestamp: Color::BrightBlack,
            source: Color::Magenta,
            compile_time: Color::Yellow,
            runtime: Color::Green,
            system: Color::Cyan,
            error: Color::Red,
            warning: Color::TrueColor {
                r: 255,
                g: 165,
                b: 0,
            },
            info: Color::Blue,
            debug: Color::BrightBlack,
        }
    }

    /// Get the dracula theme
    pub fn dracula() -> Self {
        Self {
            timestamp: Color::TrueColor {
                r: 98,
                g: 114,
                b: 164,
            },
            source: Color::TrueColor {
                r: 139,
                g: 233,
                b: 253,
            },
            compile_time: Color::TrueColor {
                r: 241,
                g: 250,
                b: 140,
            },
            runtime: Color::TrueColor {
                r: 80,
                g: 250,
                b: 123,
            },
            system: Color::TrueColor {
                r: 255,
                g: 121,
                b: 198,
            },
            error: Color::TrueColor {
                r: 255,
                g: 85,
                b: 85,
            },
            warning: Color::TrueColor {
                r: 255,
                g: 184,
                b: 108,
            },
            info: Color::TrueColor {
                r: 189,
                g: 147,
                b: 249,
            },
            debug: Color::TrueColor {
                r: 98,
                g: 114,
                b: 164,
            },
        }
    }
}

/// Colored terminal formatter
pub struct ColoredFormatter {
    theme: ColorTheme,
    component_colors: HashMap<String, Color>,
    timestamp_format: String,
    show_phase: bool,
}

impl Default for ColoredFormatter {
    fn default() -> Self {
        Self {
            theme: ColorTheme::default(),
            component_colors: HashMap::new(),
            timestamp_format: "%H:%M:%S%.3f".to_string(),
            show_phase: true,
        }
    }
}

impl ColoredFormatter {
    /// Create a new colored formatter
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the color theme
    pub fn with_theme(mut self, theme: ColorTheme) -> Self {
        self.theme = theme;
        self
    }

    /// Set a specific color for a component
    pub fn with_component_color(mut self, component: impl Into<String>, color: Color) -> Self {
        self.component_colors.insert(component.into(), color);
        self
    }

    /// Get the color for a log level
    fn level_color(&self, level: LogLevel) -> Color {
        match level {
            LogLevel::Error => self.theme.error,
            LogLevel::Warn => self.theme.warning,
            LogLevel::Info => self.theme.info,
            LogLevel::Debug => self.theme.debug,
            LogLevel::Trace => self.theme.debug,
        }
    }

    /// Get the color for a phase
    fn phase_color(&self, phase: &ExecutionPhase) -> Color {
        match phase {
            ExecutionPhase::CompileTime { .. } => self.theme.compile_time,
            ExecutionPhase::Runtime { .. } => self.theme.runtime,
            ExecutionPhase::System { .. } => self.theme.system,
        }
    }
}

impl OutputFormatter for ColoredFormatter {
    fn format(&self, event: &OutputEvent) -> String {
        let timestamp = event
            .timestamp
            .with_timezone(&Local)
            .format(&self.timestamp_format)
            .to_string()
            .color(self.theme.timestamp);

        let source_color = self
            .component_colors
            .get(&event.source.name)
            .copied()
            .unwrap_or(self.theme.source);
        let source = event.source.name.color(source_color);

        let content_str = event.stream.as_str();
        let content = content_str.trim_end();

        // Don't re-color content if it already contains ANSI codes
        // This preserves colors from Docker containers
        let has_ansi_codes = content.contains("\x1b[");

        // Color content based on log level if present and no existing ANSI codes
        let content = if !has_ansi_codes && event.metadata.level.is_some() {
            content
                .to_string()
                .color(self.level_color(event.metadata.level.unwrap()))
        } else {
            content.to_string().normal()
        };

        if self.show_phase {
            let phase_str = match &event.phase {
                ExecutionPhase::CompileTime { stage, .. } => stage.as_str(),
                ExecutionPhase::Runtime { .. } => "Runtime",
                ExecutionPhase::System { subsystem } => subsystem.as_str(),
            };
            let phase = phase_str.color(self.phase_color(&event.phase));

            format!("{} {} {} | {}", timestamp, phase, source, content)
        } else {
            format!("{} {} | {}", timestamp, source, content)
        }
    }
}

/// Structured log formats
#[derive(Clone, Copy)]
pub enum StructuredFormat {
    Logfmt,
    Json,
    Csv,
}

/// Structured log formatter
pub struct StructuredFormatter {
    format: StructuredFormat,
}

impl StructuredFormatter {
    /// Create a new structured formatter
    pub fn new(format: StructuredFormat) -> Self {
        Self { format }
    }
}

impl OutputFormatter for StructuredFormatter {
    fn format(&self, event: &OutputEvent) -> String {
        match self.format {
            StructuredFormat::Logfmt => {
                let mut parts = vec![
                    format!("ts={}", event.timestamp.to_rfc3339()),
                    format!("source={}", event.source.name),
                    format!(
                        "msg=\"{}\"",
                        event.stream.as_str().trim_end().replace('"', "\\\"")
                    ),
                ];

                if let Some(level) = event.metadata.level {
                    parts.push(format!("level={:?}", level).to_lowercase());
                }

                for (key, value) in &event.metadata.tags {
                    parts.push(format!("{}={}", key, value));
                }

                parts.join(" ")
            }
            StructuredFormat::Json => {
                let output = serde_json::json!({
                    "timestamp": event.timestamp.to_rfc3339(),
                    "source": event.source.name,
                    "message": event.stream.as_string().trim_end(),
                    "level": event.metadata.level.map(|l| format!("{:?}", l).to_lowercase()),
                    "tags": event.metadata.tags,
                });
                serde_json::to_string(&output).unwrap()
            }
            StructuredFormat::Csv => {
                // Simple CSV format
                format!(
                    "{},{},{},\"{}\",{:?}",
                    event.timestamp.to_rfc3339(),
                    event.source.name,
                    event.source.source_type,
                    event.stream.as_str().trim_end().replace('"', "\"\""),
                    event.metadata.level
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OutputSource, OutputStream};

    #[test]
    fn test_plain_formatter() {
        let formatter = PlainFormatter::new();
        let source = OutputSource::new("test", "container");
        let event = OutputEvent::runtime(
            source,
            OutputStream::stdout(b"Hello, World!\n".to_vec()),
            None,
        );

        let formatted = formatter.format(&event);
        assert!(formatted.contains("test"));
        assert!(formatted.contains("Hello, World!"));
    }

    #[test]
    fn test_json_formatter() {
        let formatter = JsonFormatter::compact();
        let source = OutputSource::new("test", "container");
        let event =
            OutputEvent::runtime(source, OutputStream::stdout(b"test message".to_vec()), None);

        let formatted = formatter.format(&event);
        assert!(formatted.contains("\"source\":"));
        assert!(formatted.contains("\"test\""));
    }

    #[test]
    fn test_colored_formatter() {
        let formatter = ColoredFormatter::new();
        let source = OutputSource::new("backend", "container");
        let event = OutputEvent::compile_time(
            source,
            crate::event::CompileStage::Compilation,
            "backend".to_string(),
            OutputStream::stdout(b"Compiling backend...\n".to_vec()),
        );

        let formatted = formatter.format(&event);
        assert!(formatted.contains("backend"));
        assert!(formatted.contains("Compiling backend"));
    }

    #[test]
    fn test_structured_formatter_logfmt() {
        let formatter = StructuredFormatter::new(StructuredFormat::Logfmt);
        let source = OutputSource::new("test", "container");
        let mut event =
            OutputEvent::runtime(source, OutputStream::stdout(b"test message".to_vec()), None);
        event.metadata.level = Some(LogLevel::Info);

        let formatted = formatter.format(&event);
        assert!(formatted.contains("source=test"));
        assert!(formatted.contains("level=info"));
        assert!(formatted.contains("msg=\"test message\""));
    }
}

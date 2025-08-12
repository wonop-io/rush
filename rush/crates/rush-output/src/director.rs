use super::{OutputSource, OutputStream};
use rush_core::error::Result;
use async_trait::async_trait;

/// Trait for directing output streams to different destinations
#[async_trait]
pub trait OutputDirector: Send + Sync {
    /// Write output data to the director's destination
    ///
    /// # Arguments
    /// * `source` - Information about where the output is coming from
    /// * `stream` - The output data and metadata
    async fn write_output(&mut self, source: &OutputSource, stream: &OutputStream) -> Result<()>;

    /// Flush any buffered output
    async fn flush(&mut self) -> Result<()>;

    /// Check if the director supports colored output
    fn supports_color(&self) -> bool {
        false
    }

    /// Set whether colored output should be used
    fn set_color_enabled(&mut self, _enabled: bool) {}
}

/// Standard output/error director that writes to stdout/stderr
pub struct StdOutputDirector {
    /// Whether colored output is enabled
    color_enabled: bool,
}

impl StdOutputDirector {
    /// Create a new standard output director
    pub fn new() -> Self {
        Self {
            color_enabled: true,
        }
    }

    /// Create a new director with color disabled
    pub fn new_no_color() -> Self {
        Self {
            color_enabled: false,
        }
    }

    /// Format the source label with optional color
    fn format_source_label(&self, source: &OutputSource) -> String {
        let label = source.display_name();

        if self.color_enabled {
            if let Some(color) = &source.color {
                use colored::Colorize;
                match color.as_str() {
                    "red" => label.red().bold().to_string(),
                    "green" => label.green().bold().to_string(),
                    "blue" => label.blue().bold().to_string(),
                    "yellow" => label.yellow().bold().to_string(),
                    "purple" => label.purple().bold().to_string(),
                    "cyan" => label.cyan().bold().to_string(),
                    "white" => label.white().bold().to_string(),
                    _ => label.bold().to_string(),
                }
            } else {
                use colored::Colorize;
                label.bold().to_string()
            }
        } else {
            label
        }
    }
}

impl Default for StdOutputDirector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OutputDirector for StdOutputDirector {
    async fn write_output(&mut self, source: &OutputSource, stream: &OutputStream) -> Result<()> {
        if stream.is_empty() {
            return Ok(());
        }

        let formatted_label = self.format_source_label(source);
        let content = stream.as_str();

        // Handle line-by-line output to ensure proper formatting
        for line in content.lines() {
            match stream.stream_type {
                super::OutputStreamType::Stdout | super::OutputStreamType::Combined => {
                    println!("{formatted_label} | {line}");
                }
                super::OutputStreamType::Stderr => {
                    eprintln!("{formatted_label} | {line}");
                }
            }
        }

        // Handle the case where the content doesn't end with a newline
        if !content.ends_with('\n') && !content.ends_with("\r\n") && !content.is_empty() {
            match stream.stream_type {
                super::OutputStreamType::Stdout | super::OutputStreamType::Combined => {
                    print!("{formatted_label} | {content}");
                }
                super::OutputStreamType::Stderr => {
                    eprint!("{formatted_label} | {content}");
                }
            }
        }

        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        use std::io::{self, Write};
        io::stdout()
            .flush()
            .map_err(rush_core::Error::Io)?;
        io::stderr()
            .flush()
            .map_err(rush_core::Error::Io)?;
        Ok(())
    }

    fn supports_color(&self) -> bool {
        true
    }

    fn set_color_enabled(&mut self, enabled: bool) {
        self.color_enabled = enabled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OutputSource, OutputStream};

    #[tokio::test]
    async fn test_std_output_director() {
        let mut director = StdOutputDirector::new();
        let source = OutputSource::with_color("test-container", "container", "blue");
        let stream = OutputStream::stdout(b"Hello, World!\n".to_vec());

        // This test mainly ensures the trait implementation compiles and runs
        let result = director.write_output(&source, &stream).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_output_director_flush() {
        let mut director = StdOutputDirector::new();
        let result = director.flush().await;
        assert!(result.is_ok());
    }
}

use super::{BufferedOutputDirector, FileOutputDirector, OutputDirector, StdOutputDirector};
use rush_core::error::Result;
use std::path::Path;

/// Configuration for output director creation
#[derive(Debug, Clone)]
pub enum OutputDirectorConfig {
    /// Standard output/error (default)
    Stdout {
        /// Whether to enable colored output
        colored: bool,
        /// Whether to use buffering
        buffered: bool,
    },
    /// File-based output to a directory
    Files {
        /// Directory to write log files to
        output_dir: String,
        /// Whether to include timestamps in log entries
        include_timestamps: bool,
        /// Whether to include source names in log entries
        include_source_names: bool,
        /// Whether to use buffering
        buffered: bool,
    },
    /// Both stdout and files
    Both {
        /// Stdout configuration
        stdout_colored: bool,
        /// File output directory
        output_dir: String,
        /// Whether to include timestamps in file logs
        include_timestamps: bool,
        /// Whether to include source names in file logs
        include_source_names: bool,
        /// Whether to use buffering
        buffered: bool,
    },
}

impl Default for OutputDirectorConfig {
    fn default() -> Self {
        Self::Stdout {
            colored: true,
            buffered: true,
        }
    }
}

/// Factory for creating output directors
pub struct OutputDirectorFactory;

impl OutputDirectorFactory {
    /// Create an output director based on configuration
    pub async fn create(config: OutputDirectorConfig) -> Result<Box<dyn OutputDirector + Send>> {
        eprintln!(
            "DEBUG: OutputDirectorFactory::create called with config: {config:?}"
        );
        match config {
            OutputDirectorConfig::Stdout { colored, buffered } => {
                let mut std_director = StdOutputDirector::new();
                std_director.set_color_enabled(colored);

                if buffered {
                    Ok(Box::new(BufferedOutputDirector::new(std_director)))
                } else {
                    Ok(Box::new(std_director))
                }
            }

            OutputDirectorConfig::Files {
                output_dir,
                include_timestamps,
                include_source_names,
                buffered,
            } => {
                let file_director = FileOutputDirector::new_with_options(
                    &output_dir,
                    include_timestamps,
                    include_source_names,
                )
                .await?;

                if buffered {
                    Ok(Box::new(BufferedOutputDirector::new(file_director)))
                } else {
                    Ok(Box::new(file_director))
                }
            }

            OutputDirectorConfig::Both {
                stdout_colored,
                output_dir,
                include_timestamps,
                include_source_names,
                buffered,
            } => {
                // Create a combined director that writes to both stdout and files
                let combined = CombinedOutputDirector::new(
                    stdout_colored,
                    &output_dir,
                    include_timestamps,
                    include_source_names,
                )
                .await?;

                if buffered {
                    Ok(Box::new(BufferedOutputDirector::new(combined)))
                } else {
                    Ok(Box::new(combined))
                }
            }
        }
    }

    /// Parse output director configuration from command line arguments
    pub fn parse_from_args(
        output_type: Option<&str>,
        output_dir: Option<&str>,
        no_color: bool,
        no_timestamps: bool,
        no_source_names: bool,
        no_buffering: bool,
    ) -> OutputDirectorConfig {
        let colored = !no_color;
        let include_timestamps = !no_timestamps;
        let include_source_names = !no_source_names;
        let buffered = !no_buffering;

        match output_type {
            Some("stdout") | None => OutputDirectorConfig::Stdout { colored, buffered },

            Some("files") => OutputDirectorConfig::Files {
                output_dir: output_dir.unwrap_or("logs").to_string(),
                include_timestamps,
                include_source_names,
                buffered,
            },

            Some("both") => OutputDirectorConfig::Both {
                stdout_colored: colored,
                output_dir: output_dir.unwrap_or("logs").to_string(),
                include_timestamps,
                include_source_names,
                buffered,
            },

            _ => {
                eprintln!(
                    "Warning: Unknown output type '{}', defaulting to stdout",
                    output_type.unwrap()
                );
                OutputDirectorConfig::default()
            }
        }
    }
}

/// Combined output director that writes to both stdout and files
pub struct CombinedOutputDirector {
    stdout_director: StdOutputDirector,
    file_director: FileOutputDirector,
}

impl CombinedOutputDirector {
    pub async fn new<P: AsRef<Path>>(
        stdout_colored: bool,
        output_dir: P,
        include_timestamps: bool,
        include_source_names: bool,
    ) -> Result<Self> {
        let mut stdout_director = StdOutputDirector::new();
        stdout_director.set_color_enabled(stdout_colored);

        let file_director = FileOutputDirector::new_with_options(
            output_dir,
            include_timestamps,
            include_source_names,
        )
        .await?;

        Ok(Self {
            stdout_director,
            file_director,
        })
    }
}

#[async_trait::async_trait]
impl OutputDirector for CombinedOutputDirector {
    async fn write_output(
        &mut self,
        source: &super::OutputSource,
        stream: &super::OutputStream,
    ) -> Result<()> {
        // Write to both stdout and file
        let stdout_result = self.stdout_director.write_output(source, stream).await;
        let file_result = self.file_director.write_output(source, stream).await;

        // Return error if either failed
        stdout_result?;
        file_result?;
        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        let stdout_result = self.stdout_director.flush().await;
        let file_result = self.file_director.flush().await;

        // Return error if either failed
        stdout_result?;
        file_result?;
        Ok(())
    }

    fn supports_color(&self) -> bool {
        self.stdout_director.supports_color()
    }

    fn set_color_enabled(&mut self, enabled: bool) {
        self.stdout_director.set_color_enabled(enabled);
        self.file_director.set_color_enabled(enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_factory_create_stdout() {
        let config = OutputDirectorConfig::Stdout {
            colored: true,
            buffered: true,
        };

        let director = OutputDirectorFactory::create(config).await;
        assert!(director.is_ok());
    }

    #[tokio::test]
    async fn test_factory_create_files() {
        let temp_dir = TempDir::new().unwrap();
        let config = OutputDirectorConfig::Files {
            output_dir: temp_dir.path().to_string_lossy().to_string(),
            include_timestamps: true,
            include_source_names: true,
            buffered: false,
        };

        let director = OutputDirectorFactory::create(config).await;
        assert!(director.is_ok());
    }

    #[test]
    fn test_parse_from_args_stdout() {
        let config = OutputDirectorFactory::parse_from_args(
            Some("stdout"),
            None,
            false,
            false,
            false,
            false,
        );

        matches!(
            config,
            OutputDirectorConfig::Stdout {
                colored: true,
                buffered: true
            }
        );
    }

    #[test]
    fn test_parse_from_args_files() {
        let config = OutputDirectorFactory::parse_from_args(
            Some("files"),
            Some("/tmp/logs"),
            false,
            false,
            false,
            false,
        );

        matches!(config, OutputDirectorConfig::Files {
            output_dir,
            include_timestamps: true,
            include_source_names: true,
            buffered: true
        } if output_dir == "/tmp/logs");
    }
}

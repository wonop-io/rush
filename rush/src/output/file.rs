use super::{OutputDirector, OutputSource, OutputStream};
use crate::error::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncWriteExt, BufWriter};

/// File-based output director that writes to separate log files per source
pub struct FileOutputDirector {
    /// Base directory for log files
    output_dir: PathBuf,
    /// Map of source names to their file writers
    writers: HashMap<String, BufWriter<File>>,
    /// Whether to include timestamps
    include_timestamps: bool,
    /// Whether to include source names in output
    include_source_names: bool,
}

impl FileOutputDirector {
    /// Create a new file output director
    pub async fn new<P: AsRef<Path>>(output_dir: P) -> Result<Self> {
        let output_dir = output_dir.as_ref().to_path_buf();

        // Create output directory if it doesn't exist
        if !output_dir.exists() {
            tokio::fs::create_dir_all(&output_dir)
                .await
                .map_err(|e| crate::error::Error::Io(e))?;
        }

        Ok(Self {
            output_dir,
            writers: HashMap::new(),
            include_timestamps: true,
            include_source_names: true,
        })
    }

    /// Create a new file output director with custom options
    pub async fn new_with_options<P: AsRef<Path>>(
        output_dir: P,
        include_timestamps: bool,
        include_source_names: bool,
    ) -> Result<Self> {
        let mut director = Self::new(output_dir).await?;
        director.include_timestamps = include_timestamps;
        director.include_source_names = include_source_names;
        Ok(director)
    }

    /// Get or create a writer for a specific source
    async fn get_writer(&mut self, source: &OutputSource) -> Result<&mut BufWriter<File>> {
        if !self.writers.contains_key(&source.name) {
            let filename = format!("{}.log", sanitize_filename(&source.name));
            let file_path = self.output_dir.join(filename);

            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&file_path)
                .await
                .map_err(|e| crate::error::Error::Io(e))?;

            let writer = BufWriter::new(file);
            self.writers.insert(source.name.clone(), writer);
        }

        Ok(self.writers.get_mut(&source.name).unwrap())
    }

    /// Format a log line with optional timestamp and source name
    fn format_log_line(&self, source: &OutputSource, content: &str) -> String {
        let mut line = String::new();

        if self.include_timestamps {
            let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
            line.push_str(&format!("[{}] ", timestamp));
        }

        if self.include_source_names {
            line.push_str(&format!("[{}] ", source.name));
        }

        line.push_str(content);

        // Ensure line ends with newline if it doesn't already
        if !line.ends_with('\n') {
            line.push('\n');
        }

        line
    }
}

#[async_trait]
impl OutputDirector for FileOutputDirector {
    async fn write_output(&mut self, source: &OutputSource, stream: &OutputStream) -> Result<()> {
        eprintln!(
            "DEBUG: FileOutputDirector writing output for source: {} to dir: {:?}",
            source.name, self.output_dir
        );
        if stream.is_empty() {
            return Ok(());
        }

        let content = stream.as_str();

        // Prepare all formatted lines first to avoid borrowing issues
        let mut formatted_lines = Vec::new();

        // Handle line-by-line output
        for line in content.lines() {
            let formatted_line = self.format_log_line(source, line);
            formatted_lines.push(formatted_line);
        }

        // Handle content that doesn't end with newline
        if !content.ends_with('\n') && !content.ends_with("\r\n") && !content.is_empty() {
            let formatted_line = self.format_log_line(source, &content);
            formatted_lines.push(formatted_line);
        }

        // Now get the writer and write all lines
        let writer = self.get_writer(source).await?;
        for line in formatted_lines {
            writer
                .write_all(line.as_bytes())
                .await
                .map_err(|e| crate::error::Error::Io(e))?;
        }

        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        for writer in self.writers.values_mut() {
            writer
                .flush()
                .await
                .map_err(|e| crate::error::Error::Io(e))?;
        }
        Ok(())
    }

    fn supports_color(&self) -> bool {
        false
    }

    fn set_color_enabled(&mut self, _enabled: bool) {
        // File output doesn't support color
    }
}

/// Sanitize a filename by replacing problematic characters
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{OutputSource, OutputStream};
    use tempfile::TempDir;
    use tokio::fs;

    #[tokio::test]
    async fn test_file_output_director_creation() {
        let temp_dir = TempDir::new().unwrap();
        let director = FileOutputDirector::new(temp_dir.path()).await;
        assert!(director.is_ok());
    }

    #[tokio::test]
    async fn test_file_output_director_write() {
        let temp_dir = TempDir::new().unwrap();
        let mut director = FileOutputDirector::new(temp_dir.path()).await.unwrap();

        let source = OutputSource::new("test-container", "container");
        let stream = OutputStream::stdout(b"Hello, World!\n".to_vec());

        let result = director.write_output(&source, &stream).await;
        assert!(result.is_ok());

        // Flush to ensure data is written
        director.flush().await.unwrap();

        // Check that file was created
        let log_file = temp_dir.path().join("test-container.log");
        assert!(log_file.exists());

        // Verify content
        let content = fs::read_to_string(log_file).await.unwrap();
        assert!(content.contains("Hello, World!"));
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("test-container"), "test-container");
        assert_eq!(
            sanitize_filename("test/container:name"),
            "test-container-name"
        );
        assert_eq!(sanitize_filename("test*?<>|\"\\"), "test-------");
    }
}

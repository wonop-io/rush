use super::{OutputDirector, OutputSource, OutputStream, OutputStreamType};
use crate::error::Result;
use async_trait::async_trait;
use std::collections::HashMap;

/// A buffered output director that accumulates partial lines before writing
pub struct BufferedOutputDirector<T: OutputDirector> {
    /// The underlying director to write to
    director: T,
    /// Buffer for accumulating partial lines per source and stream type
    buffers: HashMap<String, HashMap<OutputStreamType, Vec<u8>>>,
}

impl<T: OutputDirector> BufferedOutputDirector<T> {
    /// Create a new buffered output director wrapping another director
    pub fn new(director: T) -> Self {
        Self {
            director,
            buffers: HashMap::new(),
        }
    }

    /// Get the buffer key for a source and stream type
    fn get_buffer_key(&self, source: &OutputSource, stream_type: OutputStreamType) -> String {
        format!("{}:{:?}", source.name, stream_type)
    }

    /// Get or create the buffer for a source and stream type
    fn get_buffer_mut(
        &mut self,
        source: &OutputSource,
        stream_type: OutputStreamType,
    ) -> &mut Vec<u8> {
        let _key = self.get_buffer_key(source, stream_type);
        self.buffers
            .entry(source.name.clone())
            .or_default()
            .entry(stream_type)
            .or_default()
    }

    /// Process buffered data and extract complete lines
    fn extract_complete_lines(
        &mut self,
        source: &OutputSource,
        stream_type: OutputStreamType,
    ) -> Vec<OutputStream> {
        let buffer = self.get_buffer_mut(source, stream_type);
        let mut lines = Vec::new();

        let data = buffer.clone();
        buffer.clear();

        let mut start = 0;
        for (i, &byte) in data.iter().enumerate() {
            if byte == b'\n' {
                let line_data = data[start..=i].to_vec();
                if !line_data.is_empty() {
                    lines.push(OutputStream::new(stream_type, line_data));
                }
                start = i + 1;
            }
        }

        // If there's remaining data without a newline, keep it in the buffer
        if start < data.len() {
            buffer.extend_from_slice(&data[start..]);
        }

        lines
    }
}

#[async_trait]
impl<T: OutputDirector> OutputDirector for BufferedOutputDirector<T> {
    async fn write_output(&mut self, source: &OutputSource, stream: &OutputStream) -> Result<()> {
        if stream.is_empty() {
            return Ok(());
        }

        // Add the new data to the buffer
        let buffer = self.get_buffer_mut(source, stream.stream_type);
        buffer.extend_from_slice(&stream.data);

        // Extract and write complete lines
        let complete_lines = self.extract_complete_lines(source, stream.stream_type);

        for line in complete_lines {
            self.director.write_output(source, &line).await?;
        }

        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        // Flush any remaining buffered data
        let mut sources_to_flush = Vec::new();

        for source_name in self.buffers.keys() {
            sources_to_flush.push(source_name.clone());
        }

        for source_name in sources_to_flush {
            if let Some(stream_buffers) = self.buffers.get(&source_name).cloned() {
                for (stream_type, buffer_data) in stream_buffers {
                    if !buffer_data.is_empty() {
                        let source = OutputSource::new(source_name.clone(), "buffered");
                        let stream = OutputStream::new(stream_type, buffer_data);
                        self.director.write_output(&source, &stream).await?;
                    }
                }
            }
        }

        // Clear all buffers
        self.buffers.clear();

        // Flush the underlying director
        self.director.flush().await
    }

    fn supports_color(&self) -> bool {
        self.director.supports_color()
    }

    fn set_color_enabled(&mut self, enabled: bool) {
        self.director.set_color_enabled(enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::director::StdOutputDirector;

    #[tokio::test]
    async fn test_buffered_output_director_complete_lines() {
        let std_director = StdOutputDirector::new();
        let mut buffered = BufferedOutputDirector::new(std_director);

        let source = OutputSource::new("test", "container");

        // Write partial line
        let partial = OutputStream::stdout(b"Hello, ".to_vec());
        let result = buffered.write_output(&source, &partial).await;
        assert!(result.is_ok());

        // Complete the line
        let complete = OutputStream::stdout(b"World!\n".to_vec());
        let result = buffered.write_output(&source, &complete).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_buffered_output_director_flush() {
        let std_director = StdOutputDirector::new();
        let mut buffered = BufferedOutputDirector::new(std_director);

        let source = OutputSource::new("test", "container");
        let partial = OutputStream::stdout(b"Incomplete line".to_vec());

        let result = buffered.write_output(&source, &partial).await;
        assert!(result.is_ok());

        // Flush should output the incomplete line
        let result = buffered.flush().await;
        assert!(result.is_ok());
    }
}

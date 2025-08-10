/// Type of output stream
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputStreamType {
    Stdout,
    Stderr,
    Combined,
}

/// Represents an output stream with its data
#[derive(Debug, Clone)]
pub struct OutputStream {
    /// Type of stream
    pub stream_type: OutputStreamType,
    /// Raw byte data
    pub data: Vec<u8>,
    /// Whether this is a complete line (ends with newline)
    pub is_complete_line: bool,
}

impl OutputStream {
    /// Create a new output stream
    pub fn new(stream_type: OutputStreamType, data: Vec<u8>) -> Self {
        let is_complete_line = data.ends_with(b"\n") || data.ends_with(b"\r\n");
        Self {
            stream_type,
            data,
            is_complete_line,
        }
    }

    /// Create a stdout stream
    pub fn stdout(data: Vec<u8>) -> Self {
        Self::new(OutputStreamType::Stdout, data)
    }

    /// Create a stderr stream
    pub fn stderr(data: Vec<u8>) -> Self {
        Self::new(OutputStreamType::Stderr, data)
    }

    /// Get the data as a string (lossy conversion)
    pub fn as_string(&self) -> String {
        String::from_utf8_lossy(&self.data).to_string()
    }

    /// Get the data as a string slice (lossy conversion)
    pub fn as_str(&self) -> std::borrow::Cow<str> {
        String::from_utf8_lossy(&self.data)
    }

    /// Check if the stream is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get the length of the data
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Append more data to this stream
    pub fn append(&mut self, mut data: Vec<u8>) {
        self.data.append(&mut data);
        self.is_complete_line = self.data.ends_with(b"\n") || self.data.ends_with(b"\r\n");
    }
}
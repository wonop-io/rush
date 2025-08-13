use serde::{Deserialize, Serialize};
use std::fmt::Display;

/// Represents the source of output data
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputSource {
    /// The name/label of the source (e.g., "helloworld.wonop.io-database")
    pub name: String,
    /// The type of the source (e.g., "container", "build", "system")
    pub source_type: String,
    /// Optional color for formatting (e.g., "red", "blue", "green")
    pub color: Option<String>,
}

impl OutputSource {
    /// Create a new output source
    pub fn new(name: impl Into<String>, source_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source_type: source_type.into(),
            color: None,
        }
    }

    /// Create a new output source with color
    pub fn with_color(
        name: impl Into<String>,
        source_type: impl Into<String>,
        color: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            source_type: source_type.into(),
            color: Some(color.into()),
        }
    }

    /// Set the color for this source
    pub fn set_color(&mut self, color: impl Into<String>) {
        self.color = Some(color.into());
    }

    /// Get the display name with optional formatting
    pub fn display_name(&self) -> String {
        format!("{:15}", self.name)
    }
}

impl Display for OutputSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

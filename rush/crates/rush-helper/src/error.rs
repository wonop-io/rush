use thiserror::Error;

#[derive(Error, Debug)]
pub enum HelperError {
    #[error("{message}")]
    MissingTool {
        message: String,
        command: Vec<String>,
    },

    #[error("{message}")]
    MissingTarget {
        message: String,
        command: Vec<String>,
    },

    #[error("{message}")]
    ConfigurationError {
        message: String,
        command: Vec<String>,
    },

    #[error("Multiple issues found:\n{issues}")]
    MultipleIssues {
        issues: String,
        commands: Vec<Vec<String>>,
    },

    #[error("Command execution failed: {0}")]
    CommandFailed(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl HelperError {
    pub fn missing_tool(tool: &str, install_cmd: Vec<String>) -> Self {
        Self::MissingTool {
            message: format!("{} is not installed or not in PATH", tool),
            command: install_cmd,
        }
    }

    pub fn missing_target(target: &str) -> Self {
        Self::MissingTarget {
            message: format!("Rust target '{}' is not installed", target),
            command: vec![
                "rustup".to_string(),
                "target".to_string(),
                "add".to_string(),
                target.to_string(),
            ],
        }
    }

    pub fn get_fix_commands(&self) -> Vec<Vec<String>> {
        match self {
            Self::MissingTool { command, .. }
            | Self::MissingTarget { command, .. }
            | Self::ConfigurationError { command, .. } => vec![command.clone()],
            Self::MultipleIssues { commands, .. } => commands.clone(),
            _ => vec![],
        }
    }

    pub fn get_message(&self) -> String {
        self.to_string()
    }
}

pub type HelperResult<T> = Result<T, HelperError>;

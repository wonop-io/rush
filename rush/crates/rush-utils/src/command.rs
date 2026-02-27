//! Unified command execution utilities
//!
//! This module provides a consistent interface for executing external commands
//! with proper error handling, logging, and timeout support.

use std::process::Stdio;
use std::time::Duration;

use log::{debug, error, trace};
use rush_core::{Error, ErrorContext, Result};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;
use tokio::time::timeout;

/// Configuration for command execution
#[derive(Debug, Clone)]
pub struct CommandConfig {
    /// The program to execute
    pub program: String,
    /// Arguments to pass to the program
    pub args: Vec<String>,
    /// Optional working directory
    pub working_dir: Option<String>,
    /// Optional environment variables
    pub env_vars: Vec<(String, String)>,
    /// Optional timeout in seconds
    pub timeout_secs: Option<u64>,
    /// Whether to capture output or inherit stdio
    pub capture_output: bool,
}

impl CommandConfig {
    /// Create a new command configuration
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            working_dir: None,
            env_vars: Vec::new(),
            timeout_secs: Some(300), // Default 5 minute timeout
            capture_output: true,
        }
    }

    /// Add an argument
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add multiple arguments
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(|s| s.into()));
        self
    }

    /// Set working directory
    pub fn working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Add environment variable
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.push((key.into(), value.into()));
        self
    }

    /// Set timeout
    pub fn timeout(mut self, seconds: u64) -> Self {
        self.timeout_secs = Some(seconds);
        self
    }

    /// Disable timeout
    pub fn no_timeout(mut self) -> Self {
        self.timeout_secs = None;
        self
    }

    /// Set whether to capture output
    pub fn capture(mut self, capture: bool) -> Self {
        self.capture_output = capture;
        self
    }
}

/// Result of command execution
#[derive(Debug)]
pub struct CommandOutput {
    /// Exit status code
    pub status: i32,
    /// Captured stdout (if capture_output was true)
    pub stdout: String,
    /// Captured stderr (if capture_output was true)
    pub stderr: String,
}

impl CommandOutput {
    /// Check if the command succeeded (exit code 0)
    pub fn success(&self) -> bool {
        self.status == 0
    }
}

/// Unified command runner
pub struct CommandRunner;

impl CommandRunner {
    /// Execute a command with the given configuration
    pub async fn run(config: CommandConfig) -> Result<CommandOutput> {
        debug!(
            "Executing command: {} {}",
            config.program,
            config.args.join(" ")
        );

        let mut cmd = TokioCommand::new(&config.program);
        cmd.args(&config.args);

        if let Some(dir) = &config.working_dir {
            trace!("Setting working directory: {dir}");
            cmd.current_dir(dir);
        }

        for (key, value) in &config.env_vars {
            trace!("Setting env var: {key}={value}");
            cmd.env(key, value);
        }

        if config.capture_output {
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        } else {
            cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        }

        let fut = cmd.output();

        let output = if let Some(timeout_secs) = config.timeout_secs {
            match timeout(Duration::from_secs(timeout_secs), fut).await {
                Ok(result) => result.context("Failed to execute command")?,
                Err(_) => {
                    return Err(Error::Internal(format!(
                        "Command timed out after {} seconds: {} {}",
                        timeout_secs,
                        config.program,
                        config.args.join(" ")
                    )));
                }
            }
        } else {
            fut.await.context("Failed to execute command")?
        };

        let status = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            error!(
                "Command failed with status {}: {} {}",
                status,
                config.program,
                config.args.join(" ")
            );
            if !stderr.is_empty() {
                error!("Stderr: {stderr}");
            }
        }

        Ok(CommandOutput {
            status,
            stdout,
            stderr,
        })
    }

    /// Execute a command and return stdout if successful
    pub async fn run_output(config: CommandConfig) -> Result<String> {
        let output = Self::run(config).await?;
        if output.success() {
            Ok(output.stdout)
        } else {
            Err(Error::Internal(format!(
                "Command failed with status {}: {}",
                output.status, output.stderr
            )))
        }
    }

    /// Execute a command and check if it succeeded
    pub async fn run_check(config: CommandConfig) -> Result<bool> {
        let output = Self::run(config).await?;
        Ok(output.success())
    }
}

/// Quick helper to run a simple command
pub async fn run_command(program: &str, args: &[&str]) -> Result<CommandOutput> {
    CommandRunner::run(CommandConfig::new(program).args(args.iter().map(|s| s.to_string()))).await
}

/// Quick helper to get command output
pub async fn get_command_output(program: &str, args: &[&str]) -> Result<String> {
    CommandRunner::run_output(CommandConfig::new(program).args(args.iter().map(|s| s.to_string())))
        .await
}

// Keep the existing run_command for backward compatibility, but with a different signature
/// Executes a command and captures its output with formatted label
///
/// # Arguments
///
/// * `formatted_label` - A label to display with command output
/// * `command` - The command to execute
/// * `args` - Arguments for the command
///
/// # Returns
///
/// * `Result<String, String>` - The command output or an error message
pub async fn run_command_with_label(
    formatted_label: &str,
    command: &str,
    args: Vec<&str>,
) -> std::result::Result<String, String> {
    let debug_args = args.join(" ");
    trace!("Running command: {command} {debug_args}");

    // Create process
    let mut child = TokioCommand::new(command)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to execute command: {e}"))?;

    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

    // Create channels for handling output
    let (tx, mut rx) = mpsc::channel(100);
    let stdout_task = tokio::spawn(handle_stream(stdout, tx.clone()));
    let stderr_task = tokio::spawn(handle_stream(stderr, tx));

    // Collect output
    let mut lines = Vec::new();
    while let Some(line) = rx.recv().await {
        trace!("Received line: {}", line.trim_end());
        lines.push(line.trim_end().to_string());
        let clean_line = line.trim_end().replace(['\x1B', '\r', '\n'], "");
        println!("{formatted_label} {clean_line}");
    }

    // Wait for tasks to complete
    let _ = tokio::join!(stdout_task, stderr_task);

    // Wait for child process
    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed to wait for process: {e}"))?;

    let output = lines.join("\n");

    if status.success() {
        Ok(output)
    } else {
        Err(format!(
            "Command failed with status {:?}: {}",
            status.code(),
            output
        ))
    }
}

/// Helper function to handle async reading from a stream
async fn handle_stream<R: AsyncRead + Unpin>(stream: R, tx: mpsc::Sender<String>) -> Result<()> {
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        if tx.send(line).await.is_err() {
            break; // Receiver dropped
        }
    }
    Ok(())
}

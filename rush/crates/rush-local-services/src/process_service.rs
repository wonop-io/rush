//! Process-based local service implementation
//!
//! This module provides a LocalService implementation for local processes like Stripe CLI.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;
use log::warn;
use rush_core::error::{Error, Result};
use rush_output::simple::Sink;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::output::ServiceOutput;
use crate::r#trait::LocalService;
use crate::types::LocalServiceType;

/// Process-based implementation of LocalService
pub struct ProcessLocalService {
    /// Service name
    name: String,

    /// Service type
    service_type: LocalServiceType,

    /// Process handle
    process: Arc<Mutex<Option<Child>>>,

    /// Command to run
    command: String,

    /// Command arguments
    args: Vec<String>,

    /// Environment variables for the process
    env_vars: HashMap<String, String>,

    /// Generated webhook secret (for Stripe)
    webhook_secret: Arc<Mutex<Option<String>>>,

    /// Whether the service is ready
    is_ready: Arc<Mutex<bool>>,

    /// Whether to use PTY (for Stripe CLI)
    use_pty: bool,

    /// Output handler
    output: ServiceOutput,
}

impl ProcessLocalService {
    /// Create a new process-based local service
    pub fn new(
        name: String,
        service_type: LocalServiceType,
        command: String,
        args: Vec<String>,
        env_vars: HashMap<String, String>,
        use_pty: bool,
    ) -> Self {
        let output = ServiceOutput::new(name.clone());
        Self {
            name,
            service_type,
            process: Arc::new(Mutex::new(None)),
            command,
            args,
            env_vars,
            webhook_secret: Arc::new(Mutex::new(None)),
            is_ready: Arc::new(Mutex::new(false)),
            use_pty,
            output,
        }
    }

    /// Create a Stripe CLI service
    pub fn stripe_cli(name: String, webhook_url: String) -> Self {
        // Try to find stripe in PATH, otherwise use common locations
        let stripe_path = std::process::Command::new("which")
            .arg("stripe")
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout)
                        .ok()
                        .map(|s| s.trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                // Try common locations
                if std::path::Path::new("/opt/homebrew/bin/stripe").exists() {
                    "/opt/homebrew/bin/stripe".to_string()
                } else if std::path::Path::new("/usr/local/bin/stripe").exists() {
                    "/usr/local/bin/stripe".to_string()
                } else {
                    "stripe".to_string() // Fallback to PATH
                }
            });

        log::info!("Using Stripe CLI at: {}", stripe_path);

        let args = vec![
            "listen".to_string(),
            "--forward-to".to_string(),
            webhook_url,
            "--skip-verify".to_string(),
        ];

        // Get API key from environment if available
        let mut env_vars = HashMap::new();
        if let Ok(api_key) = std::env::var("STRIPE_API_KEY") {
            env_vars.insert("STRIPE_API_KEY".to_string(), api_key);
        }

        Self::new(
            name,
            LocalServiceType::StripeCLI,
            stripe_path,
            args,
            env_vars,
            true, // Use PTY for Stripe CLI
        )
    }

    /// Start the process with PTY allocation
    async fn start_with_pty(&mut self) -> Result<Child> {
        // Build the command with PTY support using script command for proper PTY allocation
        let mut cmd = if cfg!(target_os = "macos") {
            // macOS: use script to allocate PTY
            // Pass command and args directly to script
            let mut c = Command::new("script");
            c.arg("-q");
            c.arg("/dev/null");
            c.arg(&self.command);
            for arg in &self.args {
                c.arg(arg);
            }
            c
        } else {
            // Linux: use script with -c flag
            // Format: script [-q] [-c command] output_file
            let mut c = Command::new("script");
            c.arg("-q");
            c.arg("-c");
            c.arg(format!("{} {}", self.command, self.args.join(" ")));
            c.arg("/dev/null");
            c
        };

        // Set up stdio for capturing output
        cmd.stdin(Stdio::piped()) // Important: Use piped for PTY to work properly
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Set environment variables
        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }

        // Kill on drop to ensure cleanup
        cmd.kill_on_drop(true);

        cmd.spawn()
            .map_err(|e| Error::Docker(format!("Failed to start {} with PTY: {}", self.name, e)))
    }

    /// Start the process normally
    async fn start_normal(&mut self) -> Result<Child> {
        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Set environment variables
        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }

        cmd.spawn()
            .map_err(|e| Error::Docker(format!("Failed to start {}: {}", self.name, e)))
    }

    /// Parse Stripe CLI output for webhook secret and ready state
    async fn parse_stripe_output(
        output: String,
        webhook_secret: Arc<Mutex<Option<String>>>,
        is_ready: Arc<Mutex<bool>>,
    ) {
        // Look for webhook signing secret in output
        if output.contains("webhook signing secret is") || output.contains("whsec_") {
            if let Some(secret_start) = output.find("whsec_") {
                let secret_end = output[secret_start..]
                    .find(|c: char| c.is_whitespace() || c == '(' || c == ')')
                    .unwrap_or(output[secret_start..].len());
                let secret = &output[secret_start..secret_start + secret_end];

                let mut webhook_secret_guard = webhook_secret.lock().await;
                *webhook_secret_guard = Some(secret.to_string());
                log::info!("Captured Stripe webhook signing secret: whsec_...");
            }
        }

        // Check if Stripe is ready
        if output.contains("Ready!") && output.contains("You are using Stripe API") {
            let mut is_ready_guard = is_ready.lock().await;
            *is_ready_guard = true;
            log::info!("Stripe CLI is ready and listening for webhooks");
        }
    }

    /// Monitor process output
    async fn monitor_output(
        child: &mut Child,
        _name: String,
        service_type: LocalServiceType,
        webhook_secret: Arc<Mutex<Option<String>>>,
        is_ready: Arc<Mutex<bool>>,
        output: ServiceOutput,
    ) {
        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            let is_ready_clone = is_ready.clone();
            let output = output.clone();

            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    // Clean up the line (remove carriage returns and ANSI escape codes)
                    let clean_line = strip_ansi_escapes::strip_str(line.replace("\r", ""));

                    // Forward to output sink
                    output.info(clean_line.clone()).await;

                    // Parse Stripe output if applicable
                    if matches!(service_type, LocalServiceType::StripeCLI) {
                        Self::parse_stripe_output(
                            clean_line,
                            webhook_secret.clone(),
                            is_ready_clone.clone(),
                        )
                        .await;
                    }
                }
            });
        }

        if let Some(stderr) = child.stderr.take() {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            let output_clone = output.clone();

            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    // Forward error to output sink
                    output_clone.error(line).await;
                }
            });
        }
    }
}

#[async_trait]
impl LocalService for ProcessLocalService {
    async fn start(&mut self) -> Result<()> {
        self.output
            .info(format!("Starting process local service: {}", self.name))
            .await;

        // Start the process
        let mut child = if self.use_pty {
            self.start_with_pty().await?
        } else {
            self.start_normal().await?
        };

        // Monitor output
        let webhook_secret = self.webhook_secret.clone();
        let is_ready = self.is_ready.clone();
        Self::monitor_output(
            &mut child,
            self.name.clone(),
            self.service_type.clone(),
            webhook_secret,
            is_ready,
            self.output.clone(),
        )
        .await;

        // Store the process handle
        let mut process_guard = self.process.lock().await;
        *process_guard = Some(child);

        self.output
            .info(format!(
                "Process local service {} started successfully",
                self.name
            ))
            .await;
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        let mut process_guard = self.process.lock().await;
        if let Some(mut child) = process_guard.take() {
            self.output
                .info(format!("Stopping process local service: {}", self.name))
                .await;

            // Try graceful shutdown first
            if let Err(e) = child.kill().await {
                warn!("Failed to kill process {}: {}", self.name, e);
            }

            self.output
                .info(format!("Process local service {} stopped", self.name))
                .await;
        }

        Ok(())
    }

    async fn is_healthy(&self) -> Result<bool> {
        let process_guard = self.process.lock().await;
        if process_guard.is_some() {
            // For Stripe CLI, check if it's ready
            if matches!(self.service_type, LocalServiceType::StripeCLI) {
                let is_ready_guard = self.is_ready.lock().await;
                Ok(*is_ready_guard)
            } else {
                // For other processes, just check if running
                Ok(true)
            }
        } else {
            Ok(false)
        }
    }

    async fn run_post_startup_tasks(&mut self) -> Result<()> {
        // Process services typically don't have post-startup tasks
        // This is mainly for container-based services
        Ok(())
    }

    async fn generated_env_vars(&self) -> Result<HashMap<String, String>> {
        // Most process services don't generate regular env vars
        Ok(HashMap::new())
    }

    async fn generated_env_secrets(&self) -> Result<HashMap<String, String>> {
        let mut secrets = HashMap::new();

        // Add Stripe webhook secret if available
        if matches!(self.service_type, LocalServiceType::StripeCLI) {
            let webhook_secret_guard = self.webhook_secret.lock().await;
            if let Some(secret) = webhook_secret_guard.as_ref() {
                secrets.insert("STRIPE_WEBHOOK_SECRET".to_string(), secret.clone());
            }
        }

        Ok(secrets)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn service_type(&self) -> LocalServiceType {
        self.service_type.clone()
    }

    fn is_running(&self) -> bool {
        // This is a simplified check - could be improved
        let process_guard = self.process.try_lock();
        if let Ok(guard) = process_guard {
            guard.is_some()
        } else {
            true // If we can't get the lock, assume it's running
        }
    }

    fn set_output_sink(&mut self, sink: Arc<Mutex<Box<dyn Sink>>>) {
        self.output.set_sink(sink);
    }
}

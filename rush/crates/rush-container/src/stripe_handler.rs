//! Special handler for Stripe CLI which runs as a local process with PTY

use chrono::Utc;
use log::info;
use rush_core::error::{Error, Result};
use rush_output::simple::{LogEntry, LogOrigin};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// Handle for the Stripe CLI process
pub struct StripeCliHandler {
    child: Option<Child>,
    name: String,
    output_sink: Arc<Mutex<Box<dyn rush_output::simple::Sink>>>,
}

impl StripeCliHandler {
    /// Create a new Stripe CLI handler
    pub fn new(
        name: String,
        output_sink: Arc<Mutex<Box<dyn rush_output::simple::Sink>>>,
    ) -> Self {
        Self {
            child: None,
            name,
            output_sink,
        }
    }

    /// Start the Stripe CLI process
    pub async fn start(&mut self, webhook_url: &str, api_key: Option<&str>) -> Result<()> {
        if self.child.is_some() {
            return Err(Error::Docker("Stripe CLI is already running".to_string()));
        }

        info!("Starting Stripe CLI for webhook forwarding");

        // Build the command with PTY support using script command for proper PTY allocation
        let mut cmd = if cfg!(target_os = "macos") {
            // macOS: use script to allocate PTY
            let mut c = Command::new("script");
            c.arg("-q");
            c.arg("/dev/null");
            c.arg("stripe");
            c
        } else {
            // Linux: use script with different syntax
            let mut c = Command::new("script");
            c.arg("-q");
            c.arg("-c");
            c.arg("stripe listen --forward-to {} --skip-verify");
            c.arg("/dev/null");
            c
        };

        // Add arguments for stripe listen (macOS case)
        if cfg!(target_os = "macos") {
            cmd.arg("listen");
            cmd.arg("--forward-to");
            cmd.arg(webhook_url);
            cmd.arg("--skip-verify");
            
            // Add API key if provided
            if let Some(key) = api_key {
                cmd.arg("--api-key");
                cmd.arg(key);
            }
        }

        // Set environment variables
        if let Some(key) = api_key {
            cmd.env("STRIPE_API_KEY", key);
        }

        // Set up stdio for capturing output
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(Stdio::piped());
        
        // Kill on drop to ensure cleanup
        cmd.kill_on_drop(true);

        // Spawn the process
        let mut child = cmd.spawn().map_err(|e| {
            Error::Docker(format!("Failed to start Stripe CLI: {}", e))
        })?;

        // Set up output capture to send to sink
        if let Some(stdout) = child.stdout.take() {
            let sink = self.output_sink.clone();
            let name = self.name.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    // Send to output sink
                    let entry = LogEntry {
                        component: name.clone(),
                        content: line,
                        timestamp: Utc::now(),
                        is_error: false,
                        log_origin: LogOrigin::Docker,
                    };
                    
                    let mut sink_guard = sink.lock().await;
                    let _ = sink_guard.write(entry).await;
                }
            });
        }

        if let Some(stderr) = child.stderr.take() {
            let sink = self.output_sink.clone();
            let name = self.name.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    // Send to output sink
                    let entry = LogEntry {
                        component: name.clone(),
                        content: line,
                        timestamp: Utc::now(),
                        is_error: true,
                        log_origin: LogOrigin::Docker,
                    };
                    
                    let mut sink_guard = sink.lock().await;
                    let _ = sink_guard.write(entry).await;
                }
            });
        }

        self.child = Some(child);
        info!("Stripe CLI started successfully");
        Ok(())
    }

    /// Stop the Stripe CLI process
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            info!("Stopping Stripe CLI");
            let _ = child.kill().await;
            info!("Stripe CLI stopped");
        }
        Ok(())
    }

    /// Check if the process is running
    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }
}

impl Drop for StripeCliHandler {
    fn drop(&mut self) {
        // Ensure process is killed when handler is dropped
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
        }
    }
}
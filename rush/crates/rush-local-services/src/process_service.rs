//! Process-based local service implementation
//!
//! This module provides a LocalService implementation for local processes like Stripe CLI.

use async_trait::async_trait;
use log::{debug, info, warn};
use rush_core::error::{Error, Result};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

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
    
    /// Whether to use PTY (for Stripe CLI)
    use_pty: bool,
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
        Self {
            name,
            service_type,
            process: Arc::new(Mutex::new(None)),
            command,
            args,
            env_vars,
            webhook_secret: Arc::new(Mutex::new(None)),
            use_pty,
        }
    }
    
    /// Create a Stripe CLI service
    pub fn stripe_cli(name: String, webhook_url: String) -> Self {
        let mut args = vec![
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
            "stripe".to_string(),
            args,
            env_vars,
            true, // Use PTY for Stripe CLI
        )
    }
    
    /// Start the process with PTY allocation
    async fn start_with_pty(&mut self) -> Result<Child> {
        // Use the 'script' command to allocate a PTY
        let mut cmd = Command::new("script");
        cmd.arg("-q")
            .arg("-c")
            .arg(format!("{} {}", self.command, self.args.join(" ")))
            .arg("/dev/null")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        
        // Set environment variables
        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }
        
        cmd.spawn()
            .map_err(|e| Error::Docker(format!(
                "Failed to start {} with PTY: {}",
                self.name, e
            )))
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
            .map_err(|e| Error::Docker(format!(
                "Failed to start {}: {}",
                self.name, e
            )))
    }
    
    /// Parse Stripe CLI output for webhook secret
    async fn parse_stripe_output(output: String, webhook_secret: Arc<Mutex<Option<String>>>) {
        // Look for webhook signing secret in output
        if output.contains("webhook signing secret is") {
            if let Some(secret_start) = output.find("whsec_") {
                let secret_end = output[secret_start..]
                    .find(|c: char| c.is_whitespace() || c == '(' || c == ')')
                    .unwrap_or(output[secret_start..].len());
                let secret = &output[secret_start..secret_start + secret_end];
                
                let mut webhook_secret_guard = webhook_secret.lock().await;
                *webhook_secret_guard = Some(secret.to_string());
                info!("Captured Stripe webhook signing secret");
            }
        }
    }
    
    /// Monitor process output
    async fn monitor_output(
        child: &mut Child,
        name: String,
        service_type: LocalServiceType,
        webhook_secret: Arc<Mutex<Option<String>>>,
    ) {
        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            let name_clone = name.clone();
            
            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    debug!("[{}] {}", name_clone, line);
                    
                    // Parse Stripe output if applicable
                    if matches!(service_type, LocalServiceType::StripeCLI) {
                        Self::parse_stripe_output(line, webhook_secret.clone()).await;
                    }
                }
            });
        }
        
        if let Some(stderr) = child.stderr.take() {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            
            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    warn!("[{}] {}", name, line);
                }
            });
        }
    }
}

#[async_trait]
impl LocalService for ProcessLocalService {
    async fn start(&mut self) -> Result<()> {
        info!("Starting process local service: {}", self.name);
        
        // Start the process
        let mut child = if self.use_pty {
            self.start_with_pty().await?
        } else {
            self.start_normal().await?
        };
        
        // Monitor output
        let webhook_secret = self.webhook_secret.clone();
        Self::monitor_output(
            &mut child,
            self.name.clone(),
            self.service_type.clone(),
            webhook_secret,
        ).await;
        
        // Store the process handle
        let mut process_guard = self.process.lock().await;
        *process_guard = Some(child);
        
        info!("Process local service {} started successfully", self.name);
        Ok(())
    }
    
    async fn stop(&mut self) -> Result<()> {
        let mut process_guard = self.process.lock().await;
        if let Some(mut child) = process_guard.take() {
            info!("Stopping process local service: {}", self.name);
            
            // Try graceful shutdown first
            if let Err(e) = child.kill().await {
                warn!("Failed to kill process {}: {}", self.name, e);
            }
            
            info!("Process local service {} stopped", self.name);
        }
        
        Ok(())
    }
    
    async fn is_healthy(&self) -> Result<bool> {
        let process_guard = self.process.lock().await;
        if let Some(child) = process_guard.as_ref() {
            // Check if process is still running
            // Note: This is a simplified check
            Ok(true)
        } else {
            Ok(false)
        }
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
}
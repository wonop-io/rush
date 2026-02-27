//! Deployment hooks for pre and post deployment actions
//!
//! This module provides a flexible hook system for running custom actions
//! before and after deployments, including validation, notifications, and custom scripts.

use std::collections::HashMap;
use std::process::Stdio;

use async_trait::async_trait;
use log::{debug, error, info, warn};
use rush_core::{Error, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

/// Hook execution context containing deployment information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookContext {
    /// Product name being deployed
    pub product_name: String,
    /// Environment (dev, staging, production)
    pub environment: String,
    /// Deployment version or tag
    pub version: String,
    /// Whether this is a dry run
    pub dry_run: bool,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Hook execution result
#[derive(Debug, Clone)]
pub struct HookResult {
    /// Whether the hook succeeded
    pub success: bool,
    /// Output message from the hook
    pub message: String,
    /// Additional data returned by the hook
    pub data: Option<HashMap<String, String>>,
}

/// Trait for implementing deployment hooks
#[async_trait]
pub trait DeploymentHook: Send + Sync {
    /// Hook name for identification
    fn name(&self) -> &str;

    /// Execute the hook with given context
    async fn execute(&self, context: &HookContext) -> Result<HookResult>;

    /// Whether this hook is required (deployment fails if hook fails)
    fn required(&self) -> bool {
        true
    }

    /// Whether to run this hook in dry-run mode
    fn run_in_dry_run(&self) -> bool {
        false
    }
}

/// Script-based hook that executes a shell script
pub struct ScriptHook {
    name: String,
    script_path: String,
    required: bool,
    run_in_dry_run: bool,
    timeout_seconds: u64,
}

impl ScriptHook {
    pub fn new(name: String, script_path: String) -> Self {
        Self {
            name,
            script_path,
            required: true,
            run_in_dry_run: false,
            timeout_seconds: 60,
        }
    }

    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout_seconds = seconds;
        self
    }

    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    pub fn with_dry_run(mut self) -> Self {
        self.run_in_dry_run = true;
        self
    }
}

#[async_trait]
impl DeploymentHook for ScriptHook {
    fn name(&self) -> &str {
        &self.name
    }

    async fn execute(&self, context: &HookContext) -> Result<HookResult> {
        info!("Executing script hook: {}", self.name);

        // Serialize context to JSON for the script
        let context_json =
            serde_json::to_string(context).map_err(|e| Error::Serialization(e.to_string()))?;

        let output = Command::new(&self.script_path)
            .env("HOOK_CONTEXT", context_json)
            .env("PRODUCT_NAME", &context.product_name)
            .env("ENVIRONMENT", &context.environment)
            .env("VERSION", &context.version)
            .env("DRY_RUN", context.dry_run.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Command(format!("Failed to execute hook script: {e}")))?;

        let success = output.status.success();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !success {
            error!("Hook {} failed: {}", self.name, stderr);
        }

        Ok(HookResult {
            success,
            message: if success {
                stdout.to_string()
            } else {
                stderr.to_string()
            },
            data: None,
        })
    }

    fn required(&self) -> bool {
        self.required
    }

    fn run_in_dry_run(&self) -> bool {
        self.run_in_dry_run
    }
}

/// Webhook notification hook
pub struct WebhookHook {
    name: String,
    url: String,
    required: bool,
    timeout_seconds: u64,
}

impl WebhookHook {
    pub fn new(name: String, url: String) -> Self {
        Self {
            name,
            url,
            required: false,
            timeout_seconds: 30,
        }
    }
}

#[async_trait]
impl DeploymentHook for WebhookHook {
    fn name(&self) -> &str {
        &self.name
    }

    async fn execute(&self, context: &HookContext) -> Result<HookResult> {
        info!("Sending webhook notification: {}", self.name);

        // Create HTTP client
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(self.timeout_seconds))
            .build()
            .map_err(|e| Error::Network(e.to_string()))?;

        // Send POST request with context as JSON
        let response = client
            .post(&self.url)
            .json(context)
            .send()
            .await
            .map_err(|e| Error::Network(format!("Webhook failed: {e}")))?;

        let success = response.status().is_success();
        let message = response
            .text()
            .await
            .unwrap_or_else(|_| "No response".to_string());

        Ok(HookResult {
            success,
            message,
            data: None,
        })
    }

    fn required(&self) -> bool {
        self.required
    }
}

/// Validation hook for pre-deployment checks
pub struct ValidationHook {
    name: String,
    validators: Vec<Box<dyn Validator>>,
}

/// Trait for validators
#[async_trait]
pub trait Validator: Send + Sync {
    async fn validate(&self, context: &HookContext) -> Result<bool>;
    fn error_message(&self) -> String;
}

/// Resource quota validator
pub struct ResourceQuotaValidator {
    max_replicas: u32,
    max_memory_gb: u32,
    max_cpu_cores: u32,
}

#[async_trait]
impl Validator for ResourceQuotaValidator {
    async fn validate(&self, _context: &HookContext) -> Result<bool> {
        // TODO: Check actual resource usage from manifests
        Ok(true)
    }

    fn error_message(&self) -> String {
        format!(
            "Resource limits exceeded (max replicas: {}, max memory: {}GB, max CPU: {} cores)",
            self.max_replicas, self.max_memory_gb, self.max_cpu_cores
        )
    }
}

impl ValidationHook {
    pub fn new(name: String) -> Self {
        Self {
            name,
            validators: Vec::new(),
        }
    }

    pub fn add_validator(mut self, validator: Box<dyn Validator>) -> Self {
        self.validators.push(validator);
        self
    }
}

#[async_trait]
impl DeploymentHook for ValidationHook {
    fn name(&self) -> &str {
        &self.name
    }

    async fn execute(&self, context: &HookContext) -> Result<HookResult> {
        info!("Running validation hook: {}", self.name);

        for validator in &self.validators {
            if !validator.validate(context).await? {
                return Ok(HookResult {
                    success: false,
                    message: validator.error_message(),
                    data: None,
                });
            }
        }

        Ok(HookResult {
            success: true,
            message: "All validations passed".to_string(),
            data: None,
        })
    }
}

/// Hook manager that coordinates hook execution
pub struct HookManager {
    pre_deploy_hooks: Vec<Box<dyn DeploymentHook>>,
    post_deploy_hooks: Vec<Box<dyn DeploymentHook>>,
}

impl Default for HookManager {
    fn default() -> Self {
        Self::new()
    }
}

impl HookManager {
    pub fn new() -> Self {
        Self {
            pre_deploy_hooks: Vec::new(),
            post_deploy_hooks: Vec::new(),
        }
    }

    /// Add a pre-deployment hook
    pub fn add_pre_deploy_hook(&mut self, hook: Box<dyn DeploymentHook>) {
        self.pre_deploy_hooks.push(hook);
    }

    /// Add a post-deployment hook
    pub fn add_post_deploy_hook(&mut self, hook: Box<dyn DeploymentHook>) {
        self.post_deploy_hooks.push(hook);
    }

    /// Execute all pre-deployment hooks
    pub async fn run_pre_deploy_hooks(&self, context: &HookContext) -> Result<()> {
        info!("Running pre-deployment hooks");

        for hook in &self.pre_deploy_hooks {
            if context.dry_run && !hook.run_in_dry_run() {
                debug!("Skipping hook {} in dry-run mode", hook.name());
                continue;
            }

            let result = hook.execute(context).await?;

            if !result.success && hook.required() {
                return Err(Error::Hook(format!(
                    "Pre-deployment hook '{}' failed: {}",
                    hook.name(),
                    result.message
                )));
            } else if !result.success {
                warn!(
                    "Optional pre-deployment hook '{}' failed: {}",
                    hook.name(),
                    result.message
                );
            } else {
                info!("Pre-deployment hook '{}' succeeded", hook.name());
            }
        }

        Ok(())
    }

    /// Execute all post-deployment hooks
    pub async fn run_post_deploy_hooks(&self, context: &HookContext) -> Result<()> {
        info!("Running post-deployment hooks");

        for hook in &self.post_deploy_hooks {
            if context.dry_run && !hook.run_in_dry_run() {
                debug!("Skipping hook {} in dry-run mode", hook.name());
                continue;
            }

            let result = hook.execute(context).await?;

            if !result.success && hook.required() {
                return Err(Error::Hook(format!(
                    "Post-deployment hook '{}' failed: {}",
                    hook.name(),
                    result.message
                )));
            } else if !result.success {
                warn!(
                    "Optional post-deployment hook '{}' failed: {}",
                    hook.name(),
                    result.message
                );
            } else {
                info!("Post-deployment hook '{}' succeeded", hook.name());
            }
        }

        Ok(())
    }

    /// Load hooks from configuration
    pub fn load_from_config(config: &HookConfig) -> Result<Self> {
        let mut manager = Self::new();

        // Load pre-deploy hooks
        for hook_cfg in &config.pre_deploy {
            let hook = Self::create_hook_from_config(hook_cfg)?;
            manager.add_pre_deploy_hook(hook);
        }

        // Load post-deploy hooks
        for hook_cfg in &config.post_deploy {
            let hook = Self::create_hook_from_config(hook_cfg)?;
            manager.add_post_deploy_hook(hook);
        }

        Ok(manager)
    }

    fn create_hook_from_config(config: &HookConfigItem) -> Result<Box<dyn DeploymentHook>> {
        match config.hook_type.as_str() {
            "script" => {
                let mut hook = ScriptHook::new(
                    config.name.clone(),
                    config.script_path.clone().ok_or_else(|| {
                        Error::Configuration("Script path required for script hook".to_string())
                    })?,
                );

                if let Some(timeout) = config.timeout {
                    hook = hook.with_timeout(timeout);
                }

                if !config.required.unwrap_or(true) {
                    hook = hook.optional();
                }

                if config.run_in_dry_run.unwrap_or(false) {
                    hook = hook.with_dry_run();
                }

                Ok(Box::new(hook))
            }
            "webhook" => {
                let hook = WebhookHook::new(
                    config.name.clone(),
                    config.url.clone().ok_or_else(|| {
                        Error::Configuration("URL required for webhook hook".to_string())
                    })?,
                );

                Ok(Box::new(hook))
            }
            "validation" => {
                let hook = ValidationHook::new(config.name.clone());
                // TODO: Add validators based on config
                Ok(Box::new(hook))
            }
            _ => Err(Error::Configuration(format!(
                "Unknown hook type: {}",
                config.hook_type
            ))),
        }
    }
}

/// Hook configuration loaded from YAML
#[derive(Debug, Clone, Deserialize)]
pub struct HookConfig {
    pub pre_deploy: Vec<HookConfigItem>,
    pub post_deploy: Vec<HookConfigItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HookConfigItem {
    pub name: String,
    pub hook_type: String,
    pub script_path: Option<String>,
    pub url: Option<String>,
    pub required: Option<bool>,
    pub run_in_dry_run: Option<bool>,
    pub timeout: Option<u64>,
}

//! Command middleware system for cross-cutting concerns
//!
//! This module provides a middleware pipeline for processing commands,
//! enabling features like logging, metrics, validation, and authorization.

use async_trait::async_trait;
use std::any::Any;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::{Error, Result};

/// Command context passed through middleware
#[derive(Debug, Clone)]
pub struct CommandContext {
    /// Command name
    pub command: String,
    /// Command arguments
    pub args: Vec<String>,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Request metadata
    pub metadata: HashMap<String, String>,
    /// Execution start time
    pub start_time: Instant,
    /// Custom data storage
    data: Arc<RwLock<HashMap<String, Box<dyn Any + Send + Sync>>>>,
}

impl CommandContext {
    /// Create a new command context
    pub fn new(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            command: command.into(),
            args,
            env: std::env::vars().collect(),
            metadata: HashMap::new(),
            start_time: Instant::now(),
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Set custom data
    pub async fn set_data<T: Any + Send + Sync + 'static>(
        &self,
        key: impl Into<String>,
        value: T,
    ) {
        let mut data = self.data.write().await;
        data.insert(key.into(), Box::new(value));
    }
    
    /// Get custom data
    pub async fn get_data<T: Any + Send + Sync + 'static>(&self, key: &str) -> Option<T>
    where
        T: Clone,
    {
        let data = self.data.read().await;
        data.get(key).and_then(|v| v.downcast_ref::<T>()).cloned()
    }
    
    /// Get elapsed time since context creation
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
}

/// Command response from middleware
#[derive(Debug)]
pub struct CommandResponse {
    /// Whether the command succeeded
    pub success: bool,
    /// Optional result data
    pub data: Option<Box<dyn Any + Send + Sync>>,
    /// Error if failed
    pub error: Option<Error>,
    /// Response metadata
    pub metadata: HashMap<String, String>,
}

impl CommandResponse {
    /// Create a successful response
    pub fn success() -> Self {
        Self {
            success: true,
            data: None,
            error: None,
            metadata: HashMap::new(),
        }
    }
    
    /// Create a successful response with data
    pub fn with_data<T: Any + Send + Sync + 'static>(data: T) -> Self {
        Self {
            success: true,
            data: Some(Box::new(data)),
            error: None,
            metadata: HashMap::new(),
        }
    }
    
    /// Create an error response
    pub fn error(error: Error) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
            metadata: HashMap::new(),
        }
    }
}

/// Middleware trait for command processing
#[async_trait]
pub trait Middleware: Send + Sync + Debug {
    /// Process the command, calling the next middleware in the chain
    async fn process(
        &self,
        ctx: &CommandContext,
        next: Next<'_>,
    ) -> Result<CommandResponse>;
    
    /// Get middleware name
    fn name(&self) -> &str;
    
    /// Get middleware priority (lower = higher priority)
    fn priority(&self) -> i32 {
        50
    }
}

/// Type alias for the next middleware in the chain
pub type Next<'a> = Box<dyn FnOnce(&CommandContext) -> BoxedFuture<'a, Result<CommandResponse>> + Send + 'a>;

/// Type alias for boxed future
pub type BoxedFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// Logging middleware
#[derive(Debug)]
pub struct LoggingMiddleware {
    level: log::Level,
}

impl LoggingMiddleware {
    pub fn new(level: log::Level) -> Self {
        Self { level }
    }
}

impl Default for LoggingMiddleware {
    fn default() -> Self {
        Self::new(log::Level::Info)
    }
}

#[async_trait]
impl Middleware for LoggingMiddleware {
    async fn process(
        &self,
        ctx: &CommandContext,
        next: Next<'_>,
    ) -> Result<CommandResponse> {
        log::log!(
            self.level,
            "Starting command: {} with args: {:?}",
            ctx.command,
            ctx.args
        );
        
        let start = Instant::now();
        let result = next(ctx).await;
        let duration = start.elapsed();
        
        match &result {
            Ok(resp) if resp.success => {
                log::log!(
                    self.level,
                    "Command {} completed successfully in {:?}",
                    ctx.command,
                    duration
                );
            }
            Ok(resp) => {
                log::warn!(
                    "Command {} failed: {:?} (duration: {:?})",
                    ctx.command,
                    resp.error,
                    duration
                );
            }
            Err(e) => {
                log::error!(
                    "Command {} error: {} (duration: {:?})",
                    ctx.command,
                    e,
                    duration
                );
            }
        }
        
        result
    }
    
    fn name(&self) -> &str {
        "logging"
    }
    
    fn priority(&self) -> i32 {
        10
    }
}

/// Metrics middleware for collecting command statistics
#[derive(Debug)]
pub struct MetricsMiddleware {
    metrics: Arc<RwLock<CommandMetrics>>,
}

#[derive(Debug, Default)]
pub struct CommandMetrics {
    pub total_commands: u64,
    pub successful_commands: u64,
    pub failed_commands: u64,
    pub total_duration: Duration,
    pub command_counts: HashMap<String, u64>,
}

impl MetricsMiddleware {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(CommandMetrics::default())),
        }
    }
    
    pub async fn get_metrics(&self) -> CommandMetrics {
        self.metrics.read().await.clone()
    }
}

impl Clone for CommandMetrics {
    fn clone(&self) -> Self {
        Self {
            total_commands: self.total_commands,
            successful_commands: self.successful_commands,
            failed_commands: self.failed_commands,
            total_duration: self.total_duration,
            command_counts: self.command_counts.clone(),
        }
    }
}

#[async_trait]
impl Middleware for MetricsMiddleware {
    async fn process(
        &self,
        ctx: &CommandContext,
        next: Next<'_>,
    ) -> Result<CommandResponse> {
        let start = Instant::now();
        let result = next(ctx).await;
        let duration = start.elapsed();
        
        let mut metrics = self.metrics.write().await;
        metrics.total_commands += 1;
        metrics.total_duration += duration;
        
        *metrics.command_counts.entry(ctx.command.clone()).or_insert(0) += 1;
        
        match &result {
            Ok(resp) if resp.success => {
                metrics.successful_commands += 1;
            }
            _ => {
                metrics.failed_commands += 1;
            }
        }
        
        result
    }
    
    fn name(&self) -> &str {
        "metrics"
    }
    
    fn priority(&self) -> i32 {
        20
    }
}

/// Validation middleware
#[derive(Debug)]
pub struct ValidationMiddleware {
    validators: Vec<Box<dyn CommandValidator>>,
}

/// Trait for command validators
#[async_trait]
pub trait CommandValidator: Send + Sync + Debug {
    async fn validate(&self, ctx: &CommandContext) -> Result<()>;
}

impl ValidationMiddleware {
    pub fn new() -> Self {
        Self {
            validators: Vec::new(),
        }
    }
    
    pub fn add_validator(&mut self, validator: Box<dyn CommandValidator>) {
        self.validators.push(validator);
    }
}

#[async_trait]
impl Middleware for ValidationMiddleware {
    async fn process(
        &self,
        ctx: &CommandContext,
        next: Next<'_>,
    ) -> Result<CommandResponse> {
        // Run all validators
        for validator in &self.validators {
            validator.validate(ctx).await?;
        }
        
        next(ctx).await
    }
    
    fn name(&self) -> &str {
        "validation"
    }
    
    fn priority(&self) -> i32 {
        30
    }
}

/// Authorization middleware
#[derive(Debug)]
pub struct AuthorizationMiddleware {
    authorized_commands: HashMap<String, Vec<String>>,
}

impl AuthorizationMiddleware {
    pub fn new() -> Self {
        Self {
            authorized_commands: HashMap::new(),
        }
    }
    
    pub fn authorize(&mut self, command: impl Into<String>, roles: Vec<String>) {
        self.authorized_commands.insert(command.into(), roles);
    }
    
    async fn check_authorization(&self, ctx: &CommandContext) -> Result<()> {
        if let Some(required_roles) = self.authorized_commands.get(&ctx.command) {
            // Check if user has required role (simplified for example)
            let user_role = ctx.metadata.get("user_role");
            
            if let Some(role) = user_role {
                if required_roles.contains(role) {
                    return Ok(());
                }
            }
            
            return Err(Error::Internal(format!(
                "Unauthorized: command '{}' requires roles {:?}",
                ctx.command, required_roles
            )));
        }
        
        Ok(())
    }
}

#[async_trait]
impl Middleware for AuthorizationMiddleware {
    async fn process(
        &self,
        ctx: &CommandContext,
        next: Next<'_>,
    ) -> Result<CommandResponse> {
        self.check_authorization(ctx).await?;
        next(ctx).await
    }
    
    fn name(&self) -> &str {
        "authorization"
    }
    
    fn priority(&self) -> i32 {
        40
    }
}

/// Retry middleware for handling transient failures
/// Note: This middleware can only retry if the operation itself is retryable
/// For true retry support, operations need to be designed to be idempotent
#[derive(Debug)]
pub struct RetryMiddleware {
    max_retries: u32,
    retry_delay: Duration,
}

impl RetryMiddleware {
    pub fn new(max_retries: u32, retry_delay: Duration) -> Self {
        Self {
            max_retries,
            retry_delay,
        }
    }
}

impl Default for RetryMiddleware {
    fn default() -> Self {
        Self::new(3, Duration::from_secs(1))
    }
}

#[async_trait]
impl Middleware for RetryMiddleware {
    async fn process(
        &self,
        ctx: &CommandContext,
        next: Next<'_>,
    ) -> Result<CommandResponse> {
        // Since we can't clone the next function, we can only execute it once
        // In a real implementation, you'd need to structure this differently
        // to support true retries (e.g., by having the handler be cloneable)
        
        let result = next(ctx).await;
        
        // Log if the operation failed (for demonstration)
        if let Ok(ref resp) = result {
            if !resp.success {
                log::warn!(
                    "Command {} failed (retry middleware active but can't retry FnOnce)",
                    ctx.command
                );
            }
        }
        
        result
    }
    
    fn name(&self) -> &str {
        "retry"
    }
    
    fn priority(&self) -> i32 {
        60
    }
}

/// Middleware pipeline for executing commands
/// Note: This is a simplified implementation that executes middleware in sequence
#[derive(Debug)]
pub struct MiddlewarePipeline {
    middlewares: Vec<Arc<dyn Middleware>>,
}

impl MiddlewarePipeline {
    /// Create a new middleware pipeline
    pub fn new() -> Self {
        Self {
            middlewares: Vec::new(),
        }
    }
    
    /// Add middleware to the pipeline
    pub fn use_middleware(&mut self, middleware: Arc<dyn Middleware>) {
        self.middlewares.push(middleware);
        // Sort by priority
        self.middlewares.sort_by_key(|m| m.priority());
    }
    
    /// Execute a command directly without middleware
    /// For the simplified implementation, middleware must be applied manually
    pub async fn execute_direct<F, Fut>(&self, ctx: &CommandContext, handler: F) -> Result<CommandResponse>
    where
        F: FnOnce(&CommandContext) -> Fut,
        Fut: std::future::Future<Output = Result<CommandResponse>> + Send,
    {
        handler(ctx).await
    }
    
    /// Apply a single middleware to a handler
    /// This is a simplified approach - full chaining would require a different design
    pub async fn apply_middleware<'a>(
        &'a self,
        ctx: &'a CommandContext,
        index: usize,
    ) -> Result<CommandResponse> {
        if index >= self.middlewares.len() {
            // Default response when no handler is provided
            return Ok(CommandResponse::success());
        }
        
        let middleware = &self.middlewares[index];
        
        // Create a simple next function that returns success
        let next: Next = Box::new(|_ctx| {
            Box::pin(async { Ok(CommandResponse::success()) })
        });
        
        middleware.process(ctx, next).await
    }
}

impl Default for MiddlewarePipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Global middleware pipeline instance
static MIDDLEWARE_PIPELINE: once_cell::sync::Lazy<Arc<RwLock<MiddlewarePipeline>>> = 
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(MiddlewarePipeline::new())));

/// Get the global middleware pipeline
pub async fn middleware_pipeline() -> Arc<RwLock<MiddlewarePipeline>> {
    MIDDLEWARE_PIPELINE.clone()
}

/// Register middleware globally
pub async fn register_middleware(middleware: Arc<dyn Middleware>) {
    let pipeline = middleware_pipeline().await;
    let mut pipeline = pipeline.write().await;
    pipeline.use_middleware(middleware);
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_logging_middleware() {
        let middleware = LoggingMiddleware::default();
        let ctx = CommandContext::new("test", vec!["arg1".to_string()]);
        
        let next: Next = Box::new(|_ctx| {
            Box::pin(async { Ok(CommandResponse::success()) })
        });
        
        let result = middleware.process(&ctx, next).await;
        assert!(result.is_ok());
        assert!(result.unwrap().success);
    }
    
    #[tokio::test]
    async fn test_metrics_middleware() {
        let middleware = MetricsMiddleware::new();
        let ctx = CommandContext::new("test", vec![]);
        
        // Execute command
        let next: Next = Box::new(|_ctx| {
            Box::pin(async { Ok(CommandResponse::success()) })
        });
        
        let _ = middleware.process(&ctx, next).await;
        
        // Check metrics
        let metrics = middleware.get_metrics().await;
        assert_eq!(metrics.total_commands, 1);
        assert_eq!(metrics.successful_commands, 1);
        assert_eq!(metrics.failed_commands, 0);
    }
    
    #[tokio::test]
    async fn test_command_context() {
        let ctx = CommandContext::new("test", vec!["arg".to_string()]);
        
        // Set and get data
        ctx.set_data("key", 42i32).await;
        let value: Option<i32> = ctx.get_data("key").await;
        assert_eq!(value, Some(42));
        
        // Check elapsed time
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(ctx.elapsed() >= Duration::from_millis(10));
    }
}
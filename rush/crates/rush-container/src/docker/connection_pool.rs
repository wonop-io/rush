//! Connection pooling for Docker operations
//!
//! This module provides connection pooling to improve Docker API performance
//! and handle connection limits gracefully.

use crate::docker::{DockerClient, ContainerStatus};
use async_trait::async_trait;
use rush_core::error::{Error, Result};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};
use log::{debug, info, warn};

/// Configuration for the connection pool
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Minimum number of connections to maintain
    pub min_connections: usize,
    /// Maximum number of connections
    pub max_connections: usize,
    /// Maximum time a connection can be idle before removal
    pub max_idle_time: Duration,
    /// Time to wait for a connection before timing out
    pub acquire_timeout: Duration,
    /// How often to check for idle connections
    pub cleanup_interval: Duration,
    /// Enable connection pooling (false uses direct connections)
    pub enabled: bool,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_connections: 2,
            max_connections: 10,
            max_idle_time: Duration::from_secs(300),
            acquire_timeout: Duration::from_secs(30),
            cleanup_interval: Duration::from_secs(60),
            enabled: true,
        }
    }
}

/// A pooled connection wrapper
#[derive(Debug)]
struct PooledConnection {
    /// The underlying Docker client
    client: Arc<dyn DockerClient>,
    /// When this connection was last used
    last_used: Instant,
    /// Unique ID for this connection
    id: usize,
    /// Whether this connection is currently in use
    in_use: bool,
}

impl PooledConnection {
    fn new(client: Arc<dyn DockerClient>, id: usize) -> Self {
        Self {
            client,
            last_used: Instant::now(),
            id,
            in_use: false,
        }
    }

    fn is_idle(&self, max_idle: Duration) -> bool {
        !self.in_use && self.last_used.elapsed() > max_idle
    }

    fn mark_used(&mut self) {
        self.in_use = true;
        self.last_used = Instant::now();
    }

    fn mark_returned(&mut self) {
        self.in_use = false;
        self.last_used = Instant::now();
    }
}

/// Connection pool for Docker clients
pub struct ConnectionPool {
    /// Pool configuration
    config: PoolConfig,
    /// Factory for creating new connections
    factory: Arc<dyn Fn() -> Arc<dyn DockerClient> + Send + Sync>,
    /// Available connections
    connections: Arc<Mutex<VecDeque<PooledConnection>>>,
    /// Semaphore to limit total connections
    semaphore: Arc<Semaphore>,
    /// Next connection ID
    next_id: Arc<Mutex<usize>>,
    /// Whether the pool is shutting down
    shutdown: Arc<Mutex<bool>>,
}

impl ConnectionPool {
    /// Create a new connection pool
    pub fn new<F>(config: PoolConfig, factory: F) -> Self
    where
        F: Fn() -> Arc<dyn DockerClient> + Send + Sync + 'static,
    {
        let pool = Self {
            config: config.clone(),
            factory: Arc::new(factory),
            connections: Arc::new(Mutex::new(VecDeque::new())),
            semaphore: Arc::new(Semaphore::new(config.max_connections)),
            next_id: Arc::new(Mutex::new(0)),
            shutdown: Arc::new(Mutex::new(false)),
        };

        // Start cleanup task if pooling is enabled
        if config.enabled {
            pool.start_cleanup_task();
        }

        pool
    }

    /// Initialize the pool with minimum connections
    pub async fn init(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        info!(
            "Initializing connection pool with {} minimum connections",
            self.config.min_connections
        );

        for _ in 0..self.config.min_connections {
            self.create_connection().await?;
        }

        Ok(())
    }

    /// Create a new connection and add it to the pool
    async fn create_connection(&self) -> Result<()> {
        let permit = self.semaphore.acquire().await
            .map_err(|e| Error::Docker(format!("Failed to acquire semaphore: {}", e)))?;
        
        let mut next_id = self.next_id.lock().await;
        let id = *next_id;
        *next_id += 1;
        drop(next_id);

        let client = (self.factory)();
        
        // Test the connection
        client.network_exists("bridge").await?;

        let connection = PooledConnection::new(client, id);
        
        let mut connections = self.connections.lock().await;
        connections.push_back(connection);
        
        // Forget the permit so it doesn't get dropped
        std::mem::forget(permit);
        
        debug!("Created new connection with ID {}", id);
        Ok(())
    }

    /// Acquire a connection from the pool
    pub async fn acquire(&self) -> Result<PoolGuard> {
        if !self.config.enabled {
            // Direct connection without pooling
            let client = (self.factory)();
            return Ok(PoolGuard::Direct(client));
        }

        let start = Instant::now();
        
        loop {
            // Check if we're shutting down
            if *self.shutdown.lock().await {
                return Err(Error::Docker("Connection pool is shutting down".into()));
            }

            // Try to get an existing connection
            {
                let mut connections = self.connections.lock().await;
                
                // Find an available connection
                for conn in connections.iter_mut() {
                    if !conn.in_use {
                        conn.mark_used();
                        debug!("Acquired existing connection {}", conn.id);
                        return Ok(PoolGuard::Pooled {
                            client: conn.client.clone(),
                            pool: self.clone_ref(),
                            id: conn.id,
                        });
                    }
                }
            }

            // Check if we can create a new connection
            if self.semaphore.available_permits() > 0 {
                self.create_connection().await?;
                // Loop again to acquire the newly created connection
                continue;
            }

            // Check timeout
            if start.elapsed() > self.config.acquire_timeout {
                return Err(Error::Docker(format!(
                    "Timeout acquiring connection after {:?}",
                    self.config.acquire_timeout
                )));
            }

            // Wait a bit before trying again
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Return a connection to the pool
    async fn return_connection(&self, id: usize) {
        let mut connections = self.connections.lock().await;
        
        for conn in connections.iter_mut() {
            if conn.id == id {
                conn.mark_returned();
                debug!("Returned connection {}", id);
                return;
            }
        }
        
        warn!("Attempted to return unknown connection {}", id);
    }

    /// Remove idle connections
    async fn cleanup_idle_connections(&self) {
        if *self.shutdown.lock().await {
            return;
        }

        let mut connections = self.connections.lock().await;
        let initial_count = connections.len();
        
        // Count how many can be removed
        let mut to_remove = Vec::new();
        let min_connections = self.config.min_connections;
        let max_idle_time = self.config.max_idle_time;
        
        for (i, conn) in connections.iter().enumerate() {
            if connections.len() - to_remove.len() <= min_connections {
                break;
            }
            if conn.is_idle(max_idle_time) {
                to_remove.push(i);
                debug!("Removing idle connection {}", conn.id);
            }
        }
        
        // Remove in reverse order to maintain indices
        for i in to_remove.iter().rev() {
            connections.remove(*i);
        }

        let removed = initial_count - connections.len();
        if removed > 0 {
            info!("Removed {} idle connections", removed);
            
            // Release semaphore permits for removed connections
            for _ in 0..removed {
                self.semaphore.add_permits(1);
            }
        }
    }

    /// Start the cleanup task
    fn start_cleanup_task(&self) {
        let pool = self.clone_ref();
        let interval = self.config.cleanup_interval;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            
            loop {
                interval.tick().await;
                
                if *pool.shutdown.lock().await {
                    break;
                }
                
                pool.cleanup_idle_connections().await;
            }
        });
    }

    /// Shutdown the pool
    pub async fn shutdown(&self) {
        info!("Shutting down connection pool");
        
        *self.shutdown.lock().await = true;
        
        // Clear all connections
        let mut connections = self.connections.lock().await;
        connections.clear();
    }

    /// Get pool statistics
    pub async fn stats(&self) -> PoolStats {
        let connections = self.connections.lock().await;
        
        let total = connections.len();
        let in_use = connections.iter().filter(|c| c.in_use).count();
        let idle = total - in_use;
        
        PoolStats {
            total_connections: total,
            in_use_connections: in_use,
            idle_connections: idle,
            max_connections: self.config.max_connections,
        }
    }

    /// Clone a reference to this pool
    fn clone_ref(&self) -> Arc<ConnectionPool> {
        // This is a placeholder - in real implementation, ConnectionPool would be wrapped in Arc
        unimplemented!("ConnectionPool should be wrapped in Arc for sharing")
    }
}

impl std::fmt::Debug for ConnectionPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionPool")
            .field("config", &self.config)
            .field("connections", &"<connections>")
            .field("semaphore", &self.semaphore)
            .field("next_id", &self.next_id)
            .field("shutdown", &self.shutdown)
            .finish()
    }
}

/// Statistics about the connection pool
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Total number of connections
    pub total_connections: usize,
    /// Number of connections currently in use
    pub in_use_connections: usize,
    /// Number of idle connections
    pub idle_connections: usize,
    /// Maximum connections allowed
    pub max_connections: usize,
}

/// Guard for a pooled connection
pub enum PoolGuard {
    /// A pooled connection that will be returned
    Pooled {
        client: Arc<dyn DockerClient>,
        pool: Arc<ConnectionPool>,
        id: usize,
    },
    /// A direct connection (when pooling is disabled)
    Direct(Arc<dyn DockerClient>),
}

impl PoolGuard {
    /// Get the underlying client
    pub fn client(&self) -> &Arc<dyn DockerClient> {
        match self {
            PoolGuard::Pooled { client, .. } => client,
            PoolGuard::Direct(client) => client,
        }
    }
}

impl Drop for PoolGuard {
    fn drop(&mut self) {
        if let PoolGuard::Pooled { pool, id, .. } = self {
            let pool = pool.clone();
            let id = *id;
            
            // Return connection asynchronously
            tokio::spawn(async move {
                pool.return_connection(id).await;
            });
        }
    }
}

/// Pooled Docker client that uses the connection pool
#[derive(Debug)]
pub struct PooledDockerClient {
    pool: Arc<ConnectionPool>,
}

impl PooledDockerClient {
    /// Create a new pooled Docker client
    pub fn new(pool: Arc<ConnectionPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DockerClient for PooledDockerClient {
    async fn build_image(
        &self,
        tag: &str,
        dockerfile: &str,
        context: &str,
    ) -> Result<()> {
        let guard = self.pool.acquire().await?;
        guard.client().build_image(tag, dockerfile, context).await
    }

    async fn create_network(&self, name: &str) -> Result<()> {
        let guard = self.pool.acquire().await?;
        guard.client().create_network(name).await
    }

    async fn network_exists(&self, name: &str) -> Result<bool> {
        let guard = self.pool.acquire().await?;
        guard.client().network_exists(name).await
    }

    async fn run_container(
        &self,
        image: &str,
        name: &str,
        network: &str,
        env_vars: &[String],
        ports: &[String],
        volumes: &[String],
    ) -> Result<String> {
        let guard = self.pool.acquire().await?;
        guard.client().run_container(
            image,
            name,
            network,
            env_vars,
            ports,
            volumes,
        ).await
    }

    async fn stop_container(&self, id: &str) -> Result<()> {
        let guard = self.pool.acquire().await?;
        guard.client().stop_container(id).await
    }

    async fn remove_container(&self, id: &str) -> Result<()> {
        let guard = self.pool.acquire().await?;
        guard.client().remove_container(id).await
    }

    async fn container_status(&self, id: &str) -> Result<ContainerStatus> {
        let guard = self.pool.acquire().await?;
        guard.client().container_status(id).await
    }

    async fn container_logs(&self, id: &str, lines: usize) -> Result<String> {
        let guard = self.pool.acquire().await?;
        guard.client().container_logs(id, lines).await
    }

    async fn container_exists(&self, name: &str) -> Result<bool> {
        let guard = self.pool.acquire().await?;
        guard.client().container_exists(name).await
    }

    async fn get_container_by_name(&self, name: &str) -> Result<String> {
        let guard = self.pool.acquire().await?;
        guard.client().get_container_by_name(name).await
    }

    async fn delete_network(&self, name: &str) -> Result<()> {
        let guard = self.pool.acquire().await?;
        guard.client().delete_network(name).await
    }

    async fn pull_image(&self, name: &str) -> Result<()> {
        let guard = self.pool.acquire().await?;
        guard.client().pull_image(name).await
    }

    async fn run_container_with_command(
        &self,
        image: &str,
        name: &str,
        network: &str,
        env_vars: &[String],
        ports: &[String],
        volumes: &[String],
        command: Option<&[String]>,
    ) -> Result<String> {
        let guard = self.pool.acquire().await?;
        guard.client().run_container_with_command(
            image,
            name,
            network,
            env_vars,
            ports,
            volumes,
            command,
        ).await
    }

    async fn follow_container_logs(
        &self,
        container_id: &str,
        label: String,
        color: &str,
    ) -> Result<()> {
        let guard = self.pool.acquire().await?;
        guard.client().follow_container_logs(container_id, label, color).await
    }

    async fn send_signal_to_container(&self, container_id: &str, signal: i32) -> Result<()> {
        let guard = self.pool.acquire().await?;
        guard.client().send_signal_to_container(container_id, signal).await
    }

    async fn exec_in_container(&self, container_id: &str, command: &[&str]) -> Result<String> {
        let guard = self.pool.acquire().await?;
        guard.client().exec_in_container(container_id, command).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pool_config_default() {
        let config = PoolConfig::default();
        assert_eq!(config.min_connections, 2);
        assert_eq!(config.max_connections, 10);
        assert!(config.enabled);
    }

    #[tokio::test]
    async fn test_pool_stats() {
        // This test would require a mock Docker client factory
        // Placeholder for now
    }
}
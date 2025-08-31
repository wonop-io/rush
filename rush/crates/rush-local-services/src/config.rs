use crate::types::{LocalServiceType, PortMapping, ResourceLimits, VolumeMapping};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for a local service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalServiceConfig {
    /// Service name
    pub name: String,

    /// Service type
    pub service_type: LocalServiceType,

    /// Docker image (overrides default)
    pub image: Option<String>,

    /// Port mappings
    pub ports: Vec<PortMapping>,

    /// Environment variables
    pub env: HashMap<String, String>,

    /// Volume mounts for persistence
    pub volumes: Vec<VolumeMapping>,

    /// Additional Docker run arguments
    pub docker_args: Vec<String>,

    /// Network mode
    pub network_mode: Option<String>,

    /// Resource limits
    pub resources: Option<ResourceLimits>,

    /// Whether to persist data between runs
    pub persist_data: bool,

    /// Health check command
    pub health_check: Option<String>,

    /// Initialization scripts or commands
    pub init_scripts: Vec<String>,
    
    /// Post-startup tasks to run after the service is healthy
    /// These are commands that will be executed inside the container
    pub post_startup_tasks: Vec<String>,

    /// Dependencies on other local services
    pub depends_on: Vec<String>,

    /// Custom command to run
    pub command: Option<String>,

    /// Container name override
    pub container_name: Option<String>,
}

impl LocalServiceConfig {
    /// Create a new LocalServiceConfig with defaults
    pub fn new(name: String, service_type: LocalServiceType) -> Self {
        Self {
            name,
            service_type,
            image: None,
            ports: Vec::new(),
            env: HashMap::new(),
            volumes: Vec::new(),
            docker_args: Vec::new(),
            network_mode: None,
            resources: None,
            persist_data: true,
            health_check: None,
            init_scripts: Vec::new(),
            post_startup_tasks: Vec::new(),
            depends_on: Vec::new(),
            command: None,
            container_name: None,
        }
    }

    /// Get the Docker image to use
    pub fn get_image(&self) -> String {
        self.image
            .clone()
            .unwrap_or_else(|| self.service_type.default_image())
    }

    /// Get the container name
    pub fn get_container_name(&self) -> String {
        self.container_name
            .clone()
            .unwrap_or_else(|| format!("rush-local-{}", self.name))
    }

    /// Get the health check command
    pub fn get_health_check(&self) -> Option<String> {
        self.health_check
            .clone()
            .or_else(|| self.service_type.default_health_check())
    }

    /// Apply service-specific defaults
    pub fn with_defaults(mut self) -> Self {
        // Apply default ports if none specified
        if self.ports.is_empty() {
            if let Some(default_port) = self.service_type.default_port() {
                self.ports
                    .push(PortMapping::new(default_port, default_port));
            }
        }

        // Apply service-specific environment variables
        match &self.service_type {
            LocalServiceType::PostgreSQL => {
                self.env
                    .entry("POSTGRES_DB".to_string())
                    .or_insert("postgres".to_string());
                self.env
                    .entry("POSTGRES_USER".to_string())
                    .or_insert("postgres".to_string());
                self.env
                    .entry("POSTGRES_PASSWORD".to_string())
                    .or_insert("postgres".to_string());
            }
            LocalServiceType::MySQL => {
                self.env
                    .entry("MYSQL_ROOT_PASSWORD".to_string())
                    .or_insert("root".to_string());
                self.env
                    .entry("MYSQL_DATABASE".to_string())
                    .or_insert("mysql".to_string());
            }
            LocalServiceType::MongoDB => {
                self.env
                    .entry("MONGO_INITDB_ROOT_USERNAME".to_string())
                    .or_insert("root".to_string());
                self.env
                    .entry("MONGO_INITDB_ROOT_PASSWORD".to_string())
                    .or_insert("root".to_string());
            }
            LocalServiceType::LocalStack => {
                self.env
                    .entry("SERVICES".to_string())
                    .or_insert("s3,sqs,sns,dynamodb".to_string());
                self.env
                    .entry("AWS_DEFAULT_REGION".to_string())
                    .or_insert("us-east-1".to_string());
                self.env
                    .entry("EDGE_PORT".to_string())
                    .or_insert("4566".to_string());
            }
            LocalServiceType::MinIO => {
                self.env
                    .entry("MINIO_ROOT_USER".to_string())
                    .or_insert("minioadmin".to_string());
                self.env
                    .entry("MINIO_ROOT_PASSWORD".to_string())
                    .or_insert("minioadmin".to_string());

                // Add MinIO console port if main port is configured
                if self.ports.iter().any(|p| p.container_port == 9000)
                    && !self.ports.iter().any(|p| p.container_port == 9001)
                {
                    self.ports.push(PortMapping::new(9001, 9001));
                }

                // Set default command for MinIO
                if self.command.is_none() {
                    self.command = Some("server /data --console-address \":9001\"".to_string());
                }
            }
            _ => {}
        }

        self
    }
}

/// Service-specific default configurations
pub struct ServiceDefaults;

impl ServiceDefaults {
    pub fn postgres(name: String) -> LocalServiceConfig {
        LocalServiceConfig::new(name, LocalServiceType::PostgreSQL).with_defaults()
    }

    pub fn redis(name: String) -> LocalServiceConfig {
        LocalServiceConfig::new(name, LocalServiceType::Redis).with_defaults()
    }

    pub fn minio(name: String) -> LocalServiceConfig {
        LocalServiceConfig::new(name, LocalServiceType::MinIO).with_defaults()
    }

    pub fn localstack(name: String) -> LocalServiceConfig {
        let mut config = LocalServiceConfig::new(name, LocalServiceType::LocalStack);
        // LocalStack needs Docker socket access
        config.volumes.push(VolumeMapping::new(
            "/var/run/docker.sock".to_string(),
            "/var/run/docker.sock".to_string(),
            false,
        ));
        // Add default post-startup task to create a local bucket
        // This can be overridden in the stack.spec.yaml
        config.post_startup_tasks.push(
            "awslocal s3 mb s3://local-bucket --region us-east-1 || true".to_string()
        );
        config.with_defaults()
    }

    pub fn stripe_cli(name: String, webhook_url: String) -> LocalServiceConfig {
        let mut config = LocalServiceConfig::new(name, LocalServiceType::StripeCLI);
        config.command = Some(format!("listen --forward-to {webhook_url}"));
        config.network_mode = Some("host".to_string());
        config
    }
}

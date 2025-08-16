use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents different types of local services
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LocalServiceType {
    // Databases
    PostgreSQL,
    MySQL,
    MongoDB,
    Redis,

    // AWS Services
    LocalStack, // Provides S3, SQS, SNS, DynamoDB, etc.
    MinIO,      // S3-compatible storage
    ElasticMQ,  // SQS alternative

    // Development Tools
    StripeCLI,
    MailHog, // Email testing

    // Custom
    Custom(String),
}

impl LocalServiceType {
    /// Get the default Docker image for this service type
    pub fn default_image(&self) -> String {
        match self {
            Self::PostgreSQL => "postgres:15-alpine".to_string(),
            Self::MySQL => "mysql:8".to_string(),
            Self::MongoDB => "mongo:6".to_string(),
            Self::Redis => "redis:7-alpine".to_string(),
            Self::LocalStack => "localstack/localstack:latest".to_string(),
            Self::MinIO => "minio/minio:latest".to_string(),
            Self::ElasticMQ => "softwaremill/elasticmq:latest".to_string(),
            Self::StripeCLI => "stripe/stripe-cli:latest".to_string(),
            Self::MailHog => "mailhog/mailhog:latest".to_string(),
            Self::Custom(image) => image.clone(),
        }
    }

    /// Get the default port for this service type
    pub fn default_port(&self) -> Option<u16> {
        match self {
            Self::PostgreSQL => Some(5432),
            Self::MySQL => Some(3306),
            Self::MongoDB => Some(27017),
            Self::Redis => Some(6379),
            Self::LocalStack => Some(4566),
            Self::MinIO => Some(9000),
            Self::ElasticMQ => Some(9324),
            Self::StripeCLI => None,
            Self::MailHog => Some(1025), // SMTP port
            Self::Custom(_) => None,
        }
    }

    /// Get the default health check command for this service type
    pub fn default_health_check(&self) -> Option<String> {
        match self {
            Self::PostgreSQL => Some("pg_isready -U postgres".to_string()),
            Self::MySQL => Some("mysqladmin ping -h localhost".to_string()),
            Self::MongoDB => Some("mongosh --eval 'db.adminCommand(\"ping\")'".to_string()),
            Self::Redis => Some("redis-cli ping".to_string()),
            Self::LocalStack => {
                Some("curl -f http://localhost:4566/_localstack/health".to_string())
            }
            Self::MinIO => Some("mc ready local".to_string()),
            Self::ElasticMQ => Some("curl -f http://localhost:9324/".to_string()),
            Self::StripeCLI => None,
            Self::MailHog => Some("curl -f http://localhost:8025/api/v2/messages".to_string()),
            Self::Custom(_) => None,
        }
    }

    /// Parse from string representation
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "postgresql" | "postgres" | "pg" => Self::PostgreSQL,
            "mysql" => Self::MySQL,
            "mongodb" | "mongo" => Self::MongoDB,
            "redis" => Self::Redis,
            "localstack" => Self::LocalStack,
            "minio" | "s3" => Self::MinIO,
            "elasticmq" | "sqs" => Self::ElasticMQ,
            "stripe" | "stripe-cli" => Self::StripeCLI,
            "mailhog" | "mail" => Self::MailHog,
            _ => Self::Custom(s.to_string()),
        }
    }
}

/// Port mapping configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host_port: u16,
    pub container_port: u16,
}

impl PortMapping {
    pub fn new(host_port: u16, container_port: u16) -> Self {
        Self {
            host_port,
            container_port,
        }
    }

    /// Parse from string format "host:container"
    pub fn from_str(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() == 2 {
            let host_port = parts[0].parse().ok()?;
            let container_port = parts[1].parse().ok()?;
            Some(Self::new(host_port, container_port))
        } else if parts.len() == 1 {
            let port = parts[0].parse().ok()?;
            Some(Self::new(port, port))
        } else {
            None
        }
    }

    pub fn to_docker_format(&self) -> String {
        format!("{}:{}", self.host_port, self.container_port)
    }
}

/// Volume mapping configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMapping {
    pub host_path: String,
    pub container_path: String,
    pub read_only: bool,
}

impl VolumeMapping {
    pub fn new(host_path: String, container_path: String, read_only: bool) -> Self {
        Self {
            host_path,
            container_path,
            read_only,
        }
    }

    /// Parse from string format "host:container" or "host:container:ro"
    pub fn from_str(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() >= 2 {
            let host_path = parts[0].to_string();
            let container_path = parts[1].to_string();
            let read_only = parts.get(2).map_or(false, |&p| p == "ro");
            Some(Self::new(host_path, container_path, read_only))
        } else {
            None
        }
    }

    pub fn to_docker_format(&self) -> String {
        if self.read_only {
            format!("{}:{}:ro", self.host_path, self.container_path)
        } else {
            format!("{}:{}", self.host_path, self.container_path)
        }
    }
}

/// Resource limits for a service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub memory: Option<String>, // e.g., "512m", "1g"
    pub cpus: Option<String>,   // e.g., "0.5", "2"
}

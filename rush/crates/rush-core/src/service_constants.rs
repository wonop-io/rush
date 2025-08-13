//! Service type constants used across Rush for LocalService configuration
//!
//! This module defines standard service type identifiers and their associated
//! configuration constants to ensure consistency across the codebase.

/// Standard service type identifiers
pub mod service_types {
    /// PostgreSQL database service
    pub const POSTGRESQL: &str = "postgresql";
    pub const POSTGRES: &str = "postgres"; // Alias
    
    /// Redis cache service
    pub const REDIS: &str = "redis";
    
    /// MongoDB database service
    pub const MONGODB: &str = "mongodb";
    
    /// MySQL database service
    pub const MYSQL: &str = "mysql";
    
    /// MinIO S3-compatible storage
    pub const MINIO: &str = "minio";
    
    /// LocalStack AWS services emulator
    pub const LOCALSTACK: &str = "localstack";
    
    /// Stripe CLI for webhook forwarding
    pub const STRIPE_CLI: &str = "stripe-cli";
    
    /// RabbitMQ message broker
    pub const RABBITMQ: &str = "rabbitmq";
    
    /// Apache Kafka distributed streaming
    pub const KAFKA: &str = "kafka";
    
    /// ElasticMQ SQS-compatible queue
    pub const ELASTICMQ: &str = "elasticmq";
    
    /// MailHog email testing service
    pub const MAILHOG: &str = "mailhog";
}

/// Environment variable names for service ports
pub mod port_env_vars {
    use super::service_types;
    
    /// Get the standard port environment variable name for a service type
    pub fn get_port_var(service_type: &str) -> Option<&'static str> {
        match service_type {
            service_types::POSTGRESQL | service_types::POSTGRES => Some("POSTGRES_PORT"),
            service_types::REDIS => Some("REDIS_PORT"),
            service_types::MONGODB => Some("MONGO_PORT"),
            service_types::MYSQL => Some("MYSQL_PORT"),
            service_types::MINIO => Some("MINIO_PORT"),
            service_types::RABBITMQ => Some("RABBITMQ_PORT"),
            service_types::KAFKA => Some("KAFKA_PORT"),
            _ => None,
        }
    }
    
    /// Get additional port environment variables (e.g., admin consoles)
    pub fn get_additional_port_vars(service_type: &str) -> Vec<&'static str> {
        match service_type {
            service_types::MINIO => vec!["MINIO_CONSOLE_PORT"],
            service_types::RABBITMQ => vec!["RABBITMQ_MANAGEMENT_PORT"],
            service_types::KAFKA => vec!["KAFKA_ZOOKEEPER_PORT"],
            _ => vec![],
        }
    }
}

/// Docker image mappings for services
pub mod docker_images {
    use super::service_types;
    
    /// Get the default Docker image for a service type
    pub fn get_default_image(service_type: &str, version: Option<&str>) -> Option<String> {
        let base_image = match service_type {
            service_types::POSTGRESQL | service_types::POSTGRES => "postgres",
            service_types::REDIS => "redis",
            service_types::MONGODB => "mongo",
            service_types::MYSQL => "mysql",
            service_types::MINIO => "minio/minio",
            service_types::LOCALSTACK => "localstack/localstack",
            service_types::RABBITMQ => "rabbitmq",
            service_types::KAFKA => "confluentinc/cp-kafka",
            service_types::ELASTICMQ => "softwaremill/elasticmq",
            service_types::MAILHOG => "mailhog/mailhog",
            _ => return None,
        };
        
        Some(match version {
            Some(v) => format!("{}:{}", base_image, v),
            None => format!("{}:latest", base_image),
        })
    }
}

/// Version validation utilities
pub mod version_validation {
    use regex::Regex;
    use once_cell::sync::Lazy;
    
    // Semantic version pattern (e.g., 1.2.3, 15, 7.2)
    static SEMVER_PATTERN: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"^(\d+)(?:\.(\d+))?(?:\.(\d+))?(?:-(.+))?$").unwrap()
    });
    
    // Docker tag pattern (alphanumeric with dots, dashes, underscores)
    static DOCKER_TAG_PATTERN: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9._-]{0,127}$").unwrap()
    });
    
    /// Validate a version string
    pub fn validate_version(version: &str) -> Result<(), String> {
        // Check for common version keywords
        if matches!(version, "latest" | "stable" | "edge" | "nightly") {
            return Ok(());
        }
        
        // Check semantic version
        if SEMVER_PATTERN.is_match(version) {
            return Ok(());
        }
        
        // Check Docker tag format
        if DOCKER_TAG_PATTERN.is_match(version) {
            return Ok(());
        }
        
        Err(format!(
            "Invalid version '{}'. Expected semantic version (e.g., '15', '7.2.1') or Docker tag",
            version
        ))
    }
}
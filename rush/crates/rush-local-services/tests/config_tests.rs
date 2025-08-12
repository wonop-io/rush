use rush_local_services::{LocalServiceConfig, LocalServiceType, PortMapping, VolumeMapping};
use std::collections::HashMap;

#[test]
fn test_create_postgresql_config() {
    let config = LocalServiceConfig::new(
        "test-postgres".to_string(),
        LocalServiceType::PostgreSQL,
    );
    
    assert_eq!(config.name, "test-postgres");
    assert_eq!(config.service_type, LocalServiceType::PostgreSQL);
    assert!(config.persist_data);
    assert!(config.image.is_none());
    assert!(config.ports.is_empty());
    assert!(config.env.is_empty());
}

#[test]
fn test_config_with_custom_settings() {
    let mut env = HashMap::new();
    env.insert("POSTGRES_USER".to_string(), "testuser".to_string());
    env.insert("POSTGRES_PASSWORD".to_string(), "testpass".to_string());
    
    let mut config = LocalServiceConfig::new(
        "postgres".to_string(),
        LocalServiceType::PostgreSQL,
    );
    
    config.ports = vec![PortMapping::new(5432, 5432)];
    config.env = env.clone();
    config.health_check = Some("pg_isready -U testuser".to_string());
    config.persist_data = true;
    
    assert_eq!(config.ports.len(), 1);
    assert_eq!(config.ports[0].host_port, 5432);
    assert_eq!(config.ports[0].container_port, 5432);
    assert_eq!(config.env.get("POSTGRES_USER"), Some(&"testuser".to_string()));
    assert_eq!(config.env.get("POSTGRES_PASSWORD"), Some(&"testpass".to_string()));
    assert!(config.health_check.is_some());
}

#[test]
fn test_get_image_with_default() {
    let mut config = LocalServiceConfig::new(
        "postgres".to_string(),
        LocalServiceType::PostgreSQL,
    );
    
    // Should use default image
    assert_eq!(config.get_image(), "postgres:15-alpine");
    
    // Override with custom image
    config.image = Some("postgres:16-alpine".to_string());
    assert_eq!(config.get_image(), "postgres:16-alpine");
}

#[test]
fn test_get_container_name() {
    let config = LocalServiceConfig::new(
        "my-postgres".to_string(),
        LocalServiceType::PostgreSQL,
    );
    
    // Default container name
    assert_eq!(config.get_container_name(), "rush-local-my-postgres");
    
    // Custom container name
    let mut config2 = config.clone();
    config2.container_name = Some("custom-postgres".to_string());
    assert_eq!(config2.get_container_name(), "custom-postgres");
}

#[test]
fn test_redis_config() {
    let config = LocalServiceConfig::new(
        "redis".to_string(),
        LocalServiceType::Redis,
    );
    
    assert_eq!(config.get_image(), "redis:7-alpine");
    assert_eq!(config.get_container_name(), "rush-local-redis");
}

#[test]
fn test_minio_config() {
    let mut config = LocalServiceConfig::new(
        "minio".to_string(),
        LocalServiceType::MinIO,
    );
    
    config.env.insert("MINIO_ROOT_USER".to_string(), "minioadmin".to_string());
    config.env.insert("MINIO_ROOT_PASSWORD".to_string(), "minioadmin".to_string());
    
    assert_eq!(config.get_image(), "minio/minio:latest");
    assert_eq!(config.env.get("MINIO_ROOT_USER"), Some(&"minioadmin".to_string()));
}

#[test]
fn test_custom_service_config() {
    let mut config = LocalServiceConfig::new(
        "custom".to_string(),
        LocalServiceType::Custom("my-service".to_string()),
    );
    
    config.image = Some("my-image:v1.0".to_string());
    config.command = Some("my-command --with-args".to_string());
    
    assert_eq!(config.get_image(), "my-image:v1.0");
    assert_eq!(config.command, Some("my-command --with-args".to_string()));
}

#[test]
fn test_service_dependencies() {
    let mut config = LocalServiceConfig::new(
        "app".to_string(),
        LocalServiceType::Custom("app".to_string()),
    );
    
    config.depends_on = vec!["postgres".to_string(), "redis".to_string()];
    
    assert_eq!(config.depends_on.len(), 2);
    assert!(config.depends_on.contains(&"postgres".to_string()));
    assert!(config.depends_on.contains(&"redis".to_string()));
}

#[test]
fn test_volume_mappings() {
    let mut config = LocalServiceConfig::new(
        "postgres".to_string(),
        LocalServiceType::PostgreSQL,
    );
    
    config.volumes = vec![
        VolumeMapping::new("/data".to_string(), "/var/lib/postgresql/data".to_string(), false),
        VolumeMapping::new("/config".to_string(), "/etc/postgresql".to_string(), true),
    ];
    
    assert_eq!(config.volumes.len(), 2);
    assert!(!config.volumes[0].read_only);
    assert!(config.volumes[1].read_only);
}
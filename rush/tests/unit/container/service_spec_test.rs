use rush_cli::container::{ServiceSpec, ServicesSpec};
use std::collections::HashMap;

#[test]
fn test_service_spec_creation() {
    let service = ServiceSpec {
        name: "api".to_string(),
        docker_host: "api-container".to_string(),
        host: "localhost".to_string(),
        port: 8080,
        target_port: 3000,
        mount_point: Some("/api".to_string()),
        domain: "example.com".to_string(),
    };
    
    assert_eq!(service.name, "api");
    assert_eq!(service.docker_host, "api-container");
    assert_eq!(service.host, "localhost");
    assert_eq!(service.port, 8080);
    assert_eq!(service.target_port, 3000);
    assert_eq!(service.mount_point, Some("/api".to_string()));
    assert_eq!(service.domain, "example.com");
}

#[test]
fn test_service_spec_with_no_mount_point() {
    let service = ServiceSpec {
        name: "db".to_string(),
        docker_host: "postgres".to_string(),
        host: "localhost".to_string(),
        port: 5432,
        target_port: 5432,
        mount_point: None,
        domain: "example.com".to_string(),
    };
    
    assert_eq!(service.name, "db");
    assert_eq!(service.docker_host, "postgres");
    assert_eq!(service.mount_point, None);
}

#[test]
fn test_services_spec_creation() {
    let mut services: ServicesSpec = HashMap::new();
    
    let service1 = ServiceSpec {
        name: "api".to_string(),
        docker_host: "api-container".to_string(),
        host: "localhost".to_string(),
        port: 8080,
        target_port: 3000,
        mount_point: Some("/api".to_string()),
        domain: "example.com".to_string(),
    };
    
    let service2 = ServiceSpec {
        name: "web".to_string(),
        docker_host: "web-container".to_string(),
        host: "localhost".to_string(),
        port: 8081,
        target_port: 8000,
        mount_point: Some("/".to_string()),
        domain: "example.com".to_string(),
    };
    
    // Add services to the component "my-app"
    services.insert("my-app".to_string(), vec![service1.clone(), service2.clone()]);
    
    // Verify services were added correctly
    assert_eq!(services.len(), 1);
    assert!(services.contains_key("my-app"));
    assert_eq!(services.get("my-app").unwrap().len(), 2);
    assert_eq!(services.get("my-app").unwrap()[0].name, "api");
    assert_eq!(services.get("my-app").unwrap()[1].name, "web");
}

#[test]
fn test_services_spec_multiple_components() {
    let mut services: ServicesSpec = HashMap::new();
    
    // Create services for two different components
    let api_service = ServiceSpec {
        name: "api".to_string(),
        docker_host: "api-container".to_string(),
        host: "localhost".to_string(),
        port: 8080,
        target_port: 3000,
        mount_point: Some("/api".to_string()),
        domain: "example.com".to_string(),
    };
    
    let db_service = ServiceSpec {
        name: "db".to_string(),
        docker_host: "postgres".to_string(),
        host: "localhost".to_string(),
        port: 5432,
        target_port: 5432,
        mount_point: None,
        domain: "example.com".to_string(),
    };
    
    // Add services to different components
    services.insert("backend".to_string(), vec![api_service.clone()]);
    services.insert("database".to_string(), vec![db_service.clone()]);
    
    // Verify services were added correctly
    assert_eq!(services.len(), 2);
    assert!(services.contains_key("backend"));
    assert!(services.contains_key("database"));
    
    assert_eq!(services.get("backend").unwrap().len(), 1);
    assert_eq!(services.get("backend").unwrap()[0].name, "api");
    
    assert_eq!(services.get("database").unwrap().len(), 1);
    assert_eq!(services.get("database").unwrap()[0].name, "db");
}

#[test]
fn test_services_spec_empty() {
    let services: ServicesSpec = HashMap::new();
    
    assert_eq!(services.len(), 0);
    assert!(services.is_empty());
}
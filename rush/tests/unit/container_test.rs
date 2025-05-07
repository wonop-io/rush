use crate::rush_cli::container::{ServiceSpec, ServicesSpec};
use std::collections::HashMap;

#[test]
fn test_service_spec_creation() {
    // Create a basic service spec
    let service_spec = ServiceSpec {
        name: "test-service".to_string(),
        docker_host: "unix:///var/run/docker.sock".to_string(),
        host: "localhost".to_string(),
        port: 8080,
        target_port: 80,
        mount_point: Some("/api".to_string()),
        domain: "example.com".to_string(),
    };
    
    // Verify fields
    assert_eq!(service_spec.name, "test-service");
    assert_eq!(service_spec.docker_host, "unix:///var/run/docker.sock");
    assert_eq!(service_spec.host, "localhost");
    assert_eq!(service_spec.port, 8080);
    assert_eq!(service_spec.target_port, 80);
    assert_eq!(service_spec.mount_point, Some("/api".to_string()));
    assert_eq!(service_spec.domain, "example.com");
}

#[test]
fn test_services_spec_serialization() {
    // Create a service spec
    let service_spec = ServiceSpec {
        name: "test-service".to_string(),
        docker_host: "unix:///var/run/docker.sock".to_string(),
        host: "localhost".to_string(),
        port: 8080,
        target_port: 80,
        mount_point: Some("/api".to_string()),
        domain: "example.com".to_string(),
    };
    
    // Create a services spec
    let mut services_spec = ServicesSpec::new();
    services_spec.insert("component1".to_string(), vec![service_spec.clone()]);
    
    // Add another component with multiple services
    let service_spec2 = ServiceSpec {
        name: "another-service".to_string(),
        docker_host: "tcp://remote-docker:2375".to_string(),
        host: "192.168.1.100".to_string(),
        port: 9090,
        target_port: 8080,
        mount_point: None,
        domain: "another-example.com".to_string(),
    };
    
    services_spec.insert("component2".to_string(), vec![service_spec.clone(), service_spec2]);
    
    // Verify structure
    assert_eq!(services_spec.len(), 2);
    assert_eq!(services_spec.get("component1").unwrap().len(), 1);
    assert_eq!(services_spec.get("component2").unwrap().len(), 2);
    
    // Serialization and deserialization test
    let serialized = serde_json::to_string(&services_spec).unwrap();
    let deserialized: ServicesSpec = serde_json::from_str(&serialized).unwrap();
    
    // Verify deserialized structure matches original
    assert_eq!(deserialized.len(), 2);
    assert_eq!(deserialized.get("component1").unwrap().len(), 1);
    assert_eq!(deserialized.get("component2").unwrap().len(), 2);
    
    // Check a specific field to ensure deserialization was correct
    assert_eq!(deserialized.get("component1").unwrap()[0].name, "test-service");
    assert_eq!(deserialized.get("component2").unwrap()[1].port, 9090);
}
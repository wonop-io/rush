use rush_local_services::{LocalServiceType, PortMapping, VolumeMapping};

#[test]
fn test_service_type_from_string() {
    assert_eq!(
        LocalServiceType::parse("postgresql"),
        LocalServiceType::PostgreSQL
    );
    assert_eq!(
        LocalServiceType::parse("postgres"),
        LocalServiceType::PostgreSQL
    );
    assert_eq!(LocalServiceType::parse("redis"), LocalServiceType::Redis);
    assert_eq!(LocalServiceType::parse("minio"), LocalServiceType::MinIO);
    assert_eq!(LocalServiceType::parse("s3"), LocalServiceType::MinIO);
    assert_eq!(
        LocalServiceType::parse("localstack"),
        LocalServiceType::LocalStack
    );
    assert_eq!(
        LocalServiceType::parse("stripe-cli"),
        LocalServiceType::StripeCLI
    );
    assert_eq!(
        LocalServiceType::parse("stripe"),
        LocalServiceType::StripeCLI
    );
    assert_eq!(
        LocalServiceType::parse("unknown"),
        LocalServiceType::Custom("unknown".to_string())
    );
}

#[test]
fn test_port_mapping_from_str() {
    // Standard port mapping
    let port = PortMapping::parse("8080:80").unwrap();
    assert_eq!(port.host_port, 8080);
    assert_eq!(port.container_port, 80);

    // Same port on both sides
    let port = PortMapping::parse("3000").unwrap();
    assert_eq!(port.host_port, 3000);
    assert_eq!(port.container_port, 3000);

    // Invalid format
    assert!(PortMapping::parse("invalid").is_none());
    assert!(PortMapping::parse("80:90:100").is_none());
    assert!(PortMapping::parse("not:numbers").is_none());
}

#[test]
fn test_port_mapping_to_docker_format() {
    let port = PortMapping::new(8080, 80);
    assert_eq!(port.to_docker_format(), "8080:80");

    let port = PortMapping::new(3000, 3000);
    assert_eq!(port.to_docker_format(), "3000:3000");
}

#[test]
fn test_volume_mapping_from_str() {
    // Read-write volume
    let vol = VolumeMapping::parse("/host/path:/container/path").unwrap();
    assert_eq!(vol.host_path, "/host/path");
    assert_eq!(vol.container_path, "/container/path");
    assert!(!vol.read_only);

    // Read-only volume
    let vol = VolumeMapping::parse("/host/path:/container/path:ro").unwrap();
    assert_eq!(vol.host_path, "/host/path");
    assert_eq!(vol.container_path, "/container/path");
    assert!(vol.read_only);

    // Invalid format
    assert!(VolumeMapping::parse("/only/one/path").is_none());
    assert!(VolumeMapping::parse("").is_none());
}

#[test]
fn test_volume_mapping_to_docker_format() {
    let vol = VolumeMapping::new("/host".to_string(), "/container".to_string(), false);
    assert_eq!(vol.to_docker_format(), "/host:/container");

    let vol = VolumeMapping::new("/host".to_string(), "/container".to_string(), true);
    assert_eq!(vol.to_docker_format(), "/host:/container:ro");
}

#[test]
fn test_service_type_equality() {
    assert_eq!(LocalServiceType::PostgreSQL, LocalServiceType::PostgreSQL);
    assert_ne!(LocalServiceType::PostgreSQL, LocalServiceType::Redis);
    assert_eq!(
        LocalServiceType::Custom("test".to_string()),
        LocalServiceType::Custom("test".to_string())
    );
    assert_ne!(
        LocalServiceType::Custom("test1".to_string()),
        LocalServiceType::Custom("test2".to_string())
    );
}

#[test]
fn test_service_type_clone() {
    let original = LocalServiceType::Custom("my-service".to_string());
    let cloned = original.clone();
    assert_eq!(original, cloned);
}

#[test]
fn test_port_mapping_edge_cases() {
    // Maximum port number
    let port = PortMapping::new(65535, 65535);
    assert_eq!(port.to_docker_format(), "65535:65535");

    // Minimum port number
    let port = PortMapping::new(1, 1);
    assert_eq!(port.to_docker_format(), "1:1");

    // Different ports
    let port = PortMapping::new(8080, 3000);
    assert_eq!(port.host_port, 8080);
    assert_eq!(port.container_port, 3000);
}

#[test]
fn test_volume_mapping_with_spaces() {
    let vol = VolumeMapping::parse("/path with spaces:/container path").unwrap();
    assert_eq!(vol.host_path, "/path with spaces");
    assert_eq!(vol.container_path, "/container path");
    assert_eq!(vol.to_docker_format(), "/path with spaces:/container path");
}

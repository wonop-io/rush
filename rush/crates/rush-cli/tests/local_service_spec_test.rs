use std::fs;
use std::path::Path;
use serde_yaml;

#[test]
fn test_local_service_example_spec_file_structure() {
    // Test that the example spec file has the correct flat structure
    let spec_path = Path::new("../../examples/local-services-test/stack.spec.yaml");
    
    // Skip test if file doesn't exist (e.g., in CI without full repo)
    if !spec_path.exists() {
        eprintln!("Skipping test: example spec file not found at {:?}", spec_path);
        return;
    }

    let spec_content = fs::read_to_string(spec_path)
        .expect("Failed to read example spec file");

    let spec_value: serde_yaml::Value = serde_yaml::from_str(&spec_content)
        .expect("Failed to parse example spec file as YAML");

    let spec_map = spec_value.as_mapping()
        .expect("Spec file should be a YAML mapping");

    // Count LocalService components
    let mut local_service_count = 0;
    let mut local_service_components = Vec::new();

    for (name, component) in spec_map {
        let name_str = name.as_str().unwrap_or("unknown");
        
        if let Some(build_type_value) = component.get("build_type") {
            // Verify build_type is a flat string, not nested
            assert!(
                build_type_value.is_string(),
                "Component '{}' has build_type that is not a flat string: {:?}",
                name_str,
                build_type_value
            );

            let build_type = build_type_value.as_str().unwrap();
            
            if build_type == "LocalService" {
                local_service_count += 1;
                local_service_components.push(name_str.to_string());

                // Verify required LocalService fields
                assert!(
                    component.get("service_type").is_some(),
                    "LocalService component '{}' missing required 'service_type' field",
                    name_str
                );

                assert!(
                    component.get("persist_data").is_some(),
                    "LocalService component '{}' missing required 'persist_data' field",
                    name_str
                );

                // Verify no Docker-specific fields that shouldn't be there
                assert!(
                    component.get("image").is_none(),
                    "LocalService component '{}' should not have 'image' field",
                    name_str
                );

                assert!(
                    component.get("ports").is_none(),
                    "LocalService component '{}' should not have 'ports' field (use env vars instead)",
                    name_str
                );

                assert!(
                    component.get("volumes").is_none(),
                    "LocalService component '{}' should not have 'volumes' field",
                    name_str
                );

                assert!(
                    component.get("docker_args").is_none(),
                    "LocalService component '{}' should not have 'docker_args' field",
                    name_str
                );

                // Check for version field (optional but recommended)
                if let Some(version) = component.get("version") {
                    assert!(
                        version.is_string(),
                        "LocalService component '{}' version should be a string",
                        name_str
                    );
                }

                // Check environment variables for port configuration
                if let Some(env) = component.get("env") {
                    let env_map = env.as_mapping()
                        .expect(&format!("Component '{}' env should be a mapping", name_str));
                    
                    // Check for port configuration in env vars based on service type
                    let service_type = component.get("service_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    
                    match service_type {
                        "postgresql" => {
                            assert!(
                                env_map.get(&serde_yaml::Value::String("POSTGRES_PORT".to_string())).is_some(),
                                "PostgreSQL service '{}' should configure port via POSTGRES_PORT env var",
                                name_str
                            );
                        }
                        "redis" => {
                            assert!(
                                env_map.get(&serde_yaml::Value::String("REDIS_PORT".to_string())).is_some(),
                                "Redis service '{}' should configure port via REDIS_PORT env var",
                                name_str
                            );
                        }
                        "minio" => {
                            assert!(
                                env_map.get(&serde_yaml::Value::String("MINIO_PORT".to_string())).is_some(),
                                "MinIO service '{}' should configure port via MINIO_PORT env var",
                                name_str
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Verify we found the expected LocalService components
    assert!(
        local_service_count > 0,
        "Expected to find LocalService components in example spec file"
    );

    println!("✅ Found {} LocalService components with correct flat structure: {:?}",
             local_service_count, local_service_components);
}

#[test]
fn test_spec_file_consistency() {
    // Test that all spec files use consistent flat structure
    let spec_files = vec![
        "../../examples/local-services-test/stack.spec.yaml",
        "../../products/io.wonop.helloworld/stack.spec.yaml",
    ];

    for spec_path_str in spec_files {
        let spec_path = Path::new(spec_path_str);
        
        if !spec_path.exists() {
            eprintln!("Skipping {}: file not found", spec_path_str);
            continue;
        }

        let spec_content = fs::read_to_string(spec_path)
            .expect(&format!("Failed to read spec file: {}", spec_path_str));

        let spec_value: serde_yaml::Value = serde_yaml::from_str(&spec_content)
            .expect(&format!("Failed to parse spec file as YAML: {}", spec_path_str));

        let spec_map = spec_value.as_mapping()
            .expect(&format!("Spec file should be a YAML mapping: {}", spec_path_str));

        for (name, component) in spec_map {
            let name_str = name.as_str().unwrap_or("unknown");
            
            if let Some(build_type_value) = component.get("build_type") {
                // All build_type values should be flat strings
                assert!(
                    build_type_value.is_string(),
                    "In {}: Component '{}' has build_type that is not a flat string",
                    spec_path_str,
                    name_str
                );
                
                // build_type should not be a nested mapping
                assert!(
                    build_type_value.as_mapping().is_none(),
                    "In {}: Component '{}' has nested build_type structure, should be flat",
                    spec_path_str,
                    name_str
                );
            }
        }
        
        println!("✅ {} has consistent flat structure", spec_path_str);
    }
}
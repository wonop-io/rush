use rush_cli::builder::BuildType;
use serde_json::{self, json};

#[test]
fn test_build_type_trunk_wasm_serialization() {
    let build_type = BuildType::TrunkWasm {
        location: "/path/to/wasm".to_string(),
        dockerfile_path: "/path/to/Dockerfile".to_string(),
        context_dir: Some("/path/to/context".to_string()),
        ssr: true,
        features: Some(vec!["feature1".to_string(), "feature2".to_string()]),
        precompile_commands: Some(vec!["npm install".to_string()]),
    };
    
    let serialized = serde_json::to_string(&build_type).expect("Failed to serialize TrunkWasm");
    let deserialized: BuildType = serde_json::from_str(&serialized).expect("Failed to deserialize TrunkWasm");
    
    match deserialized {
        BuildType::TrunkWasm { location, dockerfile_path, context_dir, ssr, features, precompile_commands } => {
            assert_eq!(location, "/path/to/wasm");
            assert_eq!(dockerfile_path, "/path/to/Dockerfile");
            assert_eq!(context_dir, Some("/path/to/context".to_string()));
            assert_eq!(ssr, true);
            assert_eq!(features, Some(vec!["feature1".to_string(), "feature2".to_string()]));
            assert_eq!(precompile_commands, Some(vec!["npm install".to_string()]));
        },
        _ => panic!("Deserialized to wrong variant"),
    }
}

#[test]
fn test_build_type_rust_binary_serialization() {
    let build_type = BuildType::RustBinary {
        location: "/path/to/rust".to_string(),
        dockerfile_path: "/path/to/Dockerfile".to_string(),
        context_dir: Some("/path/to/context".to_string()),
        features: Some(vec!["feature1".to_string()]),
        precompile_commands: Some(vec!["cargo update".to_string()]),
    };
    
    let serialized = serde_json::to_string(&build_type).expect("Failed to serialize RustBinary");
    let deserialized: BuildType = serde_json::from_str(&serialized).expect("Failed to deserialize RustBinary");
    
    match deserialized {
        BuildType::RustBinary { location, dockerfile_path, context_dir, features, precompile_commands } => {
            assert_eq!(location, "/path/to/rust");
            assert_eq!(dockerfile_path, "/path/to/Dockerfile");
            assert_eq!(context_dir, Some("/path/to/context".to_string()));
            assert_eq!(features, Some(vec!["feature1".to_string()]));
            assert_eq!(precompile_commands, Some(vec!["cargo update".to_string()]));
        },
        _ => panic!("Deserialized to wrong variant"),
    }
}

#[test]
fn test_build_type_dixious_wasm_serialization() {
    let build_type = BuildType::DixiousWasm {
        location: "/path/to/dixious".to_string(),
        dockerfile_path: "/path/to/Dockerfile".to_string(),
        context_dir: Some("/path/to/context".to_string()),
    };
    
    let serialized = serde_json::to_string(&build_type).expect("Failed to serialize DixiousWasm");
    let deserialized: BuildType = serde_json::from_str(&serialized).expect("Failed to deserialize DixiousWasm");
    
    match deserialized {
        BuildType::DixiousWasm { location, dockerfile_path, context_dir } => {
            assert_eq!(location, "/path/to/dixious");
            assert_eq!(dockerfile_path, "/path/to/Dockerfile");
            assert_eq!(context_dir, Some("/path/to/context".to_string()));
        },
        _ => panic!("Deserialized to wrong variant"),
    }
}

#[test]
fn test_build_type_script_serialization() {
    let build_type = BuildType::Script {
        location: "/path/to/script".to_string(),
        dockerfile_path: "/path/to/Dockerfile".to_string(),
        context_dir: None,
    };
    
    let serialized = serde_json::to_string(&build_type).expect("Failed to serialize Script");
    let deserialized: BuildType = serde_json::from_str(&serialized).expect("Failed to deserialize Script");
    
    match deserialized {
        BuildType::Script { location, dockerfile_path, context_dir } => {
            assert_eq!(location, "/path/to/script");
            assert_eq!(dockerfile_path, "/path/to/Dockerfile");
            assert_eq!(context_dir, None);
        },
        _ => panic!("Deserialized to wrong variant"),
    }
}

#[test]
fn test_build_type_ingress_serialization() {
    let build_type = BuildType::Ingress {
        components: vec!["comp1".to_string(), "comp2".to_string()],
        dockerfile_path: "/path/to/Dockerfile".to_string(),
        context_dir: None,
    };
    
    let serialized = serde_json::to_string(&build_type).expect("Failed to serialize Ingress");
    let deserialized: BuildType = serde_json::from_str(&serialized).expect("Failed to deserialize Ingress");
    
    match deserialized {
        BuildType::Ingress { components, dockerfile_path, context_dir } => {
            assert_eq!(components, vec!["comp1".to_string(), "comp2".to_string()]);
            assert_eq!(dockerfile_path, "/path/to/Dockerfile");
            assert_eq!(context_dir, None);
        },
        _ => panic!("Deserialized to wrong variant"),
    }
}

#[test]
fn test_build_type_pure_docker_image_serialization() {
    let build_type = BuildType::PureDockerImage {
        image_name_with_tag: "nginx:latest".to_string(),
        command: Some("nginx -g 'daemon off;'".to_string()),
        entrypoint: Some("/docker-entrypoint.sh".to_string()),
    };
    
    let serialized = serde_json::to_string(&build_type).expect("Failed to serialize PureDockerImage");
    let deserialized: BuildType = serde_json::from_str(&serialized).expect("Failed to deserialize PureDockerImage");
    
    match deserialized {
        BuildType::PureDockerImage { image_name_with_tag, command, entrypoint } => {
            assert_eq!(image_name_with_tag, "nginx:latest");
            assert_eq!(command, Some("nginx -g 'daemon off;'".to_string()));
            assert_eq!(entrypoint, Some("/docker-entrypoint.sh".to_string()));
        },
        _ => panic!("Deserialized to wrong variant"),
    }
}

#[test]
fn test_build_type_pure_kubernetes_serialization() {
    let build_type = BuildType::PureKubernetes;
    
    let serialized = serde_json::to_string(&build_type).expect("Failed to serialize PureKubernetes");
    let deserialized: BuildType = serde_json::from_str(&serialized).expect("Failed to deserialize PureKubernetes");
    
    match deserialized {
        BuildType::PureKubernetes => {},
        _ => panic!("Deserialized to wrong variant"),
    }
}

#[test]
fn test_build_type_kubernetes_installation_serialization() {
    let build_type = BuildType::KubernetesInstallation {
        namespace: "test-namespace".to_string(),
    };
    
    let serialized = serde_json::to_string(&build_type).expect("Failed to serialize KubernetesInstallation");
    let deserialized: BuildType = serde_json::from_str(&serialized).expect("Failed to deserialize KubernetesInstallation");
    
    match deserialized {
        BuildType::KubernetesInstallation { namespace } => {
            assert_eq!(namespace, "test-namespace");
        },
        _ => panic!("Deserialized to wrong variant"),
    }
}
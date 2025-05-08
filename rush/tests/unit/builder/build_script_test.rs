use rush_cli::builder::{BuildContext, BuildScript, BuildType};
use rush_cli::container::ServicesSpec;
use rush_cli::toolchain::{Platform, ToolchainContext};
use std::collections::HashMap;

#[test]
fn test_build_script_trunk_wasm() {
    let build_type = BuildType::TrunkWasm {
        location: "/path/to/wasm".to_string(),
        dockerfile_path: "/path/to/Dockerfile".to_string(),
        context_dir: None,
        ssr: false,
        features: None,
        precompile_commands: None,
    };
    
    let build_script = BuildScript::new(build_type);
    let context = create_test_build_context();
    
    let rendered = build_script.render(&context);
    
    // Just check that the rendering produces a non-empty string
    // The exact content would depend on the templates which we don't have direct access to
    assert!(!rendered.is_empty());
    
    // Check for some expected content based on the template name
    assert!(rendered.contains("#!/bin/bash") || rendered.contains("#!/usr/bin/env bash"));
}

#[test]
fn test_build_script_rust_binary() {
    let build_type = BuildType::RustBinary {
        location: "/path/to/rust".to_string(),
        dockerfile_path: "/path/to/Dockerfile".to_string(),
        context_dir: None,
        features: None,
        precompile_commands: None,
    };
    
    let build_script = BuildScript::new(build_type);
    let context = create_test_build_context();
    
    let rendered = build_script.render(&context);
    
    // Just check that the rendering produces a non-empty string
    assert!(!rendered.is_empty());
}

#[test]
fn test_build_script_dixious_wasm() {
    let build_type = BuildType::DixiousWasm {
        location: "/path/to/dixious".to_string(),
        dockerfile_path: "/path/to/Dockerfile".to_string(),
        context_dir: None,
    };
    
    let build_script = BuildScript::new(build_type);
    let context = create_test_build_context();
    
    let rendered = build_script.render(&context);
    
    // Just check that the rendering produces a non-empty string
    assert!(!rendered.is_empty());
}

#[test]
fn test_build_script_zola() {
    let build_type = BuildType::Zola {
        location: "/path/to/zola".to_string(),
        dockerfile_path: "/path/to/Dockerfile".to_string(),
        context_dir: None,
    };
    
    let build_script = BuildScript::new(build_type);
    let context = create_test_build_context();
    
    let rendered = build_script.render(&context);
    
    // Just check that the rendering produces a non-empty string
    assert!(!rendered.is_empty());
}

#[test]
fn test_build_script_book() {
    let build_type = BuildType::Book {
        location: "/path/to/book".to_string(),
        dockerfile_path: "/path/to/Dockerfile".to_string(),
        context_dir: None,
    };
    
    let build_script = BuildScript::new(build_type);
    let context = create_test_build_context();
    
    let rendered = build_script.render(&context);
    
    // Just check that the rendering produces a non-empty string
    assert!(!rendered.is_empty());
}

#[test]
fn test_build_script_script() {
    let build_type = BuildType::Script {
        location: "/path/to/script".to_string(),
        dockerfile_path: "/path/to/Dockerfile".to_string(),
        context_dir: None,
    };
    
    let build_script = BuildScript::new(build_type);
    let context = create_test_build_context();
    
    let rendered = build_script.render(&context);
    
    // For Script type, an empty string is expected
    assert_eq!(rendered, "");
}

#[test]
fn test_build_script_pure_kubernetes() {
    let build_type = BuildType::PureKubernetes;
    
    let build_script = BuildScript::new(build_type);
    let context = create_test_build_context();
    
    let rendered = build_script.render(&context);
    
    // For PureKubernetes type, an empty string is expected
    assert_eq!(rendered, "");
}

#[test]
fn test_build_script_kubernetes_installation() {
    let build_type = BuildType::KubernetesInstallation {
        namespace: "test-namespace".to_string(),
    };
    
    let build_script = BuildScript::new(build_type);
    let context = create_test_build_context();
    
    let rendered = build_script.render(&context);
    
    // For KubernetesInstallation type, an empty string is expected
    assert_eq!(rendered, "");
}

#[test]
fn test_build_script_ingress() {
    let build_type = BuildType::Ingress {
        components: vec!["comp1".to_string(), "comp2".to_string()],
        dockerfile_path: "/path/to/Dockerfile".to_string(),
        context_dir: None,
    };
    
    let build_script = BuildScript::new(build_type);
    let context = create_test_build_context();
    
    let rendered = build_script.render(&context);
    
    // For Ingress type, an empty string is expected
    assert_eq!(rendered, "");
}

#[test]
fn test_build_script_pure_docker_image() {
    let build_type = BuildType::PureDockerImage {
        image_name_with_tag: "nginx:latest".to_string(),
        command: None,
        entrypoint: None,
    };
    
    let build_script = BuildScript::new(build_type);
    let context = create_test_build_context();
    
    let rendered = build_script.render(&context);
    
    // For PureDockerImage type, an empty string is expected
    assert_eq!(rendered, "");
}

// Helper function to create a test BuildContext
fn create_test_build_context() -> BuildContext {
    BuildContext {
        build_type: BuildType::RustBinary {
            location: "/path/to/rust".to_string(),
            dockerfile_path: "/path/to/Dockerfile".to_string(),
            context_dir: None,
            features: None,
            precompile_commands: None,
        },
        location: Some("/path/to/source".to_string()),
        target: Platform { os: rush_cli::toolchain::platform::OperatingSystem::Linux, arch: rush_cli::toolchain::platform::ArchType::X86_64 },
        host: Platform { os: rush_cli::toolchain::platform::OperatingSystem::Linux, arch: rush_cli::toolchain::platform::ArchType::X86_64 },
        rust_target: "x86_64-unknown-linux-gnu".to_string(),
        toolchain: ToolchainContext::default(),
        services: ServicesSpec::default(),
        environment: "dev".to_string(),
        domain: "test-domain.com".to_string(),
        product_name: "test-product".to_string(),
        product_uri: "test-uri".to_string(),
        component: "test-component".to_string(),
        docker_registry: "test-registry.com".to_string(),
        image_name: "test-image".to_string(),
        domains: HashMap::new(),
        env: HashMap::new(),
        secrets: HashMap::new(),
    }
}
//! Early startup of local services before context creation
//!
//! This module handles starting local services before the main context is created,
//! so that their environment variables (like database connection strings) can be
//! included in the generated .env files.

use log::info;
use rush_build::{BuildType, ComponentBuildSpec, Variables};
use rush_config::Config;
use rush_container::DockerCliClient;
use rush_docker::DockerClient;
use rush_core::error::{Error, Result};
use rush_local_services::{
    DockerLocalService, LocalServiceConfig, LocalServiceManager,
    LocalServiceType, ProcessLocalService,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Starts local services and returns their environment variables and the manager
pub async fn start_local_services(
    config: &Config,
    product_name: &str,
    docker_command: &str,
) -> Result<(HashMap<String, String>, LocalServiceManager)> {
    info!("Starting local services for {}", product_name);
    
    // Parse stack.spec.yaml to find local services
    let stack_spec_path = config.product_path().join("stack.spec.yaml");
    if !stack_spec_path.exists() {
        info!("No stack.spec.yaml found, skipping local services");
        let manager = LocalServiceManager::new();
        return Ok((HashMap::new(), manager));
    }
    
    // Load and parse the stack spec
    let spec_content = std::fs::read_to_string(&stack_spec_path)
        .map_err(|e| Error::Setup(format!("Failed to read stack spec: {}", e)))?;
    let yaml: serde_yaml::Value = serde_yaml::from_str(&spec_content)
        .map_err(|e| Error::Setup(format!("Failed to parse stack spec: {}", e)))?;
    
    // Create docker client
    let docker_client = Arc::new(DockerCliClient::new(docker_command.to_string()));
    
    // Create local service manager
    let mut manager = LocalServiceManager::new();
    
    // Create data directory for local services
    let data_dir = config.product_path().join("target").join("local-services");
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| Error::Setup(format!("Failed to create data directory: {}", e)))?;
    
    // Parse component specs and register local services
    let variables = Variables::empty();
    let mut has_local_services = false;
    
    if let Some(components) = yaml.as_mapping() {
        for (name, spec_yaml) in components {
            if let (Some(name_str), Some(spec_obj)) = (name.as_str(), spec_yaml.as_mapping()) {
                // Check if this is a LocalService
                if let Some(build_type) = spec_obj.get(&serde_yaml::Value::String("build_type".to_string())) {
                    if let Some(build_type_str) = build_type.as_str() {
                        if build_type_str == "LocalService" {
                            has_local_services = true;
                            
                            // Clone the spec and add component_name field
                            let mut spec_yaml_clone = spec_yaml.clone();
                            if let serde_yaml::Value::Mapping(ref mut spec_map) = spec_yaml_clone {
                                if !spec_map.contains_key(&serde_yaml::Value::String("component_name".to_string())) {
                                    spec_map.insert(
                                        serde_yaml::Value::String("component_name".to_string()),
                                        serde_yaml::Value::String(name_str.to_string()),
                                    );
                                }
                            }
                            
                            // Parse the component spec
                            let spec = ComponentBuildSpec::from_yaml(
                                Arc::new(config.clone()),
                                variables.clone(),
                                &spec_yaml_clone,
                            );
                            
                            // Register the local service
                            register_local_service(
                                &mut manager,
                                &spec,
                                docker_client.clone(),
                                config.network_name().to_string(),
                                data_dir.clone(),
                            )?;
                        }
                    }
                }
            }
        }
    }
    
    if !has_local_services {
        info!("No local services found in stack.spec.yaml");
        return Ok((HashMap::new(), manager));
    }
    
    // Start all local services
    info!("Starting all local services...");
    manager.start_all().await
        .map_err(|e| Error::Docker(format!("Failed to start local services: {}", e)))?;
    
    // Wait for services to be healthy
    info!("Waiting for local services to be healthy...");
    manager.wait_for_healthy(Duration::from_secs(60)).await
        .map_err(|e| Error::Docker(format!("Services failed health check: {}", e)))?;
    
    // Collect environment variables and secrets
    let mut all_env_vars = HashMap::new();
    
    // Add regular environment variables
    for (key, value) in manager.get_env_vars() {
        info!("Local service env var: {}=...", key);
        all_env_vars.insert(key.clone(), value.clone());
    }
    
    // Add secrets
    for (key, value) in manager.get_env_secrets() {
        info!("Local service secret: {}=...", key);
        all_env_vars.insert(key.clone(), value.clone());
    }
    
    info!("Local services started successfully with {} environment variables", all_env_vars.len());
    Ok((all_env_vars, manager))
}

/// Register a local service based on its component spec
fn register_local_service(
    manager: &mut LocalServiceManager,
    spec: &ComponentBuildSpec,
    docker_client: Arc<dyn DockerClient>,
    network_name: String,
    data_dir: PathBuf,
) -> Result<()> {
    if let BuildType::LocalService {
        service_type,
        persist_data,
        env,
        health_check,
        init_scripts,
        post_startup_tasks,
        command,
        ..
    } = &spec.build_type
    {
        info!("Registering local service: {} ({})", spec.component_name, service_type);
        
        // Handle Stripe CLI specially as a process-based service
        if service_type == "stripe-cli" || service_type == "stripe" {
            register_stripe_service(manager, spec, command.clone())?;
            return Ok(());
        }
        
        // Parse service type
        let service_type_enum = parse_service_type(service_type);
        
        // Create default volumes for services that need persistence
        let volumes = if *persist_data {
            create_default_volumes(&service_type_enum, &data_dir, &spec.component_name)
        } else {
            vec![]
        };
        
        // Create configuration for Docker-based service
        let config = LocalServiceConfig {
            name: spec.component_name.clone(),
            service_type: service_type_enum.clone(),
            image: None, // Will use default for service type
            persist_data: *persist_data,
            env: env.clone().unwrap_or_default(),
            ports: vec![], // TODO: Extract from spec
            volumes,
            docker_args: vec![],
            health_check: health_check.clone(),
            init_scripts: init_scripts.clone().unwrap_or_default(),
            post_startup_tasks: post_startup_tasks.clone().unwrap_or_default(),
            depends_on: vec![], // TODO: Extract dependencies
            command: command.clone(),
            network_mode: Some(network_name.clone()),
            container_name: None,
            resources: None,
        };
        
        // Create Docker-based service
        let service = DockerLocalService::new(
            spec.component_name.clone(),
            service_type_enum,
            docker_client,
            config,
            network_name,
            data_dir,
        );
        
        manager.register(Box::new(service));
    }
    
    Ok(())
}

/// Register Stripe CLI as a process-based service
fn register_stripe_service(
    manager: &mut LocalServiceManager,
    spec: &ComponentBuildSpec,
    _command: Option<String>,
) -> Result<()> {
    info!("Registering Stripe CLI service: {}", spec.component_name);
    
    // Extract webhook URL from environment or use default
    let webhook_url = spec.env.as_ref()
        .and_then(|e| e.get("STRIPE_WEBHOOK_URL"))
        .cloned()
        .unwrap_or_else(|| "http://localhost:8080/api/stripe/webhook".to_string());
    
    // Use the stripe_cli constructor which finds the full path
    let service = ProcessLocalService::stripe_cli(
        spec.component_name.clone(),
        webhook_url,
    );
    
    manager.register(Box::new(service));
    Ok(())
}

/// Parse service type from string
fn parse_service_type(service_type: &str) -> LocalServiceType {
    match service_type.to_lowercase().as_str() {
        "postgresql" | "postgres" => LocalServiceType::PostgreSQL,
        "mysql" => LocalServiceType::MySQL,
        "mongodb" | "mongo" => LocalServiceType::MongoDB,
        "redis" => LocalServiceType::Redis,
        "localstack" => LocalServiceType::LocalStack,
        "minio" => LocalServiceType::MinIO,
        "elasticmq" => LocalServiceType::ElasticMQ,
        "stripe-cli" | "stripe" => LocalServiceType::StripeCLI,
        "mailhog" => LocalServiceType::MailHog,
        custom => LocalServiceType::Custom(custom.to_string()),
    }
}

/// Create default volume mappings for services that need data persistence
fn create_default_volumes(
    service_type: &LocalServiceType,
    data_dir: &PathBuf,
    component_name: &str,
) -> Vec<rush_local_services::VolumeMapping> {
    use rush_local_services::VolumeMapping;
    
    match service_type {
        LocalServiceType::PostgreSQL => {
            // Create PostgreSQL data directory
            let postgres_data = data_dir.join(component_name).join("postgres.db");
            if let Err(e) = std::fs::create_dir_all(&postgres_data) {
                eprintln!("Warning: Failed to create PostgreSQL data directory at {:?}: {}", postgres_data, e);
            }
            
            vec![VolumeMapping::new(
                postgres_data.to_string_lossy().to_string(),
                "/var/lib/postgresql/data".to_string(),
                false, // Not read-only
            )]
        }
        LocalServiceType::MySQL => {
            // Create MySQL data directory
            let mysql_data = data_dir.join(component_name).join("mysql.db");
            if let Err(e) = std::fs::create_dir_all(&mysql_data) {
                eprintln!("Warning: Failed to create MySQL data directory at {:?}: {}", mysql_data, e);
            }
            
            vec![VolumeMapping::new(
                mysql_data.to_string_lossy().to_string(),
                "/var/lib/mysql".to_string(),
                false,
            )]
        }
        LocalServiceType::MongoDB => {
            // Create MongoDB data directory
            let mongo_data = data_dir.join(component_name).join("mongo.db");
            if let Err(e) = std::fs::create_dir_all(&mongo_data) {
                eprintln!("Warning: Failed to create MongoDB data directory at {:?}: {}", mongo_data, e);
            }
            
            vec![VolumeMapping::new(
                mongo_data.to_string_lossy().to_string(),
                "/data/db".to_string(),
                false,
            )]
        }
        LocalServiceType::Redis => {
            // Create Redis data directory
            let redis_data = data_dir.join(component_name).join("redis.data");
            if let Err(e) = std::fs::create_dir_all(&redis_data) {
                eprintln!("Warning: Failed to create Redis data directory at {:?}: {}", redis_data, e);
            }
            
            vec![VolumeMapping::new(
                redis_data.to_string_lossy().to_string(),
                "/data".to_string(),
                false,
            )]
        }
        LocalServiceType::MinIO => {
            // Create MinIO data directory
            let minio_data = data_dir.join(component_name).join("minio.data");
            if let Err(e) = std::fs::create_dir_all(&minio_data) {
                eprintln!("Warning: Failed to create MinIO data directory at {:?}: {}", minio_data, e);
            }
            
            vec![VolumeMapping::new(
                minio_data.to_string_lossy().to_string(),
                "/data".to_string(),
                false,
            )]
        }
        LocalServiceType::LocalStack => {
            // LocalStack needs Docker socket and data persistence
            let localstack_data = data_dir.join(component_name).join("localstack.data");
            
            // Ensure the directory exists - log error if creation fails
            if let Err(e) = std::fs::create_dir_all(&localstack_data) {
                eprintln!("Warning: Failed to create LocalStack data directory at {:?}: {}", localstack_data, e);
            } else {
                println!("Created LocalStack data directory at: {:?}", localstack_data);
            }
            
            vec![
                VolumeMapping::new(
                    "/var/run/docker.sock".to_string(),
                    "/var/run/docker.sock".to_string(),
                    false,
                ),
                VolumeMapping::new(
                    localstack_data.to_string_lossy().to_string(),
                    "/var/lib/localstack".to_string(),  // Correct LocalStack data directory
                    false,
                ),
            ]
        }
        _ => vec![], // No default volumes for other services
    }
}
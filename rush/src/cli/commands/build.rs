use crate::build::{BuildContext, BuildType};
use crate::cli::context::CliContext;
use crate::core::config::Config;
use crate::core::environment::{load_environment_variables, setup_environment};
use crate::core::product::ProductLoader;
use crate::error::{Error, Result};
use crate::toolchain::{Platform, ToolchainContext};
use crate::utils::run_command;
use colored::Colorize;
use log::{debug, error, info, trace};
use std::collections::HashMap;
use std::path::Path;
use std::process;
use std::sync::Arc;

/// Executes the build command
pub async fn execute(config: Arc<Config>, matches: &clap::ArgMatches) -> Result<()> {
    trace!("Executing 'build' command");
    setup_environment();

    let product_name = matches
        .get_one::<String>("product_name")
        .ok_or_else(|| Error::InvalidInput("Product name is required".to_string()))?;

    let environment = matches
        .get_one::<String>("environment")
        .map(|s| s.as_str())
        .unwrap_or("local");

    debug!(
        "Loading environment variables for environment: {}",
        environment
    );
    load_environment_variables(environment)?;

    // Load product configuration
    let product_loader = ProductLoader::new(config.product_path());
    let product = product_loader.load_product(config.clone())?;

    // Create default platforms based on configuration
    let target_platform = Platform::default();
    let host_platform = Platform::default();

    // Setup toolchain
    let toolchain = Arc::new(ToolchainContext::new(
        host_platform.clone(),
        target_platform.clone(),
    ));

    info!(
        "Building product: {} for environment: {}",
        product_name, environment
    );

    // Build components
    let mut built_components = Vec::new();
    for (component_name, component) in product.components().iter() {
        info!("Building component: {}", component_name);

        // Parse build type from component - assuming build_type is a string that needs to be converted to BuildType
        let build_type = parse_build_type(&component.build_type)?;

        // Create a default domain for the component using product name and environment
        let domain = format!("{}.{}.example.com", component_name, environment);

        // Create build context
        let context = BuildContext {
            build_type,
            location: Some(component.location.clone()),
            target: target_platform.clone(),
            host: host_platform.clone(),
            rust_target: target_platform.to_rust_target(),
            toolchain: (*toolchain).clone(),
            services: HashMap::new(), // Empty services map, would be populated from product
            environment: environment.to_string(),
            domain,
            product_name: product_name.to_string(),
            product_uri: product.uri().to_string(),
            component: component_name.clone(),
            docker_registry: config.docker_registry().to_string(),
            image_name: format!("{}-{}", product_name, component_name),
            secrets: HashMap::new(), // Would be populated from a vault in a full implementation
            domains: HashMap::new(), // Empty domains map, would be populated from product
            env: HashMap::new(),     // Default to empty environment variables
        };

        print!("Building {} ... ", component_name);

        match build_component(&context, &toolchain).await {
            Ok(_) => {
                println!("[{}]", "OK".green().bold());
                built_components.push(component_name.clone());
            }
            Err(e) => {
                println!("[{}]", "FAILED".red().bold());
                error!("Failed to build component {}: {}", component_name, e);
                return Err(e);
            }
        }
    }

    info!("Successfully built {} components", built_components.len());
    Ok(())
}

// Helper function to parse build type from string
fn parse_build_type(build_type_str: &str) -> Result<BuildType> {
    match build_type_str {
        "RustBinary" => Ok(BuildType::RustBinary {
            location: String::new(),
            dockerfile_path: String::new(),
            context_dir: None,
            features: None,
            precompile_commands: None,
        }),
        "TrunkWasm" => Ok(BuildType::TrunkWasm {
            location: String::new(),
            dockerfile_path: String::new(),
            context_dir: None,
            ssr: false,
            features: None,
            precompile_commands: None,
        }),
        "DixiousWasm" => Ok(BuildType::DixiousWasm {
            location: String::new(),
            dockerfile_path: String::new(),
            context_dir: None,
        }),
        "Zola" => Ok(BuildType::Zola {
            location: String::new(),
            dockerfile_path: String::new(),
            context_dir: None,
        }),
        "Book" => Ok(BuildType::Book {
            location: String::new(),
            dockerfile_path: String::new(),
            context_dir: None,
        }),
        "Script" => Ok(BuildType::Script {
            location: String::new(),
            dockerfile_path: String::new(),
            context_dir: None,
        }),
        "PureKubernetes" => Ok(BuildType::PureKubernetes),
        "KubernetesInstallation" => Ok(BuildType::KubernetesInstallation {
            namespace: String::new(),
        }),
        "Ingress" => Ok(BuildType::Ingress {
            components: Vec::new(),
            dockerfile_path: String::new(),
            context_dir: None,
        }),
        "PureDockerImage" => Ok(BuildType::PureDockerImage {
            image_name_with_tag: String::new(),
            command: None,
            entrypoint: None,
        }),
        _ => Err(Error::InvalidInput(format!(
            "Unknown build type: {}",
            build_type_str
        ))),
    }
}

async fn build_component(context: &BuildContext, toolchain: &Arc<ToolchainContext>) -> Result<()> {
    match &context.build_type {
        BuildType::RustBinary { location, .. }
        | BuildType::TrunkWasm { location, .. }
        | BuildType::DixiousWasm { location, .. }
        | BuildType::Zola { location, .. }
        | BuildType::Book { location, .. }
        | BuildType::Script { location, .. } => {
            // Execute build script in component directory
            let build_script = generate_build_script(context)?;
            let component_dir = Path::new(location);

            if !component_dir.exists() {
                return Err(Error::FileSystem {
                    path: component_dir.to_path_buf(),
                    message: "Component directory not found".to_string(),
                });
            }

            debug!(
                "Executing build script for {} in {}",
                context.component, location
            );
            run_command("build", "sh", vec!["-c", &build_script])
                .await
                .map_err(|e| Error::Build(format!("Build script execution failed: {}", e)))?;

            // Build Docker image if needed
            build_docker_image(context, toolchain).await?;
        }
        BuildType::PureDockerImage {
            image_name_with_tag,
            ..
        } => {
            // Pull the Docker image
            debug!("Pulling Docker image: {}", image_name_with_tag);
            run_command(
                "docker pull",
                toolchain.docker(),
                vec!["pull", image_name_with_tag],
            )
            .await
            .map_err(|e| Error::Build(format!("Failed to pull Docker image: {}", e)))?;
        }
        BuildType::PureKubernetes => {
            // Nothing to build for pure Kubernetes
            debug!("No build steps for PureKubernetes component");
        }
        BuildType::KubernetesInstallation { .. } => {
            // Nothing to build for Kubernetes installation
            debug!("No build steps for KubernetesInstallation component");
        }
        BuildType::Ingress { components, .. } => {
            // Build Docker image for ingress
            debug!(
                "Building Ingress component that depends on: {:?}",
                components
            );
            build_docker_image(context, toolchain).await?;
        }
    }

    Ok(())
}

fn generate_build_script(context: &BuildContext) -> Result<String> {
    // A simple implementation - in a real implementation, this would use templates
    let mut script = String::from("set -e\n");

    // Add environment variables
    for (key, value) in &context.env {
        script.push_str(&format!("export {}=\"{}\"\n", key, value));
    }

    // Add domain variables
    for (key, value) in &context.domains {
        script.push_str(&format!(
            "export DOMAIN_{}=\"{}\"\n",
            key.to_uppercase(),
            value
        ));
    }

    // Add build commands based on build type
    match &context.build_type {
        BuildType::RustBinary {
            features,
            precompile_commands,
            ..
        } => {
            // Add precompile commands if any
            if let Some(commands) = precompile_commands {
                for cmd in commands {
                    script.push_str(&format!("{}\n", cmd));
                }
            }

            // Add basic Rust build command
            let mut build_cmd = "cargo build --release".to_string();

            // Add target if cross-compiling
            if context.target != context.host {
                build_cmd.push_str(&format!(" --target {}", context.rust_target));
            }

            // Add features if any
            if let Some(feat) = features {
                build_cmd.push_str(&format!(" --features {}", feat.join(",")));
            }

            script.push_str(&build_cmd);
        }
        BuildType::TrunkWasm {
            ssr,
            
            precompile_commands,
            ..
        } => {
            // Add precompile commands if any
            if let Some(commands) = precompile_commands {
                for cmd in commands {
                    script.push_str(&format!("{}\n", cmd));
                }
            }

            // Basic trunk build command
            let mut build_cmd = "trunk build --release".to_string();

            // Add features for CSR or hydration
            if *ssr {
                build_cmd.push_str(" --features hydration");
            } else {
                build_cmd.push_str(" --features csr");
            }

            script.push_str(&build_cmd);
            script.push_str("\n");

            // If SSR is enabled, also build the server
            if *ssr {
                let mut server_cmd = "cargo build --release --features ssr".to_string();

                // Add target if cross-compiling
                if context.target != context.host {
                    server_cmd.push_str(&format!(" --target {}", context.rust_target));
                }

                script.push_str(&server_cmd);
            }
        }
        BuildType::DixiousWasm { .. } => {
            script.push_str("dx build --platform web --release\n");
        }
        BuildType::Zola { .. } => {
            script.push_str("zola build --output-dir ./dist\n");
        }
        BuildType::Book { .. } => {
            script.push_str("mdbook build\n");
        }
        BuildType::Script { .. } => {
            // Custom script should be in the component directory
            script.push_str("./build.sh\n");
        }
        _ => {
            // Other build types don't need a build script
        }
    }

    Ok(script)
}

async fn build_docker_image(
    context: &BuildContext,
    toolchain: &Arc<ToolchainContext>,
) -> Result<()> {
    let dockerfile_path = match &context.build_type {
        BuildType::RustBinary {
            dockerfile_path, ..
        }
        | BuildType::TrunkWasm {
            dockerfile_path, ..
        }
        | BuildType::DixiousWasm {
            dockerfile_path, ..
        }
        | BuildType::Zola {
            dockerfile_path, ..
        }
        | BuildType::Book {
            dockerfile_path, ..
        }
        | BuildType::Script {
            dockerfile_path, ..
        }
        | BuildType::Ingress {
            dockerfile_path, ..
        } => dockerfile_path,
        _ => return Ok(()),
    };

    let context_dir = match &context.build_type {
        BuildType::RustBinary { context_dir, .. }
        | BuildType::TrunkWasm { context_dir, .. }
        | BuildType::DixiousWasm { context_dir, .. }
        | BuildType::Zola { context_dir, .. }
        | BuildType::Book { context_dir, .. }
        | BuildType::Script { context_dir, .. }
        | BuildType::Ingress { context_dir, .. } => {
            context_dir.clone().unwrap_or_else(|| ".".to_string())
        }
        _ => ".".to_string(),
    };

    let image_tag = format!("{}-{}", context.product_name, context.component);

    debug!(
        "Building Docker image: {} from {}",
        image_tag, dockerfile_path
    );

    run_command(
        "docker build",
        toolchain.docker(),
        vec![
            "build",
            "-t",
            &image_tag,
            "-f",
            dockerfile_path,
            &context_dir,
        ],
    )
    .await
    .map_err(|e| Error::Build(format!("Docker build failed: {}", e)))?;

    Ok(())
}

/// Execute build command using CLI context (wrapper)
pub async fn execute_with_context(ctx: &mut CliContext) -> Result<()> {
    trace!("Building components");
    match ctx.reactor.build().await {
        Ok(_) => {
            trace!("Build completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Build failed: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}

/// Build and push images
pub async fn push(ctx: &mut CliContext) -> Result<()> {
    trace!("Building and pushing components");
    match ctx.reactor.build_and_push().await {
        Ok(_) => {
            trace!("Build and push completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Build and push failed: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}

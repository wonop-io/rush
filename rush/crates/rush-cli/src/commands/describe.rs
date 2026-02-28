use std::path::PathBuf;
use std::sync::Arc;

use rush_build::{BuildType, ComponentBuildSpec};
use rush_config::Config;
use rush_container::tagging::ImageTagGenerator;
use rush_container::ContainerService;
use rush_core::error::{Error, Result};
use rush_security::SecretsProvider;
use rush_toolchain::ToolchainContext;
use tera::Context;

use crate::args::DescribeCommand;

pub async fn execute(
    cmd: DescribeCommand,
    config: &Arc<Config>,
    services: &[ContainerService],
    toolchain: &Arc<ToolchainContext>,
    secrets_provider: &Arc<dyn SecretsProvider>,
) -> Result<()> {
    match cmd {
        DescribeCommand::Toolchain => {
            println!("{toolchain:#?}");
            Ok(())
        }
        DescribeCommand::Images => describe_images(config, toolchain).await,
        DescribeCommand::Services => {
            // Create a view of services that contains the essential information
            let service_info = services
                .iter()
                .map(|s| (s.name.clone(), s.host.clone(), s.port, s.target_port))
                .collect::<Vec<_>>();
            println!("{service_info:#?}");
            Ok(())
        }
        DescribeCommand::BuildScript { component_name } => {
            let _service = services
                .iter()
                .find(|s| s.name == component_name)
                .ok_or_else(|| {
                    Error::InvalidInput(format!("Component '{component_name}' not found"))
                })?;

            let _secrets = secrets_provider
                .get_secrets(
                    config.product_name(),
                    &component_name,
                    &config.environment().into(),
                )
                .await
                .map_err(|e| Error::Vault(format!("Failed to get secrets: {e}")))?;

            // Building the context would require access to the build context
            // This is a placeholder implementation
            Err(Error::InvalidInput(format!(
                "Build script functionality not implemented for component '{component_name}'"
            )))
        }
        DescribeCommand::BuildContext { component_name } => {
            let service = services
                .iter()
                .find(|s| s.name == component_name)
                .ok_or_else(|| {
                    Error::InvalidInput(format!("Component '{component_name}' not found"))
                })?;

            let _secrets = secrets_provider
                .get_secrets(
                    config.product_name(),
                    &component_name,
                    &config.environment().into(),
                )
                .await
                .map_err(|e| Error::Vault(format!("Failed to get secrets: {e}")))?;

            // Convert service to a context for display
            let service_context = serde_json::to_value(service)
                .map_err(|e| Error::Template(format!("Failed to serialize service: {e}")))?;

            let tera_ctx = Context::from_value(service_context)
                .map_err(|e| Error::Template(format!("Failed to create context: {e}")))?;

            println!("{tera_ctx:#?}");
            Ok(())
        }
        DescribeCommand::Artefacts { component_name } => {
            let service = services
                .iter()
                .find(|s| s.name == component_name)
                .ok_or_else(|| {
                    Error::InvalidInput(format!("Component '{component_name}' not found"))
                })?;

            // In the new architecture, artefacts might be handled differently
            println!("Artefacts for component: {component_name}");
            println!("Service details: {service:#?}");

            Err(Error::InvalidInput(format!(
                "Artefacts functionality not implemented for component '{component_name}'"
            )))
        }
        DescribeCommand::K8s => {
            // In the new architecture, Kubernetes manifests might be accessed differently
            println!("Kubernetes manifests functionality not implemented yet");

            Err(Error::InvalidInput(
                "Kubernetes manifests functionality not implemented".to_string(),
            ))
        }
    }
}

/// Describes how images will be built for all components
async fn describe_images(config: &Arc<Config>, toolchain: &Arc<ToolchainContext>) -> Result<()> {
    use std::fs;

    let product_path = config.product_path();

    // Read stack configuration
    let stack_config = fs::read_to_string(format!("{}/stack.spec.yaml", product_path.display()))
        .map_err(|e| Error::Config(format!("Failed to read stack config: {e}")))?;

    // Parse stack spec
    let spec = serde_yaml::from_str::<serde_yaml::Value>(&stack_config)
        .map_err(|e| Error::Config(format!("Failed to parse stack config: {e}")))?;

    // Create tag generator
    let tag_generator = Arc::new(ImageTagGenerator::new(
        toolchain.clone(),
        product_path.to_path_buf(),
    ));

    println!("=== Image Build Information ===\n");
    println!("Product: {}", config.product_name());
    println!("Environment: {}", config.environment());
    println!("Product Directory: {}\n", product_path.display());

    // Build component specs from stack configuration
    if let Some(components) = spec.as_mapping() {
        let mut component_infos = Vec::new();

        for (name, component_config) in components {
            if let Some(name_str) = name.as_str() {
                // Create ComponentBuildSpec using the same approach as in from_product_dir
                let mut component_config_with_name = component_config.clone();
                if let serde_yaml::Value::Mapping(ref mut map) = component_config_with_name {
                    map.insert(
                        serde_yaml::Value::String("component_name".to_string()),
                        serde_yaml::Value::String(name_str.to_string()),
                    );
                }

                let spec = rush_build::ComponentBuildSpec::from_yaml(
                    config.clone(),
                    rush_build::Variables::empty(),
                    &component_config_with_name,
                );

                // Compute tag for this component
                let tag = tag_generator.compute_tag(&spec).unwrap_or_else(|e| {
                    eprintln!("Warning: Failed to compute tag for {name_str}: {e}");
                    "latest".to_string()
                });

                // Collect information about this component
                let info = ComponentImageInfo {
                    name: name_str.to_string(),
                    spec,
                    tag,
                    product_name: config.product_name().to_string(),
                };

                component_infos.push(info);
            }
        }

        // Sort components by name for consistent output
        component_infos.sort_by(|a, b| a.name.cmp(&b.name));

        // Display information for each component
        for info in component_infos {
            print_component_image_info(&info, product_path);
            println!(); // Add spacing between components
        }
    }

    Ok(())
}

/// Information about a component's image build
struct ComponentImageInfo {
    name: String,
    spec: ComponentBuildSpec,
    tag: String,
    product_name: String,
}

/// Print detailed information about how a component's image will be built
fn print_component_image_info(info: &ComponentImageInfo, product_path: &std::path::Path) {
    println!("Component: {}", info.name);
    println!("─────────────────────────────────");

    // Image name and tag
    let image_name = format!("{}/{}", info.product_name, info.name);
    println!("  Image Name: {image_name}");
    println!("  Image Tag: {}", info.tag);
    println!("  Full Image: {}:{}", image_name, info.tag);

    // Build type information
    println!("  Build Type: {}", format_build_type(&info.spec.build_type));

    // Context and Dockerfile information
    if let Some(dockerfile) = info.spec.build_type.dockerfile_path() {
        let dockerfile_path = product_path.join(dockerfile);
        println!("  Dockerfile: {dockerfile}");
        if dockerfile_path.exists() {
            println!("    └─ Status: ✓ Found at {}", dockerfile_path.display());
        } else {
            println!(
                "    └─ Status: ✗ Not found at {}",
                dockerfile_path.display()
            );
        }
    }

    // Context directory
    let context_dir = determine_context_dir(&info.spec.build_type, product_path);
    println!("  Context Directory: {}", context_dir.display());
    if context_dir.exists() {
        // Count files in context (excluding common build artifacts)
        if let Ok(entries) = std::fs::read_dir(&context_dir) {
            let file_count = entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let name = e.file_name();
                    let name_str = name.to_string_lossy();
                    !name_str.starts_with('.')
                        && name_str != "target"
                        && name_str != "node_modules"
                        && name_str != "dist"
                })
                .count();
            println!("    └─ Status: ✓ Found ({file_count} entries)");
        }
    } else {
        println!("    └─ Status: ✗ Directory not found");
    }

    // Build-specific information
    match &info.spec.build_type {
        BuildType::RustBinary {
            features,
            precompile_commands,
            ..
        } => {
            if let Some(features) = features {
                if !features.is_empty() {
                    println!("  Rust Features: {}", features.join(", "));
                }
            }
            if let Some(commands) = precompile_commands {
                if !commands.is_empty() {
                    println!("  Precompile Commands:");
                    for cmd in commands {
                        println!("    - {cmd}");
                    }
                }
            }
        }
        BuildType::TrunkWasm {
            ssr,
            features,
            precompile_commands,
            ..
        } => {
            println!(
                "  Server-Side Rendering: {}",
                if *ssr { "Yes" } else { "No" }
            );
            if let Some(features) = features {
                if !features.is_empty() {
                    println!("  Rust Features: {}", features.join(", "));
                }
            }
            if let Some(commands) = precompile_commands {
                if !commands.is_empty() {
                    println!("  Precompile Commands:");
                    for cmd in commands {
                        println!("    - {cmd}");
                    }
                }
            }
        }
        BuildType::LocalService {
            service_type,
            version,
            persist_data,
            ..
        } => {
            println!("  Service Type: {service_type}");
            if let Some(version) = version {
                println!("  Version: {version}");
            }
            println!(
                "  Persist Data: {}",
                if *persist_data { "Yes" } else { "No" }
            );
            println!("  Note: LocalService uses pre-built images, not built locally");
        }
        BuildType::PureDockerImage {
            image_name_with_tag,
            ..
        } => {
            println!("  Base Image: {image_name_with_tag}");
            println!("  Note: Uses existing Docker image, not built locally");
        }
        BuildType::Ingress { components, .. } => {
            println!("  Routed Components: {}", components.join(", "));
        }
        BuildType::Bazel {
            targets,
            output_dir,
            base_image,
            oci_load_target,
            ..
        } => {
            if let Some(oci_target) = oci_load_target {
                println!("  OCI Mode: rules_oci");
                println!("  OCI Load Target: {oci_target}");
            } else {
                if let Some(targets) = targets {
                    println!("  Bazel Targets: {}", targets.join(", "));
                } else {
                    println!("  Bazel Targets: //... (all)");
                }
                println!("  Output Directory: {output_dir}");
                if let Some(base) = base_image {
                    println!("  Base Image: {base}");
                }
            }
        }
        _ => {}
    }

    // Additional useful information
    if let Some(mount) = &info.spec.mount_point {
        println!("  Mount Point: {mount}");
    }

    if let Some(port) = info.spec.port {
        println!("  Exposed Port: {port}");
    }

    if let Some(env) = &info.spec.env {
        if !env.is_empty() {
            println!("  Environment Variables: {} defined", env.len());
        }
    }
}

/// Format the build type for display
fn format_build_type(build_type: &BuildType) -> &str {
    match build_type {
        BuildType::TrunkWasm { .. } => "Trunk WASM",
        BuildType::RustBinary { .. } => "Rust Binary",
        BuildType::DixiousWasm { .. } => "Dixious WASM",
        BuildType::Script { .. } => "Script",
        BuildType::Zola { .. } => "Zola Static Site",
        BuildType::Book { .. } => "mdBook Documentation",
        BuildType::Ingress { .. } => "Ingress/Proxy",
        BuildType::PureDockerImage { .. } => "Pre-built Docker Image",
        BuildType::LocalService { .. } => "Local Service",
        BuildType::PureKubernetes => "Kubernetes Only",
        BuildType::KubernetesInstallation { .. } => "Kubernetes Installation",
        BuildType::Bazel { .. } => "Bazel Build",
    }
}

/// Determine the context directory for a build
fn determine_context_dir(build_type: &BuildType, product_path: &std::path::Path) -> PathBuf {
    match build_type {
        BuildType::TrunkWasm {
            location,
            context_dir,
            ..
        }
        | BuildType::DixiousWasm {
            location,
            context_dir,
            ..
        }
        | BuildType::RustBinary {
            location,
            context_dir,
            ..
        }
        | BuildType::Script {
            location,
            context_dir,
            ..
        }
        | BuildType::Zola {
            location,
            context_dir,
            ..
        }
        | BuildType::Book {
            location,
            context_dir,
            ..
        } => {
            let component_base = product_path.join(location);
            if let Some(ctx) = context_dir {
                component_base.join(ctx)
            } else {
                component_base
            }
        }
        BuildType::Ingress { context_dir, .. } => {
            if let Some(ctx) = context_dir {
                product_path.join(ctx)
            } else {
                product_path.to_path_buf()
            }
        }
        BuildType::Bazel {
            location,
            context_dir,
            ..
        } => {
            let component_base = product_path.join(location);
            if let Some(ctx) = context_dir {
                component_base.join(ctx)
            } else {
                component_base
            }
        }
        _ => product_path.to_path_buf(),
    }
}

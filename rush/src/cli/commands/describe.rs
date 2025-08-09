use crate::cli::DescribeCommand;
use crate::container::ContainerService;
use crate::core::config::Config;
use crate::error::{Error, Result};
use crate::security::SecretsProvider;
use crate::toolchain::ToolchainContext;
use colored::Colorize;
use std::sync::Arc;
use tera::Context;

pub async fn execute(
    cmd: DescribeCommand,
    config: &Arc<Config>,
    services: &[ContainerService],
    toolchain: &Arc<ToolchainContext>,
    secrets_provider: &Arc<dyn SecretsProvider>,
) -> Result<()> {
    match cmd {
        DescribeCommand::Toolchain => {
            println!("{:#?}", toolchain);
            Ok(())
        }
        DescribeCommand::Images => {
            println!("{:#?}", services);
            Ok(())
        }
        DescribeCommand::Services => {
            // Create a view of services that contains the essential information
            let service_info = services
                .iter()
                .map(|s| (s.name.clone(), s.host.clone(), s.port, s.target_port))
                .collect::<Vec<_>>();
            println!("{:#?}", service_info);
            Ok(())
        }
        DescribeCommand::BuildScript { component_name } => {
            let service = services
                .iter()
                .find(|s| s.name == component_name)
                .ok_or_else(|| {
                    Error::InvalidInput(format!("Component '{}' not found", component_name))
                })?;

            let secrets = secrets_provider
                .get_secrets(
                    config.product_name(),
                    &component_name,
                    &config.environment().into(),
                )
                .await
                .map_err(|e| Error::Vault(format!("Failed to get secrets: {}", e)))?;

            // Building the context would require access to the build context
            // This is a placeholder implementation
            return Err(Error::InvalidInput(format!(
                "Build script functionality not implemented for component '{}'",
                component_name
            )));
        }
        DescribeCommand::BuildContext { component_name } => {
            let service = services
                .iter()
                .find(|s| s.name == component_name)
                .ok_or_else(|| {
                    Error::InvalidInput(format!("Component '{}' not found", component_name))
                })?;

            let secrets = secrets_provider
                .get_secrets(
                    config.product_name(),
                    &component_name,
                    &config.environment().into(),
                )
                .await
                .map_err(|e| Error::Vault(format!("Failed to get secrets: {}", e)))?;

            // Convert service to a context for display
            let service_context = serde_json::to_value(service)
                .map_err(|e| Error::Template(format!("Failed to serialize service: {}", e)))?;

            let tera_ctx = Context::from_value(service_context)
                .map_err(|e| Error::Template(format!("Failed to create context: {}", e)))?;

            println!("{:#?}", tera_ctx);
            Ok(())
        }
        DescribeCommand::Artefacts { component_name } => {
            let service = services
                .iter()
                .find(|s| s.name == component_name)
                .ok_or_else(|| {
                    Error::InvalidInput(format!("Component '{}' not found", component_name))
                })?;

            // In the new architecture, artefacts might be handled differently
            println!("Artefacts for component: {}", component_name);
            println!("Service details: {:#?}", service);

            return Err(Error::InvalidInput(format!(
                "Artefacts functionality not implemented for component '{}'",
                component_name
            )));
        }
        DescribeCommand::K8s => {
            // In the new architecture, Kubernetes manifests might be accessed differently
            println!("Kubernetes manifests functionality not implemented yet");

            return Err(Error::InvalidInput(
                "Kubernetes manifests functionality not implemented".to_string(),
            ));
        }
    }
}

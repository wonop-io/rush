use crate::commands;
use crate::context::CliContext;
use crate::args::DescribeCommand;
use clap::ArgMatches;
use log::{error, trace};
use rush_core::error::Result;
use std::process;
use std::sync::Arc;

/// Execute the appropriate command based on command line arguments
pub async fn execute_command(matches: &ArgMatches, ctx: &mut CliContext) -> Result<()> {
    // Validate secrets before executing commands
    if let Err(e) = ctx
        .secrets_context
        .validate_vault(ctx.vault.clone(), &ctx.environment)
        .await
    {
        error!("Missing secrets in vault: {}", e);
        eprintln!("{e}");
        process::exit(1);
    }

    // Route to appropriate command handlers
    if let Some(_validate_matches) = matches.subcommand_matches("validate") {
        trace!("Executing validate command");
        // TODO: Implement validate with context
        Ok(())
    } else if let Some(describe_matches) = matches.subcommand_matches("describe") {
        trace!("Executing describe command");

        // Parse the describe subcommand
        let describe_cmd = if describe_matches.subcommand_matches("toolchain").is_some() {
            DescribeCommand::Toolchain
        } else if describe_matches.subcommand_matches("images").is_some() {
            DescribeCommand::Images
        } else if describe_matches.subcommand_matches("services").is_some() {
            DescribeCommand::Services
        } else if let Some(build_script_matches) = describe_matches.subcommand_matches("build-script") {
            let component_name = build_script_matches
                .get_one::<String>("component")
                .unwrap_or(&String::new())
                .clone();
            DescribeCommand::BuildScript { component_name }
        } else if let Some(build_context_matches) = describe_matches.subcommand_matches("build-context") {
            let component_name = build_context_matches
                .get_one::<String>("component")
                .unwrap_or(&String::new())
                .clone();
            DescribeCommand::BuildContext { component_name }
        } else if let Some(artefacts_matches) = describe_matches.subcommand_matches("artefacts") {
            let component_name = artefacts_matches
                .get_one::<String>("component")
                .unwrap_or(&String::new())
                .clone();
            DescribeCommand::Artefacts { component_name }
        } else if describe_matches.subcommand_matches("k8s").is_some() {
            DescribeCommand::K8s
        } else {
            // Default to images if no subcommand
            DescribeCommand::Images
        };

        // For describe commands, we can use a dummy secrets provider since
        // most describe commands don't actually use secrets (and those that do
        // just fetch them but don't use them).
        // TODO: Refactor describe to not require SecretsProvider for all commands
        #[derive(Debug)]
        struct DummySecretsProvider;

        use rush_security::secrets::{SecretError, Environment};

        #[async_trait::async_trait]
        impl rush_security::SecretsProvider for DummySecretsProvider {
            async fn get_secrets(
                &self,
                _product_name: &str,
                _component_name: &str,
                _environment: &Environment,
            ) -> std::result::Result<std::collections::HashMap<String, String>, SecretError> {
                Ok(std::collections::HashMap::new())
            }

            async fn set_secrets(
                &mut self,
                _product_name: &str,
                _component_name: &str,
                _environment: &Environment,
                _secrets: std::collections::HashMap<String, String>,
            ) -> std::result::Result<(), SecretError> {
                Ok(())
            }

            async fn delete_all_secrets(
                &mut self,
                _product_name: &str,
                _component_name: &str,
                _environment: &Environment,
            ) -> std::result::Result<(), SecretError> {
                Ok(())
            }
        }

        let dummy_provider: Arc<dyn rush_security::SecretsProvider> = Arc::new(DummySecretsProvider);

        // Execute the describe command
        // Most describe commands don't actually need services or secrets
        commands::describe::execute(
            describe_cmd,
            &ctx.config,
            &[], // Empty services array - describe doesn't need running services
            &ctx.toolchain,
            &dummy_provider,
        ).await
    } else if let Some(vault_matches) = matches.subcommand_matches("vault") {
        trace!("Executing vault command");
        commands::vault::execute(vault_matches, ctx).await
    } else if let Some(secrets_matches) = matches.subcommand_matches("secrets") {
        trace!("Executing secrets command");
        commands::secrets::execute(secrets_matches, ctx).await
    } else if let Some(profile_matches) = matches.subcommand_matches("profile") {
        trace!("Executing profile command");
        commands::profile::execute(profile_matches, ctx).await
    } else if let Some(dev_matches) = matches.subcommand_matches("dev") {
        trace!("Executing dev command");

        // Get the output format from CLI arguments
        let output_format = dev_matches
            .get_one::<String>("output-format")
            .map(|s| s.as_str())
            .unwrap_or("auto");

        let no_color = dev_matches.get_flag("no-color");

        // Create a sink using the new simple system
        let sink = rush_output::simple::create_sink(output_format, no_color);

        // Set the sink on the reactor
        ctx.reactor.set_output_sink_boxed(sink);

        // Note: Local services are already started in context_builder before .env generation
        // We just need to launch the application containers now
        let result = ctx.reactor.launch().await;
        
        // Stop local services when the reactor exits
        // The component that started them is responsible for stopping them
        ctx.stop_local_services().await;
        
        match result {
            Ok(_) => {
                trace!("Development environment completed successfully");
                Ok(())
            }
            Err(e) => {
                error!("Failed to launch development environment: {}", e);
                eprintln!("{e}");
                process::exit(1);
            }
        }
    } else if let Some(build_matches) = matches.subcommand_matches("build") {
        trace!("Executing build command");
        // Check for force-rebuild flag
        if build_matches.get_flag("force-rebuild") {
            ctx.reactor.set_force_rebuild(true);
        }
        commands::build::execute_with_context(ctx).await
    } else if matches.subcommand_matches("push").is_some() {
        trace!("Executing push command");
        commands::build::push(ctx).await
    } else if matches.subcommand_matches("rollout").is_some() {
        trace!("Executing rollout command");
        commands::rollout::execute(ctx).await
    } else if matches.subcommand_matches("install").is_some() {
        trace!("Executing install command");
        commands::install::execute(ctx).await
    } else if matches.subcommand_matches("uninstall").is_some() {
        trace!("Executing uninstall command");
        commands::install::uninstall(ctx).await
    } else if let Some(deploy_matches) = matches.subcommand_matches("deploy") {
        trace!("Executing deploy command");
        
        // Parse deployment configuration from command line arguments
        let mut deployment_config = commands::deploy::DeploymentConfig::default();
        
        // Check for dry-run flag
        if deploy_matches.get_flag("dry-run") {
            deployment_config.dry_run = true;
        }
        
        // Check for force rebuild flag
        if deploy_matches.get_flag("force-rebuild") {
            deployment_config.force_rebuild = true;
        }
        
        // Check for skip-push flag
        if deploy_matches.get_flag("skip-push") {
            deployment_config.skip_push = true;
        }
        
        // Check for no-wait flag
        if deploy_matches.get_flag("no-wait") {
            deployment_config.wait_for_ready = false;
        }
        
        // Check for no-rollback flag
        if deploy_matches.get_flag("no-rollback") {
            deployment_config.auto_rollback = false;
        }
        
        // Check for deployment strategy
        if let Some(strategy) = deploy_matches.get_one::<String>("strategy") {
            deployment_config.strategy = match strategy.as_str() {
                "rolling" => commands::deploy::DeploymentStrategy::RollingUpdate { 
                    max_surge: 1, 
                    max_unavailable: 1 
                },
                "blue-green" => commands::deploy::DeploymentStrategy::BlueGreen,
                "canary" => commands::deploy::DeploymentStrategy::Canary { percentage: 10 },
                "direct" => commands::deploy::DeploymentStrategy::Direct,
                _ => commands::deploy::DeploymentStrategy::default(),
            };
        }
        
        // Execute deployment
        commands::deploy::execute(ctx.config.clone(), deployment_config).await
    } else if let Some(apply_matches) = matches.subcommand_matches("apply") {
        trace!("Executing apply command");
        
        // Check for dry-run flag
        if apply_matches.get_flag("dry-run") {
            std::env::set_var("K8S_DRY_RUN", "true");
        }
        
        // Apply Kubernetes manifests without building
        ctx.reactor.apply().await
    } else if let Some(unapply_matches) = matches.subcommand_matches("unapply") {
        trace!("Executing unapply command");
        
        // Check for dry-run flag
        if unapply_matches.get_flag("dry-run") {
            std::env::set_var("K8S_DRY_RUN", "true");
        }
        
        // Remove Kubernetes resources
        ctx.reactor.unapply().await
    } else if matches.subcommand_matches("build").is_some() {
        trace!("Executing build command");
        // Build just the images without running containers
        ctx.reactor.build().await
    } else {
        Ok(())
    }
}

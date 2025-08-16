use crate::commands;
use crate::context::CliContext;
use rush_core::error::Result;
use clap::ArgMatches;
use log::{error, trace};
use std::process;

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
    } else if let Some(_describe_matches) = matches.subcommand_matches("describe") {
        trace!("Executing describe command");
        // TODO: Implement describe with context
        Ok(())
    } else if let Some(vault_matches) = matches.subcommand_matches("vault") {
        trace!("Executing vault command");
        commands::vault::execute(vault_matches, ctx).await
    } else if let Some(secrets_matches) = matches.subcommand_matches("secrets") {
        trace!("Executing secrets command");
        commands::secrets::execute(secrets_matches, ctx).await
    } else if let Some(dev_matches) = matches.subcommand_matches("dev") {
        trace!("Executing dev command");

        // Get the output format from CLI arguments
        let output_format = dev_matches
            .get_one::<String>("output-format")
            .map(|s| s.as_str())
            .unwrap_or("auto");
        
        let no_color = dev_matches.get_flag("no-color");
        
        eprintln!("DEBUG: Using output format: {} (no-color: {})", output_format, no_color);

        // Create a sink using the new simple system
        let sink = rush_output::simple::create_sink(output_format, no_color);
        
        // Set the sink on the reactor
        ctx.reactor.set_output_sink(sink);

        match ctx.reactor.launch().await {
            Ok(_) => {
                trace!("Development environment launched successfully");
                Ok(())
            }
            Err(e) => {
                error!("Failed to launch development environment: {}", e);
                eprintln!("{e}");
                process::exit(1);
            }
        }
    } else if matches.subcommand_matches("build").is_some() {
        trace!("Executing build command");
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
    } else if matches.subcommand_matches("deploy").is_some() {
        trace!("Executing deploy command");
        // TODO: Implement deploy with context
        Ok(())
    } else if matches.subcommand_matches("apply").is_some() {
        trace!("Executing apply command");
        // TODO: Implement apply with context
        Ok(())
    } else if matches.subcommand_matches("unapply").is_some() {
        trace!("Executing unapply command");
        // TODO: Implement unapply with context
        Ok(())
    } else {
        Ok(())
    }
}

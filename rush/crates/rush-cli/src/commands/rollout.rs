use crate::context::CliContext;
use rush_container::ContainerReactor;
use rush_config::Config;
use rush_config::environment::setup_environment;
use rush_core::error::Error;
use rush_core::error::Result;
use colored::Colorize;
use log::{error, trace};
use std::process;
use std::sync::Arc;

pub struct RolloutCommand;

impl RolloutCommand {
    pub async fn execute(
        _config: Arc<Config>,
        container_reactor: &mut ContainerReactor,
    ) -> Result<()> {
        println!("{}", "Rolling out product".bold().white());

        // Ensure environment is properly set up
        setup_environment();

        match container_reactor.rollout().await {
            Ok(_) => {
                println!("{}", "Rollout completed successfully".green().bold());
                Ok(())
            }
            Err(e) => {
                eprintln!("{}: {}", "Rollout failed".red().bold(), e);
                Err(Error::Deploy(format!("Failed to rollout: {e}")))
            }
        }
    }
}

/// Execute rollout command using CLI context
pub async fn execute(ctx: &mut CliContext) -> Result<()> {
    trace!("Executing rollout");
    match ctx.reactor.rollout().await {
        Ok(_) => {
            trace!("Rollout completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Rollout failed: {}", e);
            eprintln!("{e}");
            process::exit(1);
        }
    }
}

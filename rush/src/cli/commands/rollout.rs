use crate::container::ContainerReactor;
use crate::core::config::Config;
use crate::core::environment::setup_environment;
use crate::error::Error;
use crate::error::Result;
use colored::Colorize;
use std::sync::Arc;

pub struct RolloutCommand;

impl RolloutCommand {
    pub async fn execute(
        config: Arc<Config>,
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
                Err(Error::Deploy(format!("Failed to rollout: {}", e)))
            }
        }
    }
}

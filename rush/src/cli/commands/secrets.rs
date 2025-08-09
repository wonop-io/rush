use crate::cli::context::CliContext;
use crate::error::Result;
use clap::ArgMatches;
use log::{error, trace};
use std::process;

pub async fn execute(matches: &ArgMatches, ctx: &mut CliContext) -> Result<()> {
    trace!("Executing 'secrets' subcommand");

    if matches.subcommand_matches("init").is_some() {
        initialize_secrets(ctx).await
    } else {
        Ok(())
    }
}

async fn initialize_secrets(ctx: &mut CliContext) -> Result<()> {
    // First create the vault if needed
    match ctx
        .vault
        .lock()
        .unwrap()
        .create_vault(&ctx.product_name)
        .await
    {
        Ok(_) => (),
        Err(e) => {
            error!("Failed to create vault: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }

    trace!("Initializing secrets");
    match ctx
        .secrets_context
        .populate(ctx.vault.clone(), &ctx.environment)
        .await
    {
        Ok(_) => {
            trace!("Secrets initialized successfully");
            Ok(())
        }
        Err(e) => {
            error!("Failed to initialize secrets: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}
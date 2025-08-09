use super::*;
use crate::create_vault;
use serde_json::from_str;
use std::collections::HashMap;
use std::process;

pub async fn execute(matches: &ArgMatches, ctx: &mut CliContext) -> Result<(), std::io::Error> {
    trace!("Executing 'vault' subcommand");

    if let Some(matches) = matches.subcommand_matches("migrate") {
        migrate_vault(matches, ctx).await
    } else if matches.subcommand_matches("create").is_some() {
        create_vault_cmd(ctx).await
    } else if let Some(matches) = matches.subcommand_matches("add") {
        add_secrets(matches, ctx).await
    } else if let Some(matches) = matches.subcommand_matches("remove") {
        remove_secrets(matches, ctx).await
    } else {
        Ok(())
    }
}

async fn migrate_vault(matches: &ArgMatches, ctx: &mut CliContext) -> Result<(), std::io::Error> {
    let dest = matches.get_one::<String>("dest").unwrap();
    let product_path = std::path::PathBuf::from(ctx.config.product_path());
    let dest_vault = create_vault(&product_path, &ctx.config, dest.as_str());
    trace!("Migrating secrets to: {}", dest);

    let mut dest_vault = dest_vault.lock().unwrap();
    dest_vault.create_vault(&ctx.product_name);

    let vault = ctx.vault.lock().unwrap();
    let manifests = ctx.reactor.cluster_manifests();

    println!("Migrating:");
    for component_name in ctx.reactor.available_components() {
        println!(" - {}", component_name);
        let secrets = vault
            .get(&ctx.product_name, &component_name, &ctx.environment)
            .await
            .unwrap_or_default();

        if !secrets.is_empty() {
            dest_vault
                .set(
                    &ctx.product_name,
                    &component_name,
                    &ctx.environment,
                    secrets,
                )
                .await
                .unwrap();
        }
    }
    Ok(())
}

async fn create_vault_cmd(ctx: &mut CliContext) -> Result<(), std::io::Error> {
    trace!("Creating vault");
    match ctx
        .vault
        .lock()
        .unwrap()
        .create_vault(&ctx.product_name)
        .await
    {
        Ok(_) => {
            trace!("Vault created successfully");
            Ok(())
        }
        Err(e) => {
            error!("Failed to create vault: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}

async fn add_secrets(matches: &ArgMatches, ctx: &mut CliContext) -> Result<(), std::io::Error> {
    let component_name = matches.get_one::<String>("component_name").unwrap();
    let secrets_str = matches.get_one::<String>("secrets").unwrap();
    trace!("Adding: {}", secrets_str);

    let secrets: HashMap<String, String> = match from_str(secrets_str) {
        Ok(s) => s,
        Err(e) => {
            error!("Invalid secrets format: {}", e);
            eprintln!("Invalid secrets format: {}", e);
            process::exit(1);
        }
    };

    trace!("Adding secrets to vault");
    match ctx
        .vault
        .lock()
        .unwrap()
        .set(&ctx.product_name, component_name, &ctx.environment, secrets)
        .await
    {
        Ok(_) => {
            trace!("Secrets added successfully");
            Ok(())
        }
        Err(e) => {
            error!("Failed to add secrets: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}

async fn remove_secrets(matches: &ArgMatches, ctx: &mut CliContext) -> Result<(), std::io::Error> {
    let component_name = matches.get_one::<String>("component_name").unwrap();
    trace!("Removing secrets from vault");

    match ctx
        .vault
        .lock()
        .unwrap()
        .remove(&ctx.product_name, component_name, &ctx.environment)
        .await
    {
        Ok(_) => {
            trace!("Secrets removed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Failed to remove secrets: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}

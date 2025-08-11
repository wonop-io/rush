use crate::cli::context::CliContext;
use crate::core::config::Config;
use crate::error::{Error, Result};
use crate::security::Vault;
use crate::security::{Environment, SecretsProvider};
use clap::ArgMatches;
use log::{error, trace};
use std::collections::HashMap;
use std::process;
use std::sync::{Arc, Mutex};

/// Manages vault operations
pub struct VaultCommand {
    config: Arc<Config>,
    vault: Arc<Mutex<dyn Vault + Send>>,
    secrets_provider: Arc<dyn SecretsProvider>,
}

impl VaultCommand {
    /// Creates a new vault command
    pub fn new(
        config: Arc<Config>,
        vault: Arc<Mutex<dyn Vault + Send>>,
        secrets_provider: Arc<dyn SecretsProvider>,
    ) -> Self {
        Self {
            config,
            vault,
            secrets_provider,
        }
    }

    /// Creates a new vault
    pub async fn create(&self, product_name: &str) -> Result<()> {
        match self.vault.lock().unwrap().create_vault(product_name).await {
            Ok(_) => {
                println!("Vault created successfully for {}", product_name);
                Ok(())
            }
            Err(e) => Err(Error::Vault(format!("Failed to create vault: {}", e))),
        }
    }

    /// Adds secrets to vault for a component
    pub async fn add(
        &self,
        product_name: &str,
        component_name: &str,
        environment: &str,
        secrets: HashMap<String, String>,
    ) -> Result<()> {
        match self
            .vault
            .lock()
            .unwrap()
            .set(product_name, component_name, environment, secrets)
            .await
        {
            Ok(_) => {
                println!(
                    "Secrets added successfully for {}/{} in environment {}",
                    product_name, component_name, environment
                );
                Ok(())
            }
            Err(e) => Err(Error::Vault(format!("Failed to add secrets: {}", e))),
        }
    }

    /// Removes secrets from vault for a component
    pub async fn remove(
        &self,
        product_name: &str,
        component_name: &str,
        environment: &str,
    ) -> Result<()> {
        match self
            .vault
            .lock()
            .unwrap()
            .remove(product_name, component_name, environment)
            .await
        {
            Ok(_) => {
                println!(
                    "Secrets removed successfully for {}/{} in environment {}",
                    product_name, component_name, environment
                );
                Ok(())
            }
            Err(e) => Err(Error::Vault(format!("Failed to remove secrets: {}", e))),
        }
    }

    /// Migrates secrets from one vault to another
    pub async fn migrate(
        &self,
        product_name: &str,
        dest_vault: Arc<Mutex<dyn Vault + Send>>,
        environment: &str,
        components: &[String],
    ) -> Result<()> {
        println!(
            "Migrating secrets for {} components in environment {}",
            components.len(),
            environment
        );

        // Create new vault if it doesn't exist
        match dest_vault.lock().unwrap().create_vault(product_name).await {
            Ok(_) => (),
            Err(e) => {
                return Err(Error::Vault(format!(
                    "Failed to create destination vault: {}",
                    e
                )))
            }
        }

        let source_vault = self.vault.lock().unwrap();

        for component_name in components {
            println!(" - Migrating {}", component_name);
            match source_vault
                .get(product_name, component_name, environment)
                .await
            {
                Ok(secrets) => {
                    if !secrets.is_empty() {
                        match dest_vault
                            .lock()
                            .unwrap()
                            .set(product_name, component_name, environment, secrets)
                            .await
                        {
                            Ok(_) => (),
                            Err(e) => {
                                return Err(Error::Vault(format!(
                                "Failed to set secrets in destination vault for component {}: {}",
                                component_name, e
                            )))
                            }
                        }
                    }
                }
                Err(e) => {
                    return Err(Error::Vault(format!(
                        "Failed to get secrets from source vault for component {}: {}",
                        component_name, e
                    )))
                }
            }
        }

        println!("Migration completed successfully");
        Ok(())
    }

    /// Lists secrets in vault for a component
    pub async fn list(
        &self,
        product_name: &str,
        component_name: Option<&str>,
        environment: &str,
    ) -> Result<()> {
        match component_name {
            Some(component) => {
                // List secrets for a specific component
                match self
                    .secrets_provider
                    .get_secrets(product_name, component, &Environment::from(environment))
                    .await
                {
                    Ok(secrets) => {
                        if secrets.is_empty() {
                            println!(
                                "No secrets found for {}/{} in environment {}",
                                product_name, component, environment
                            );
                        } else {
                            println!(
                                "Secrets for {}/{} in environment {}:",
                                product_name, component, environment
                            );
                            for (key, _) in secrets {
                                println!("  - {}", key);
                            }
                        }
                        Ok(())
                    }
                    Err(e) => Err(Error::Vault(format!("Failed to list secrets: {:?}", e))),
                }
            }
            None => {
                // Future enhancement: list all components
                Err(Error::Vault("Please specify a component name".to_string()))
            }
        }
    }

    /// Executes the vault command
    pub async fn execute(&self, subcommand: &str, args: &[String]) -> Result<()> {
        let product_name = self.config.product_name();
        let environment = self.config.environment();

        match subcommand {
            "create" => self.create(product_name).await,
            "add" => {
                if args.len() < 2 {
                    return Err(Error::InvalidInput(
                        "Usage: vault add <component_name> <secrets_json>".to_string(),
                    ));
                }
                let component_name = &args[0];
                let secrets_json = &args[1];
                let secrets: HashMap<String, String> = serde_json::from_str(secrets_json)
                    .map_err(|e| Error::InvalidInput(format!("Invalid JSON format: {}", e)))?;

                self.add(product_name, component_name, environment, secrets)
                    .await
            }
            "remove" => {
                if args.is_empty() {
                    return Err(Error::InvalidInput(
                        "Usage: vault remove <component_name>".to_string(),
                    ));
                }
                let component_name = &args[0];
                self.remove(product_name, component_name, environment).await
            }
            "migrate" => {
                if args.is_empty() {
                    return Err(Error::InvalidInput(
                        "Usage: vault migrate <destination_vault>".to_string(),
                    ));
                }
                // This would need to be implemented in a way that the destination vault can be created
                // based on the arg, which would require access to the vault factory logic
                return Err(Error::InvalidInput(
                    "Migration not implemented in this context".to_string(),
                ));
            }
            "list" => {
                let component_name = args.get(0).map(|s| s.as_str());
                self.list(product_name, component_name, environment).await
            }
            _ => Err(Error::InvalidInput(format!(
                "Unknown vault subcommand: {}",
                subcommand
            ))),
        }
    }
}

/// Execute vault command using CLI context
pub async fn execute(matches: &ArgMatches, ctx: &mut CliContext) -> Result<()> {
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

async fn create_vault_cmd(ctx: &mut CliContext) -> Result<()> {
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

async fn migrate_vault(matches: &ArgMatches, _ctx: &mut CliContext) -> Result<()> {
    let dest = matches.get_one::<String>("dest").unwrap();
    trace!("Migrating secrets to: {}", dest);
    
    // Implementation would go here
    Ok(())
}

async fn add_secrets(matches: &ArgMatches, ctx: &mut CliContext) -> Result<()> {
    let component = matches.get_one::<String>("component").unwrap();
    let secrets_json = matches.get_one::<String>("secrets").unwrap();
    
    let secrets: HashMap<String, String> = serde_json::from_str(secrets_json)
        .map_err(|e| Error::InvalidInput(format!("Invalid JSON format: {}", e)))?;
    
    match ctx
        .vault
        .lock()
        .unwrap()
        .set(&ctx.product_name, component, &ctx.environment, secrets)
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

async fn remove_secrets(matches: &ArgMatches, ctx: &mut CliContext) -> Result<()> {
    let component = matches.get_one::<String>("component").unwrap();
    
    match ctx
        .vault
        .lock()
        .unwrap()
        .remove(&ctx.product_name, component, &ctx.environment)
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

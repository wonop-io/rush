use std::collections::HashMap;
use std::path::PathBuf;
use std::process;
use std::sync::{Arc, Mutex};

use clap::ArgMatches;
use log::{error, info, trace, warn};
use rush_config::Config;
use rush_core::error::{Error, Result};
use rush_security::secrets::Environment;
use rush_security::{DotenvVault, FileVault, SecretsProvider, Vault};
use serde_yaml::Value;

use crate::context::CliContext;

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
                println!("Vault created successfully for {product_name}");
                Ok(())
            }
            Err(e) => Err(Error::Vault(format!("Failed to create vault: {e}"))),
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
                    "Secrets added successfully for {product_name}/{component_name} in environment {environment}"
                );
                Ok(())
            }
            Err(e) => Err(Error::Vault(format!("Failed to add secrets: {e}"))),
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
                    "Secrets removed successfully for {product_name}/{component_name} in environment {environment}"
                );
                Ok(())
            }
            Err(e) => Err(Error::Vault(format!("Failed to remove secrets: {e}"))),
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
                    "Failed to create destination vault: {e}"
                )))
            }
        }

        let source_vault = self.vault.lock().unwrap();

        for component_name in components {
            println!(" - Migrating {component_name}");
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
                                "Failed to set secrets in destination vault for component {component_name}: {e}"
                            )))
                            }
                        }
                    }
                }
                Err(e) => {
                    return Err(Error::Vault(format!(
                    "Failed to get secrets from source vault for component {component_name}: {e}"
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
                                "No secrets found for {product_name}/{component} in environment {environment}"
                            );
                        } else {
                            println!(
                                "Secrets for {product_name}/{component} in environment {environment}:"
                            );
                            for (key, _) in secrets {
                                println!("  - {key}");
                            }
                        }
                        Ok(())
                    }
                    Err(e) => Err(Error::Vault(format!("Failed to list secrets: {e:?}"))),
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
                    .map_err(|e| Error::InvalidInput(format!("Invalid JSON format: {e}")))?;

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
                Err(Error::InvalidInput(
                    "Migration not implemented in this context".to_string(),
                ))
            }
            "list" => {
                let component_name = args.first().map(|s| s.as_str());
                self.list(product_name, component_name, environment).await
            }
            _ => Err(Error::InvalidInput(format!(
                "Unknown vault subcommand: {subcommand}"
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
            error!("Failed to create vault: {e}");
            eprintln!("{e}");
            process::exit(1);
        }
    }
}

async fn migrate_vault(matches: &ArgMatches, ctx: &mut CliContext) -> Result<()> {
    let dest = matches.get_one::<String>("dest").unwrap();
    let from_override = matches.get_one::<String>("from");

    info!("Migrating secrets to: {dest}");

    // Parse destination vault
    let (dest_vault, dest_description) = parse_vault_spec(dest, &ctx.config)?;

    // Determine source vault
    let (source_vault, source_description) = if let Some(from) = from_override {
        parse_vault_spec(from, &ctx.config)?
    } else {
        // Use current vault as source
        (ctx.vault.clone(), format!("current ({})", ctx.config.vault_name()))
    };

    // Get component names from stack.spec.yaml
    let components = get_component_names(&ctx.config)?;

    println!("Migrating secrets:");
    println!("  From: {source_description}");
    println!("  To:   {dest_description}");
    println!("  Components: {}", components.len());
    println!();

    // Create destination vault if needed
    match dest_vault.lock().unwrap().create_vault(&ctx.product_name).await {
        Ok(_) => (),
        Err(e) => {
            return Err(Error::Vault(format!(
                "Failed to create destination vault: {e}"
            )))
        }
    }

    let mut migrated_count = 0;
    let mut skipped_count = 0;

    for component_name in &components {
        trace!("Processing component: {component_name}");

        // Get secrets from source
        let secrets = match source_vault
            .lock()
            .unwrap()
            .get(&ctx.product_name, component_name, &ctx.environment)
            .await
        {
            Ok(s) => s,
            Err(e) => {
                warn!("  {component_name}: failed to read ({e}), skipping");
                skipped_count += 1;
                continue;
            }
        };

        if secrets.is_empty() {
            println!("  {component_name}: no secrets, skipping");
            skipped_count += 1;
            continue;
        }

        // Set secrets in destination
        match dest_vault
            .lock()
            .unwrap()
            .set(&ctx.product_name, component_name, &ctx.environment, secrets.clone())
            .await
        {
            Ok(_) => {
                println!("  {component_name}: migrated {} secrets", secrets.len());
                migrated_count += 1;
            }
            Err(e) => {
                error!("  {component_name}: failed to write ({e})");
                return Err(Error::Vault(format!(
                    "Failed to migrate secrets for {component_name}: {e}"
                )));
            }
        }
    }

    println!();
    println!("Migration complete: {migrated_count} components migrated, {skipped_count} skipped");
    Ok(())
}

/// Parse a vault specification string into a Vault instance
fn parse_vault_spec(spec: &str, config: &Config) -> Result<(Arc<Mutex<dyn Vault + Send>>, String)> {
    if spec.starts_with("json:") {
        let path = spec.strip_prefix("json:").unwrap();
        // Expand ~ to home directory
        let expanded_path = if path.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(&path[2..])
            } else {
                return Err(Error::Config("Could not determine home directory".to_string()));
            }
        } else {
            PathBuf::from(path)
        };
        
        let vault = Arc::new(Mutex::new(FileVault::new(expanded_path.clone(), None))) 
            as Arc<Mutex<dyn Vault + Send>>;
        Ok((vault, format!("json:{}", expanded_path.display())))
    } else if spec == ".env" {
        let product_path = PathBuf::from(config.product_path());
        let vault = Arc::new(Mutex::new(DotenvVault::new(product_path))) 
            as Arc<Mutex<dyn Vault + Send>>;
        Ok((vault, ".env (component directories)".to_string()))
    } else {
        Err(Error::InvalidInput(format!(
            "Unknown vault specification: '{}'. Use 'json:/path' or '.env'",
            spec
        )))
    }
}

/// Get component names from stack.spec.yaml
fn get_component_names(config: &Config) -> Result<Vec<String>> {
    let stack_yaml_path = config.product_path().join("stack.spec.yaml");
    
    let stack_yaml_content = std::fs::read_to_string(&stack_yaml_path)
        .map_err(|e| Error::Config(format!("Failed to read stack.spec.yaml: {e}")))?;
    
    let stack_yaml: Value = serde_yaml::from_str(&stack_yaml_content)
        .map_err(|e| Error::Config(format!("Failed to parse stack.spec.yaml: {e}")))?;

    let mut components = Vec::new();
    if let Some(mapping) = stack_yaml.as_mapping() {
        for (key, _) in mapping {
            if let Some(name) = key.as_str() {
                components.push(name.to_string());
            }
        }
    }

    if components.is_empty() {
        return Err(Error::Config("No components found in stack.spec.yaml".to_string()));
    }

    Ok(components)
}

async fn add_secrets(matches: &ArgMatches, ctx: &mut CliContext) -> Result<()> {
    let component = matches.get_one::<String>("component").unwrap();
    let secrets_json = matches.get_one::<String>("secrets").unwrap();

    let secrets: HashMap<String, String> = serde_json::from_str(secrets_json)
        .map_err(|e| Error::InvalidInput(format!("Invalid JSON format: {e}")))?;

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
            error!("Failed to add secrets: {e}");
            eprintln!("{e}");
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
            error!("Failed to remove secrets: {e}");
            eprintln!("{e}");
            process::exit(1);
        }
    }
}

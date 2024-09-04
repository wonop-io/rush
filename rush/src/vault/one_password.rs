use crate::vault::Vault;
use async_trait::async_trait;
use log::{debug, error, trace};
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::process::Command;

pub struct OnePassword;

impl OnePassword {
    pub fn new() -> Self {
        trace!("Creating new OnePassword instance");
        OnePassword
    }

    fn run_op_command(&self, args: Vec<String>) -> Result<String, Box<dyn Error>> {
        debug!("Running 1Password CLI command with args: {:?}", args);
        let output = Command::new("op").args(&args).output()?;

        if output.status.success() {
            let stdout = String::from_utf8(output.stdout)?;
            debug!("1Password CLI command executed successfully");
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            debug!("1Password CLI command failed: {}", stderr);
            Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                stderr,
            )))
        }
    }
}

#[async_trait]
impl Vault for OnePassword {
    async fn get(
        &self,
        product_name: &str,
        component_name: &str,
        environment: &str,
    ) -> Result<HashMap<String, String>, Box<dyn Error>> {
        trace!(
            "Getting secrets for {}-{} in vault {}",
            component_name,
            environment,
            product_name
        );
        let item_name = format!("{}-{}", component_name, environment);
        let output = self.run_op_command(
            [
                "item",
                "get",
                &item_name,
                "--vault",
                product_name,
                "--format",
                "json",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        )?;

        let json: Value = serde_json::from_str(&output)?;
        let fields = json["fields"].as_array().ok_or("Invalid JSON structure")?;

        let mut secrets = HashMap::new();
        for field in fields {
            if let (Some(label), Some(value)) = (field["label"].as_str(), field["value"].as_str()) {
                secrets.insert(label.to_string(), value.to_string());
                debug!("Retrieved secret: {}", label);
            }
        }

        trace!("Successfully retrieved {} secrets", secrets.len());
        Ok(secrets)
    }

    async fn set(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &str,
        secrets: HashMap<String, String>,
    ) -> Result<(), Box<dyn Error>> {
        trace!(
            "Setting secrets for {}-{} in vault {}",
            component_name,
            environment,
            product_name
        );
        let item_name = format!("{}-{}", component_name, environment);

        // Check if the item already exists
        let list_output = self.run_op_command(
            ["item", "list", "--vault", product_name, "--format", "json"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
        )?;
        let items: Vec<Value> = serde_json::from_str(&list_output)?;

        let item_exists = items
            .iter()
            .any(|item| item["title"].as_str() == Some(&item_name));

        let mut args = vec!["item".to_string()];
        if item_exists {
            debug!("Item {} already exists, updating", item_name);
            args.push("edit".to_string());
            args.push(item_name.clone());
        } else {
            debug!("Item {} does not exist, creating new", item_name);
            args.push("create".to_string());
            args.push("--title".to_string());
            args.push(item_name.clone());
            args.push("--category".to_string());
            args.push("Secure Note".to_string());
        }
        args.push("--vault".to_string());
        args.push(product_name.to_string());

        for (key, value) in &secrets {
            args.push(format!("{}={}", key, value));
            debug!("Adding secret: {}", key);
        }

        let output = self.run_op_command(args)?;

        trace!(
            "Successfully {} item {}",
            if item_exists { "updated" } else { "created" },
            item_name
        );
        Ok(())
    }

    async fn create_vault(&mut self, product_name: &str) -> Result<(), Box<dyn Error>> {
        trace!("Checking if vault exists: {}", product_name);
        let list_args = vec![
            "vault".to_string(),
            "list".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        let list_output = self.run_op_command(list_args)?;
        let vaults: Vec<Value> = serde_json::from_str(&list_output)?;

        if vaults
            .iter()
            .any(|vault| vault["name"].as_str() == Some(product_name))
        {
            trace!("Vault '{}' already exists", product_name);
            return Ok(());
        }

        trace!("Creating vault: {}", product_name);
        let create_args = vec![
            "vault".to_string(),
            "create".to_string(),
            product_name.to_string(),
        ];
        let create_output = self.run_op_command(create_args)?;

        trace!("Successfully created vault: {}", product_name);
        Ok(())
    }

    async fn remove(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &str,
    ) -> Result<(), Box<dyn Error>> {
        trace!(
            "Removing secrets for {}-{} in vault {}",
            component_name,
            environment,
            product_name
        );
        let item_name = format!("{}-{}", component_name, environment);

        let args = vec![
            "item".to_string(),
            "delete".to_string(),
            item_name.clone(),
            "--vault".to_string(),
            product_name.to_string(),
        ];
        let output = self.run_op_command(args)?;

        trace!("Successfully removed item {}", item_name);
        Ok(())
    }

    async fn check_if_vault_exists(&self, product_name: &str) -> Result<bool, Box<dyn Error>> {
        trace!("Checking if vault exists: {}", product_name);
        let list_args = vec![
            "vault".to_string(),
            "list".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        let list_output = self.run_op_command(list_args)?;
        let vaults: Vec<Value> = serde_json::from_str(&list_output)?;

        let exists = vaults
            .iter()
            .any(|vault| vault["name"].as_str() == Some(product_name));
        trace!("Vault '{}' exists: {}", product_name, exists);
        Ok(exists)
    }
}

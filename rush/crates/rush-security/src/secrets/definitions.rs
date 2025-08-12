use crate::security::Vault;
use base64::prelude::*;
use chrono::Utc;
use log::{debug, trace, warn};
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// SecretsDefinitions manages the definition and generation of secrets for components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsDefinitions {
    /// The name of the product these secrets are for
    product_name: String,
    /// Component-level secret definitions
    components: HashMap<String, ComponentSecrets>,
}

/// Defines secrets for a specific component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSecrets {
    /// Map of secret names to their generation methods
    secrets: HashMap<String, GenerationMethod>,
}

/// Methods for generating secret values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GenerationMethod {
    /// A static, predefined value
    Static(String),
    /// A static value that will be Base64 encoded
    Base64EncodedStatic(String),
    /// Prompt the user to enter a value
    Ask(String),
    /// Prompt with a default value
    AskWithDefault(String, String),
    /// Prompt for a password (hidden input)
    AskPassword(String),
    /// Generate a random string of specified length
    RandomString(usize),
    /// Generate a random alphanumeric string of specified length
    RandomAlphanumeric(usize),
    /// Generate a random hex string of specified length
    RandomHex(usize),
    /// Generate a random Base64-encoded string of specified length
    RandomBase64(usize),
    /// Generate a random UUID
    RandomUUID,
    /// Use the current timestamp
    Timestamp,
    /// Read from a file with options: (ask for path, base64 encode, default path)
    FromFile(bool, bool, String),
    /// Reference another secret (format: "component.secretname")
    Ref(String),
}

/// Result of a secret generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GenerationResult {
    /// A single value was generated
    Value(String),
    /// A reference to another secret
    Ref(String, String),
    /// No value was generated
    None,
}

impl SecretsDefinitions {
    /// Create a new SecretsDefinitions from a YAML file
    pub fn new(product_name: String, yaml_filename: &str) -> Self {
        trace!("Loading secret definitions from {}", yaml_filename);

        let components = match File::open(yaml_filename) {
            Ok(mut file) => {
                let mut contents = String::new();
                match file.read_to_string(&mut contents) {
                    Ok(_) => match serde_yaml::from_str(&contents) {
                        Ok(parsed_components) => parsed_components,
                        Err(e) => {
                            warn!("Unable to parse YAML file: {}. Using empty definition.", e);
                            HashMap::new()
                        }
                    },
                    Err(e) => {
                        warn!("Unable to read YAML file: {}. Using empty definition.", e);
                        HashMap::new()
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Unable to open YAML file '{}': {}. Using empty definition.",
                    yaml_filename, e
                );
                HashMap::new()
            }
        };

        let components = components
            .into_iter()
            .map(|(component_name, secrets)| (component_name, ComponentSecrets { secrets }))
            .collect();

        debug!("Loaded secret definitions for product: {}", product_name);
        Self {
            product_name,
            components,
        }
    }

    /// Add a new component to the definitions
    pub fn add_component(&mut self, component_name: String) {
        debug!("Adding component: {}", component_name);
        self.components.insert(
            component_name,
            ComponentSecrets {
                secrets: HashMap::new(),
            },
        );
    }

    /// Add a secret to a component
    pub fn add_secret(
        &mut self,
        component_name: &str,
        secret_name: String,
        generation_method: GenerationMethod,
    ) {
        if let Some(component) = self.components.get_mut(component_name) {
            debug!(
                "Adding secret '{}' to component '{}'",
                secret_name, component_name
            );
            component.secrets.insert(secret_name, generation_method);
        } else {
            warn!("Component '{}' not found", component_name);
        }
    }

    /// Validate that all required secrets are present in the vault
    pub async fn validate_vault(
        &self,
        vault: Arc<Mutex<dyn Vault + Send>>,
        env: &str,
    ) -> Result<bool, Box<dyn Error>> {
        debug!("Validating vault for environment: {}", env);
        let mut all_valid = true;

        for (component_name, component) in &self.components {
            trace!("Validating component: {}", component_name);

            let vault_secrets = vault
                .lock()
                .unwrap()
                .get(&self.product_name, component_name, env)
                .await?;

            for secret_name in component.secrets.keys() {
                match &component.secrets[secret_name] {
                    GenerationMethod::Ref(path) => {
                        let parts: Vec<&str> = path.split('.').collect();
                        if parts.len() != 2 {
                            warn!(
                                "Invalid reference format for '{}' in component '{}'",
                                secret_name, component_name
                            );
                            all_valid = false;
                            continue;
                        }

                        let ref_component = parts[0];
                        let ref_secret = parts[1];

                        let ref_secrets = vault
                            .lock()
                            .unwrap()
                            .get(&self.product_name, ref_component, env)
                            .await?;

                        if !ref_secrets.contains_key(ref_secret) {
                            warn!(
                                "Referenced secret '{}' not found in component '{}'",
                                ref_secret, ref_component
                            );
                            all_valid = false;
                        }
                    }
                    _ => {
                        if !vault_secrets.contains_key(secret_name) {
                            warn!(
                                "Missing secret '{}' in component '{}'",
                                secret_name, component_name
                            );
                            all_valid = false;
                        }
                    }
                }
            }
        }

        debug!("Vault validation result: {}", all_valid);
        Ok(all_valid)
    }

    /// Check if a secret is a reference to another secret
    fn is_reference(&self, component_name: &str, secret_name: &str) -> bool {
        if let Some(component) = self.components.get(component_name) {
            if let Some(generation_method) = component.secrets.get(secret_name) {
                matches!(generation_method, GenerationMethod::Ref(_))
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Generate a secret value based on its definition
    pub fn generate_secret(&self, component_name: &str, secret_name: &str) -> GenerationResult {
        trace!(
            "Generating secret '{}' for component '{}'",
            secret_name,
            component_name
        );

        if let Some(component) = self.components.get(component_name) {
            if let Some(generation_method) = component.secrets.get(secret_name) {
                match generation_method {
                    GenerationMethod::Static(value) => {
                        trace!("Using static value");
                        GenerationResult::Value(value.clone())
                    }
                    GenerationMethod::Base64EncodedStatic(value) => {
                        trace!("Using base64 encoded static value");
                        GenerationResult::Value(BASE64_STANDARD.encode(value))
                    }
                    GenerationMethod::Ask(prompt) => {
                        println!("\n{prompt}: ");
                        let mut input = String::new();
                        std::io::stdin().read_line(&mut input).unwrap();
                        GenerationResult::Value(input.trim().to_string())
                    }
                    GenerationMethod::AskWithDefault(prompt, default) => {
                        println!("\n{prompt} [default: {default}]: ");
                        let mut input = String::new();
                        std::io::stdin().read_line(&mut input).unwrap();
                        let value = if input.trim().is_empty() {
                            default.clone()
                        } else {
                            input.trim().to_string()
                        };
                        GenerationResult::Value(value)
                    }
                    GenerationMethod::AskPassword(prompt) => {
                        println!("\n{prompt}: ");
                        let password = rpassword::prompt_password("").unwrap();
                        GenerationResult::Value(password)
                    }
                    GenerationMethod::RandomString(length) => {
                        let random_string: String = rand::thread_rng()
                            .sample_iter(&Alphanumeric)
                            .take(*length)
                            .map(char::from)
                            .collect();
                        GenerationResult::Value(random_string)
                    }
                    GenerationMethod::RandomAlphanumeric(length) => {
                        let random_string: String = rand::thread_rng()
                            .sample_iter(&Alphanumeric)
                            .take(*length)
                            .map(char::from)
                            .collect();
                        GenerationResult::Value(random_string)
                    }
                    GenerationMethod::RandomHex(length) => {
                        let random_bytes: Vec<u8> =
                            (0..*length).map(|_| rand::random::<u8>()).collect();
                        GenerationResult::Value(hex::encode(random_bytes))
                    }
                    GenerationMethod::RandomBase64(length) => {
                        let random_bytes: Vec<u8> =
                            (0..*length).map(|_| rand::random::<u8>()).collect();
                        GenerationResult::Value(BASE64_STANDARD.encode(random_bytes))
                    }
                    GenerationMethod::RandomUUID => {
                        GenerationResult::Value(Uuid::new_v4().to_string())
                    }
                    GenerationMethod::Timestamp => GenerationResult::Value(Utc::now().to_rfc3339()),
                    GenerationMethod::FromFile(should_ask, encode_base64, default_path) => {
                        let file_path = if *should_ask {
                            println!("\nEnter file path [default: {default_path}]: ");
                            let mut input = String::new();
                            std::io::stdin().read_line(&mut input).unwrap();
                            if input.trim().is_empty() {
                                default_path.clone()
                            } else {
                                input.trim().to_string()
                            }
                        } else {
                            default_path.clone()
                        };

                        // Expand home directory if path starts with ~
                        let expanded_path = if file_path.starts_with("~/") {
                            if let Some(home_dir) = dirs::home_dir() {
                                home_dir
                                    .join(&file_path[2..])
                                    .to_string_lossy()
                                    .into_owned()
                            } else {
                                warn!("Could not find home directory");
                                file_path
                            }
                        } else {
                            file_path
                        };

                        match File::open(&expanded_path) {
                            Ok(mut file) => {
                                let mut contents = Vec::new();
                                if let Err(e) = file.read_to_end(&mut contents) {
                                    warn!("Failed to read file contents: {}", e);
                                    return GenerationResult::None;
                                }

                                if *encode_base64 {
                                    GenerationResult::Value(BASE64_STANDARD.encode(contents))
                                } else {
                                    match String::from_utf8(contents.clone()) {
                                        Ok(text) => GenerationResult::Value(text),
                                        Err(_) => {
                                            warn!("File contains non-UTF8 data, using base64 encoding");
                                            GenerationResult::Value(
                                                BASE64_STANDARD.encode(contents),
                                            )
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to open file '{}': {}", expanded_path, e);
                                GenerationResult::None
                            }
                        }
                    }
                    GenerationMethod::Ref(path) => {
                        let path: Vec<&str> = path.split('.').collect();
                        if path.len() != 2 {
                            warn!("Invalid reference format: {}", path.join("."));
                            return GenerationResult::None;
                        }
                        let component = path[0].to_string();
                        let secret = path[1].to_string();
                        GenerationResult::Ref(component, secret)
                    }
                }
            } else {
                warn!(
                    "Secret '{}' not found in component '{}'",
                    secret_name, component_name
                );
                GenerationResult::None
            }
        } else {
            warn!("Component '{}' not found", component_name);
            GenerationResult::None
        }
    }

    /// Populate the vault with all defined secrets
    pub async fn populate(
        &self,
        vault: Arc<Mutex<dyn Vault + Send>>,
        env: &str,
    ) -> Result<(), Box<dyn Error>> {
        debug!("Populating vault for environment: {}", env);

        // Define the helper struct within this function to avoid name conflicts
        struct InnerSecretStore {
            components: HashMap<String, HashMap<String, String>>,
            references: HashMap<String, Vec<(String, String)>>,
        }

        let mut store = InnerSecretStore {
            components: HashMap::new(),
            references: HashMap::new(),
        };

        // Sort components for consistent processing
        let mut sorted_components: Vec<_> = self.components.keys().collect();
        sorted_components.sort();

        // Get existing secrets to check for overrides
        let mut existing_secrets = HashMap::new();
        for component_name in &sorted_components {
            match vault
                .lock()
                .unwrap()
                .get(&self.product_name, component_name, env)
                .await
            {
                Ok(secrets) => {
                    existing_secrets.insert(component_name.to_string(), secrets);
                }
                Err(_) => {
                    existing_secrets.insert(component_name.to_string(), HashMap::new());
                }
            }
        }

        // Process each component
        for component_name in sorted_components {
            let existing_component_secrets = existing_secrets.get(component_name).unwrap();
            let component_secrets = &self.components[component_name];

            trace!("Processing secrets for component: {}", component_name);
            println!("\n{component_name}");
            println!("{}", "=".repeat(component_name.len()));

            // Process each secret in the component
            let mut sorted_secrets: Vec<_> = component_secrets.secrets.keys().collect();
            sorted_secrets.sort();

            for secret_name in sorted_secrets {
                let should_generate_new = if self.is_reference(component_name, secret_name) {
                    true
                } else if let Some(existing_value) = existing_component_secrets.get(secret_name) {
                    // Ask for override if secret already exists
                    let mut input = String::new();
                    let truncated_value = if existing_value.len() > 10 {
                        format!(
                            "{}****{}",
                            &existing_value[..3],
                            &existing_value[existing_value.len() - 3..]
                        )
                    } else {
                        "****".to_string()
                    };

                    println!(
                        "Secret '{secret_name}' [{truncated_value}] already exists. Override? (y/N)"
                    );
                    std::io::stdout().flush().unwrap();
                    std::io::stdin().read_line(&mut input).unwrap();

                    let override_secret = input.trim().eq_ignore_ascii_case("y");
                    if !override_secret {
                        // Keep existing value
                        store
                            .components
                            .entry(component_name.to_string())
                            .or_default()
                            .insert(secret_name.clone(), existing_value.clone());
                    }

                    override_secret
                } else {
                    true
                };

                if !should_generate_new {
                    continue;
                }

                let secret_value = self.generate_secret(component_name, secret_name);

                match secret_value {
                    GenerationResult::Value(value) => {
                        store
                            .components
                            .entry(component_name.to_string())
                            .or_default()
                            .insert(secret_name.clone(), value);
                    }
                    GenerationResult::Ref(ref_component, ref_secret) => {
                        store
                            .references
                            .entry(component_name.to_string())
                            .or_default()
                            .push((
                                secret_name.clone(),
                                format!("{ref_component}.{ref_secret}"),
                            ));
                    }
                    GenerationResult::None => {
                        warn!(
                            "Failed to generate value for '{}' in component '{}'",
                            secret_name, component_name
                        );
                    }
                }
            }
        }

        // Resolve references internally rather than calling another method
        debug!("Resolving secret references");

        for (component_name, references) in &store.references {
            for (secret_name, ref_path) in references {
                let parts: Vec<&str> = ref_path.split('.').collect();
                if parts.len() != 2 {
                    warn!("Invalid reference format: {}", ref_path);
                    continue;
                }

                let ref_component = parts[0];
                let ref_secret = parts[1];

                // Create a copy of the reference value before mutating the HashMap
                let ref_value_option = store
                    .components
                    .get(ref_component)
                    .and_then(|c| c.get(ref_secret).cloned());

                if let Some(ref_value) = ref_value_option {
                    store
                        .components
                        .entry(component_name.clone())
                        .or_default()
                        .insert(secret_name.clone(), ref_value);

                    trace!(
                        "Resolved reference: {}.{} -> {}.{}",
                        component_name,
                        secret_name,
                        ref_component,
                        ref_secret
                    );
                } else {
                    warn!(
                        "Referenced secret '{}' not found in component '{}'",
                        ref_secret, ref_component
                    );
                }
            }
        }

        // Save all secrets to vault
        for (component_name, secrets) in &store.components {
            debug!(
                "Saving {} secrets for component '{}'",
                secrets.len(),
                component_name
            );

            println!("Saving secrets for {component_name}");
            for secret_name in secrets.keys() {
                println!("  {secret_name}: ***");
            }

            vault
                .lock()
                .unwrap()
                .set(&self.product_name, component_name, env, secrets.clone())
                .await?;
        }

        debug!("Vault population completed successfully");
        Ok(())
    }
}

// Helper type for tracking secrets and references
#[derive(Debug, Clone)]
struct SecretStore {
    components: HashMap<String, HashMap<String, String>>,
    references: HashMap<String, Vec<(String, String)>>,
}

use crate::vault::Vault;
use base64;
use chrono::Utc;
use colored::Colorize;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use log::{trace, warn};
use openssl::ec::{EcGroup, EcKey};
use openssl::nid::Nid;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;
use rand::{distributions::Alphanumeric, Rng, RngCore};
use serde::{Deserialize, Serialize};
use ssh_key::{self, private::KeypairData, PrivateKey, PublicKey};
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsDefinitions {
    product_name: String,
    components: HashMap<String, ComponentSecrets>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSecrets {
    secrets: HashMap<String, GenerationMethod>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GenerationMethod {
    Static(String),
    Base64EncodedStatic(String), // Added Base64 encoded static string
    Ask(String),
    AskWithDefault(String, String), // Added AskWithDefault with prompt and default value
    AskPassword(String),            // Added AskPassword with prompt
    RandomString(usize),
    RandomAlphanumeric(usize),
    RandomHex(usize),
    RandomBase64(usize),
    RandomUUID,
    Timestamp,
    FromFile(bool, bool, String), // (whether to ask, encode base64, default_file_path)
    Ref(String),
    OpenSshRSAKeyPair(usize, bool), // Added bool to specify base64 encoding
    OpenSshECDSAKeyPair(String, bool), // Added bool to specify base64 encoding
    OpenSshEd25519KeyPair(bool),    // Added bool to specify base64 encoding
    AESKey(usize, bool),            // Added bool to specify base64 encoding
    HMACKey(usize, bool),           // Added bool to specify base64 encoding
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GenerationResult {
    Value(String),
    KeyPair(String, String),
    Ref(String, String),
    None,
}

impl SecretsDefinitions {
    pub fn new(product_name: String, yaml_filename: &str) -> Self {
        let components = match File::open(yaml_filename) {
            Ok(mut file) => {
                let mut contents = String::new();
                match file.read_to_string(&mut contents) {
                    Ok(_) => match serde_yaml::from_str(&contents) {
                        Ok(parsed_components) => parsed_components,
                        Err(e) => {
                            panic!(
                                "Unable to parse YAML file: {}. Returning empty definition.",
                                e
                            );
                        }
                    },
                    Err(e) => {
                        panic!(
                            "Unable to read YAML file: {}. Returning empty definition.",
                            e
                        );
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Unable to open YAML file '{}': {}. Returning empty definition.",
                    yaml_filename, e
                );
                HashMap::new()
            }
        };

        let components = components
            .into_iter()
            .map(|(component_name, secrets)| (component_name, ComponentSecrets { secrets }))
            .collect();

        Self {
            product_name,
            components,
        }
    }

    pub fn add_component(&mut self, component_name: String) {
        self.components.insert(
            component_name,
            ComponentSecrets {
                secrets: HashMap::new(),
            },
        );
    }

    pub fn add_secret(
        &mut self,
        component_name: &str,
        secret_name: String,
        generation_method: GenerationMethod,
    ) {
        if let Some(component) = self.components.get_mut(component_name) {
            component.secrets.insert(secret_name, generation_method);
        } else {
            panic!("Component {} not found", component_name);
        }
    }

    pub async fn validate_vault(
        &self,
        vault: Arc<Mutex<dyn Vault + Send>>,
        env: &str,
    ) -> Result<bool, Box<dyn Error>> {
        let mut all_valid = true;

        for (component_name, component) in &self.components {
            let vault_secrets = vault
                .lock()
                .unwrap()
                .get(&self.product_name, component_name, env)
                .await?;

            for secret_name in component.secrets.keys() {
                match &component.secrets[secret_name] {
                    GenerationMethod::OpenSshRSAKeyPair(_, _)
                    | GenerationMethod::OpenSshECDSAKeyPair(_, _)
                    | GenerationMethod::OpenSshEd25519KeyPair(_) => {
                        let private_key = format!("{}_PRIVATE_KEY", secret_name);
                        let public_key = format!("{}_PUBLIC_KEY", secret_name);

                        if !vault_secrets.contains_key(&private_key)
                            || !vault_secrets.contains_key(&public_key)
                        {
                            println!(
                                "Missing key pair for {} in component {}",
                                secret_name, component_name
                            );
                            all_valid = false;
                        }
                    }
                    GenerationMethod::Ref(path) => {
                        let parts: Vec<&str> = path.split('.').collect();
                        if parts.len() != 2 {
                            println!(
                                "Invalid reference format for {} in component {}",
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
                            println!(
                                "Referenced secret {} not found in component {}",
                                ref_secret, ref_component
                            );
                            all_valid = false;
                        }
                    }
                    _ => {
                        if !vault_secrets.contains_key(secret_name) {
                            println!(
                                "Missing secret {} in component {}",
                                secret_name, component_name
                            );
                            all_valid = false;
                        }
                    }
                }
            }
        }

        Ok(all_valid)
    }

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

    pub fn generate_secret(&self, component_name: &str, secret_name: &str) -> GenerationResult {
        if let Some(component) = self.components.get(component_name) {
            if let Some(generation_method) = component.secrets.get(secret_name) {
                match generation_method {
                    GenerationMethod::Static(value) => GenerationResult::Value(value.clone()),
                    GenerationMethod::Base64EncodedStatic(value) => {
                        GenerationResult::Value(base64::encode(value))
                    }
                    GenerationMethod::Ask(prompt) => {
                        let prompt = format!("{} ", format!("\n{}:", prompt).white().bold());
                        let mut input = String::new();
                        print!("{}", prompt);
                        std::io::stdout().flush().unwrap();
                        std::io::stdin().read_line(&mut input).unwrap();
                        GenerationResult::Value(input.trim().to_string())
                    }
                    GenerationMethod::AskWithDefault(prompt, default) => {
                        let prompt = format!(
                            "{} ",
                            format!("\n{} [default: {}]:", prompt, default)
                                .white()
                                .bold()
                        );
                        let mut input = String::new();
                        print!("{}", prompt);
                        std::io::stdout().flush().unwrap();
                        std::io::stdin().read_line(&mut input).unwrap();
                        let value = if input.trim().is_empty() {
                            default.clone()
                        } else {
                            input.trim().to_string()
                        };
                        GenerationResult::Value(value)
                    }
                    GenerationMethod::AskPassword(prompt) => {
                        let prompt = format!("{} ", format!("\n{}:", prompt).white().bold());
                        let password = rpassword::prompt_password(&prompt).unwrap();
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
                        GenerationResult::Value(base64::encode(random_bytes))
                    }
                    GenerationMethod::RandomUUID => {
                        GenerationResult::Value(Uuid::new_v4().to_string())
                    }
                    GenerationMethod::Timestamp => GenerationResult::Value(Utc::now().to_rfc3339()),
                    GenerationMethod::FromFile(should_ask, encode_base64, default_path) => {
                        let file_path = if *should_ask {
                            let prompt = format!(
                                "{} ",
                                format!("\nEnter file path [default: {}]:", default_path)
                                    .white()
                                    .bold()
                            );
                            let mut input = String::new();
                            print!("{}", prompt);
                            std::io::stdout().flush().unwrap();
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
                            dirs::home_dir()
                                .expect("Could not find home directory")
                                .join(&file_path[2..])
                                .to_string_lossy()
                                .into_owned()
                        } else {
                            file_path
                        };

                        let mut file = File::open(&expanded_path).expect("Failed to open file");
                        let mut contents = Vec::new();
                        file.read_to_end(&mut contents)
                            .expect("Failed to read file contents");

                        if *encode_base64 {
                            GenerationResult::Value(base64::encode(contents))
                        } else {
                            GenerationResult::Value(String::from_utf8_lossy(&contents).to_string())
                        }
                    }
                    GenerationMethod::Ref(path) => {
                        let path: Vec<&str> = path.split('.').collect();
                        let component = path[0].to_string();
                        let secret = path[1..].join(".");
                        GenerationResult::Ref(component, secret)
                    }
                    GenerationMethod::OpenSshRSAKeyPair(bits, _) => {
                        let mut rng = rand::thread_rng();
                        let key =
                            PrivateKey::random(&mut rng, ssh_key::Algorithm::Rsa { hash: None })
                                .unwrap();

                        GenerationResult::KeyPair(
                            key.to_openssh(ssh_key::LineEnding::LF).unwrap().to_string(),
                            format!(
                                "{} {}",
                                key.public_key().to_openssh().unwrap(),
                                env::var("USER").unwrap_or_else(|_| "user".to_string())
                            ),
                        )
                    }
                    GenerationMethod::OpenSshECDSAKeyPair(curve, _) => {
                        let mut rng = rand::thread_rng();
                        let key = PrivateKey::random(
                            &mut rng,
                            ssh_key::Algorithm::Ecdsa {
                                curve: ssh_key::EcdsaCurve::NistP256,
                            },
                        )
                        .unwrap();

                        GenerationResult::KeyPair(
                            key.to_openssh(ssh_key::LineEnding::LF).unwrap().to_string(),
                            format!(
                                "{} {}",
                                key.public_key().to_openssh().unwrap(),
                                env::var("USER").unwrap_or_else(|_| "user".to_string())
                            ),
                        )
                    }
                    GenerationMethod::OpenSshEd25519KeyPair(_) => {
                        let mut rng = rand::thread_rng();
                        let key =
                            PrivateKey::random(&mut rng, ssh_key::Algorithm::Ed25519).unwrap();

                        GenerationResult::KeyPair(
                            key.to_openssh(ssh_key::LineEnding::LF).unwrap().to_string(),
                            format!(
                                "{} {}",
                                key.public_key().to_openssh().unwrap(),
                                env::var("USER").unwrap_or_else(|_| "user".to_string())
                            ),
                        )
                    }
                    GenerationMethod::AESKey(bits, base64_encode) => {
                        let key: Vec<u8> = (0..bits / 8).map(|_| rand::random::<u8>()).collect();
                        if *base64_encode {
                            GenerationResult::Value(base64::encode(key))
                        } else {
                            GenerationResult::Value(hex::encode(key))
                        }
                    }
                    GenerationMethod::HMACKey(bits, base64_encode) => {
                        let key: Vec<u8> = (0..bits / 8).map(|_| rand::random::<u8>()).collect();
                        if *base64_encode {
                            GenerationResult::Value(base64::encode(key))
                        } else {
                            GenerationResult::Value(hex::encode(key))
                        }
                    }
                }
            } else {
                GenerationResult::None
            }
        } else {
            GenerationResult::None
        }
    }
}

#[derive(Debug, Clone)]
struct SecretReference {
    secret_name: String,
    component: String,
    referenced_secret: String,
}

#[derive(Debug, Clone)]
struct ComponentSecretSet {
    secrets: HashMap<String, String>,
    references: Vec<SecretReference>,
}

#[derive(Debug, Clone)]
struct SecretStore {
    pub components: HashMap<String, ComponentSecretSet>,
}

impl SecretStore {
    fn new() -> Self {
        Self {
            components: HashMap::new(),
        }
    }

    fn add_secret(&mut self, component: &str, name: String, value: String) {
        self.components
            .entry(component.to_string())
            .or_insert_with(|| ComponentSecretSet {
                secrets: HashMap::new(),
                references: Vec::new(),
            })
            .secrets
            .insert(name, value);
    }

    fn add_reference(
        &mut self,
        component: &str,
        name: String,
        ref_component: String,
        ref_secret: String,
    ) {
        self.components
            .entry(component.to_string())
            .or_insert_with(|| ComponentSecretSet {
                secrets: HashMap::new(),
                references: Vec::new(),
            })
            .references
            .push(SecretReference {
                secret_name: name,
                component: ref_component,
                referenced_secret: ref_secret,
            });
    }

    fn resolve_references(&mut self) {
        let components = self.components.clone();

        for (component_name, component_set) in &mut self.components {
            for reference in &component_set.references {
                if let Some(ref_component) = components.get(&reference.component) {
                    if let Some(ref_value) = ref_component.secrets.get(&reference.referenced_secret)
                    {
                        component_set
                            .secrets
                            .insert(reference.secret_name.clone(), ref_value.clone());
                    }
                }
            }
        }
    }
}

impl SecretsDefinitions {
    pub async fn populate(
        &self,
        vault: Arc<Mutex<dyn Vault + Send>>,
        env: &str,
    ) -> Result<(), Box<dyn Error>> {
        let mut store = SecretStore::new();

        let mut sorted_components: Vec<_> = self.components.keys().collect();
        sorted_components.sort();

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

        for component_name in sorted_components {
            let existing_component_secrets = existing_secrets.get(component_name).unwrap();
            let component_secrets = &self.components[component_name];
            trace!("Generating secret for component: {}", component_name);
            println!("");
            println!("{}", component_name.white().bold());
            println!("{}", "=".repeat(component_name.len()));

            let mut sorted_secrets: Vec<_> = component_secrets.secrets.keys().collect();
            sorted_secrets.sort();

            for secret_name in sorted_secrets {
                let should_generate_new = if self.is_reference(component_name, secret_name) {
                    true
                } else if let Some(existing_value) = existing_component_secrets.get(secret_name) {
                    let mut input = String::new();
                    let value = if existing_value.len() >= 7 {
                        format!(
                            "{}****{}",
                            &existing_value[..2],
                            &existing_value[existing_value.len() - 3..]
                        )
                    } else {
                        "****".to_string()
                    };
                    print!(
                        "The secret `{}` [{}] already exists. Do you want to override it? (y/N)",
                        secret_name, value
                    );
                    std::io::stdout().flush()?;
                    std::io::stdin().read_line(&mut input)?;
                    let ret = matches!(input.trim().to_lowercase().as_str(), "y" | "yes");
                    if !ret {
                        store.add_secret(
                            component_name,
                            secret_name.clone(),
                            existing_value.to_string(),
                        );
                    }

                    ret
                } else {
                    true
                };

                if !should_generate_new {
                    continue;
                }

                let secret_value = self.generate_secret(component_name, secret_name);

                match secret_value {
                    GenerationResult::Value(value) => {
                        store.add_secret(component_name, secret_name.clone(), value);
                    }
                    GenerationResult::KeyPair(private_key, public_key) => {
                        store.add_secret(
                            component_name,
                            format!("{}_PRIVATE_KEY", secret_name),
                            private_key,
                        );
                        store.add_secret(
                            component_name,
                            format!("{}_PUBLIC_KEY", secret_name),
                            public_key,
                        );
                    }
                    GenerationResult::Ref(component, secret) => {
                        store.add_reference(component_name, secret_name.clone(), component, secret);
                    }
                    GenerationResult::None => {
                        panic!(
                            "Failed to get secret value for {} in component {}",
                            secret_name, component_name
                        );
                    }
                }
            }
        }

        store.resolve_references();

        for (component_name, component_set) in &store.components {
            println!("Writing {}", component_name);
            for (secret_name, _) in &component_set.secrets {
                println!("{}: ***", secret_name,);
            }
            let mut secrets = component_set.secrets.clone();
            let existing_secrets = existing_secrets.get(component_name).unwrap();

            vault
                .lock()
                .unwrap()
                .set(&self.product_name, component_name, env, secrets)
                .await
                .expect("Failed to set reference secrets in vault");
        }
        Ok(())
    }
}

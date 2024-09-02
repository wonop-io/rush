use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use crate::vault::Vault;
use std::sync::Arc;
use std::sync::Mutex;
use std::error::Error;
use rand::{distributions::Alphanumeric, Rng};
use base64;
use log::{warn, info};
use uuid::Uuid;
use chrono::Utc;
use openssl::rsa::Rsa;
use openssl::ec::{EcKey, EcGroup};
use openssl::nid::Nid;
use openssl::pkey::PKey;
use ed25519_dalek::{SigningKey, VerifyingKey, Signer};


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
    Ask(String),
    RandomString(usize),
    RandomAlphanumeric(usize),
    RandomHex(usize),
    RandomBase64(usize),
    RandomUUID,
    Timestamp,
    Ref(String),
    RSAKeyPair(usize, bool), // Added bool to specify base64 encoding
    ECDSAKeyPair(String, bool), // Added bool to specify base64 encoding
    Ed25519KeyPair(bool), // Added bool to specify base64 encoding
    AESKey(usize, bool), // Added bool to specify base64 encoding
    HMACKey(usize, bool), // Added bool to specify base64 encoding
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
                    Ok(_) => {
                        match serde_yaml::from_str(&contents) {
                            Ok(parsed_components) => parsed_components,
                            Err(e) => {
                                panic!("Unable to parse YAML file: {}. Returning empty definition.", e);
                            }
                        }
                    },
                    Err(e) => {
                        panic!("Unable to read YAML file: {}. Returning empty definition.", e);
                    }
                }
            },
            Err(e) => {
                warn!("Unable to open YAML file: {}. Returning empty definition.", e);
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

    pub fn generate_secret(&self, component_name: &str, secret_name: &str) -> GenerationResult {
        if let Some(component) = self.components.get(component_name) {
            if let Some(generation_method) = component.secrets.get(secret_name) {
                match generation_method {
                    GenerationMethod::Static(value) => GenerationResult::Value(value.clone()),
                    GenerationMethod::Ask(prompt) => {
                        // Implement the logic to handle the ask generation
                        // Print the prompt to the CLI and get the input from the user
                        println!("{}", prompt);
                        let mut input = String::new();
                        std::io::stdin().read_line(&mut input).expect("Failed to read input");
                        GenerationResult::Value(input.trim().to_string())
                    }
                    GenerationMethod::RandomString(length) => {
                        // Generate a random string of the specified length
                        let random_string: String = rand::thread_rng()
                            .sample_iter(&Alphanumeric)
                            .take(*length)
                            .map(char::from)
                            .collect();
                        GenerationResult::Value(random_string)
                    }
                    GenerationMethod::RandomAlphanumeric(length) => {
                        // Generate a random alphanumeric string of the specified length
                        let random_string: String = rand::thread_rng()
                            .sample_iter(&Alphanumeric)
                            .take(*length)
                            .map(char::from)
                            .collect();
                        GenerationResult::Value(random_string)
                    }
                    GenerationMethod::RandomHex(length) => {
                        // Generate a random hex string of the specified length
                        let random_bytes: Vec<u8> = (0..*length).map(|_| rand::random::<u8>()).collect();
                        GenerationResult::Value(hex::encode(random_bytes))
                    }
                    GenerationMethod::RandomBase64(length) => {
                        // Generate a random base64 string of the specified length
                        let random_bytes: Vec<u8> = (0..*length).map(|_| rand::random::<u8>()).collect();
                        GenerationResult::Value(base64::encode(random_bytes))
                    }
                    GenerationMethod::RandomUUID => {
                        // Generate a random UUID
                        GenerationResult::Value(Uuid::new_v4().to_string())
                    }
                    GenerationMethod::Timestamp => {
                        // Generate current timestamp
                        GenerationResult::Value(Utc::now().to_rfc3339())
                    }
                    GenerationMethod::Ref(path) => {
                        let path: Vec<&str> = path.split('.').collect();
                        let component = path[0].to_string();
                        let secret = path[1..].join(".");
                        GenerationResult::Ref(component, secret)
                    }
                    GenerationMethod::RSAKeyPair(bits, base64_encode) => {
                        // Generate RSA key pair
                        let rsa = Rsa::generate((*bits).try_into().unwrap()).expect("Failed to generate RSA key pair");
                        let private_key = rsa.private_key_to_pem().expect("Failed to get private key PEM");
                        let public_key = rsa.public_key_to_pem().expect("Failed to get public key PEM");
                        if *base64_encode {
                            GenerationResult::KeyPair(
                                base64::encode(&private_key),
                                base64::encode(&public_key)
                            )
                        } else {
                            GenerationResult::KeyPair(
                                String::from_utf8_lossy(&private_key).to_string(),
                                String::from_utf8_lossy(&public_key).to_string()
                            )
                        }
                    }
                    GenerationMethod::ECDSAKeyPair(curve, base64_encode) => {
                        // Generate ECDSA key pair
                        let nid = match curve.as_str() {
                            "P-256" => Nid::X9_62_PRIME256V1,
                            "secp256k1" => Nid::SECP256K1,
                            _ => panic!("Unsupported curve: {}", curve),
                        };
                        let group = EcGroup::from_curve_name(nid).expect("Failed to create EC group");
                        let key = EcKey::generate(&group).expect("Failed to generate EC key");
                        let pkey = PKey::from_ec_key(key).expect("Failed to create PKey from EC key");
                        let private_key = pkey.private_key_to_pem_pkcs8().expect("Failed to get private key PEM");
                        let public_key = pkey.public_key_to_pem().expect("Failed to get public key PEM");
                        if *base64_encode {
                            GenerationResult::KeyPair(
                                base64::encode(&private_key),
                                base64::encode(&public_key)
                            )
                        } else {
                            GenerationResult::KeyPair(
                                String::from_utf8_lossy(&private_key).to_string(),
                                String::from_utf8_lossy(&public_key).to_string()
                            )
                        }
                    }
                    GenerationMethod::Ed25519KeyPair(base64_encode) => {
                        // Generate Ed25519 key pair
                        let signing_key = SigningKey::from_bytes(&rand::random());
                        let verifying_key = VerifyingKey::from(&signing_key);
                        if *base64_encode {
                            GenerationResult::KeyPair(
                                base64::encode(signing_key.to_bytes()),
                                base64::encode(verifying_key.to_bytes())
                            )
                        } else {
                            GenerationResult::KeyPair(
                                hex::encode(signing_key.to_bytes()),
                                hex::encode(verifying_key.to_bytes())
                            )
                        }
                    }
                    GenerationMethod::AESKey(bits, base64_encode) => {
                        // Generate AES key
                        let key: Vec<u8> = (0..bits/8).map(|_| rand::random::<u8>()).collect();
                        if *base64_encode {
                            GenerationResult::Value(base64::encode(key))
                        } else {
                            GenerationResult::Value(hex::encode(key))
                        }
                    }
                    GenerationMethod::HMACKey(bits, base64_encode) => {
                        // Generate HMAC key
                        let key: Vec<u8> = (0..bits/8).map(|_| rand::random::<u8>()).collect();
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

    pub async fn populate(&self, vault: Arc<Mutex<dyn Vault + Send>>, env: &str) -> Result<(), Box<dyn Error>> {
        let mut all_secrets = HashMap::new();
        let mut all_references = HashMap::new();
        for (component_name, component_secrets) in &self.components {
            let mut secrets = HashMap::new();
            let mut references = Vec::new();
            info!("Generating secret for component: {}", component_name);

            for secret_name in component_secrets.secrets.keys() {                
                let secret_value = self.generate_secret(component_name, secret_name);
                match secret_value {
                    GenerationResult::Value(value) => {
                        secrets.insert(secret_name.clone(), value);
                    }
                    GenerationResult::KeyPair(private_key, public_key) => {
                        secrets.insert(format!("{}_PRIVATE_KEY", secret_name), private_key);
                        secrets.insert(format!("{}_PUBLIC_KEY", secret_name), public_key);
                    }   
                    GenerationResult::Ref(component, secret) => {
                        references.push((secret_name.clone(), component.clone(), secret.clone()));
                    }
                    GenerationResult::None => {
                        panic!("Failed to get secret value for {} in component {}", secret_name, component_name);
                    }
                }
            }
            let vault = vault.clone();
            let product_name = self.product_name.clone();
            let component_name = component_name.clone();
            let env = env.to_string();

            all_secrets.insert(component_name.clone(), secrets.clone());
            all_references.insert(component_name.clone(), references.clone());
        }

        for (component_name, references) in &all_references {
            let vault = vault.clone();
            let product_name = self.product_name.clone();
            let env = env.to_string();
            let mut secrets = all_secrets.get(component_name).unwrap().clone();

            for (secret_name, ref_component, ref_secret) in references {
                if let Some(ref_secrets) = all_secrets.get(ref_component) {
                    if let Some(ref_value) = ref_secrets.get(ref_secret) {
                        secrets.insert(secret_name.clone(), ref_value.clone());
                    } else {
                        panic!("Failed to get reference secret value for {} in component {}", ref_secret, ref_component);
                    }
                } else {
                    panic!("Failed to get reference secrets for component {}: Choices are {:?}", ref_component, all_secrets.keys());
                }
            }
            
            vault.lock().unwrap().set(&product_name, component_name, &env, secrets).await.expect("Failed to set reference secrets in vault");

        }
        Ok(())
    }
}
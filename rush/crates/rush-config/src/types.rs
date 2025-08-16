use log::trace;
use rush_core::constants::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tera::Context;
use tera::Tera;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainContext {
    pub product_name: String,
    pub product_uri: String,
    pub subdomain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    product_name: String,
    product_uri: String,
    product_dirname: String,
    product_path: PathBuf,
    network_name: String,
    environment: String,
    domain_template: String,
    kube_context: String,
    infrastructure_repository: String,
    docker_registry: String,
    root_path: String,
    vault_name: String,
    k8s_encoder: String,
    k8s_validator: String,
    k8s_version: String,
    one_password_account: Option<String>,
    json_vault_dir: Option<String>,
    start_port: u16,
}

impl Config {
    pub fn start_port(&self) -> u16 {
        self.start_port
    }

    pub fn k8s_encoder(&self) -> &str {
        &self.k8s_encoder
    }

    pub fn k8s_validator(&self) -> &str {
        &self.k8s_validator
    }

    pub fn k8s_version(&self) -> &str {
        &self.k8s_version
    }

    pub fn vault_name(&self) -> &str {
        &self.vault_name
    }

    pub fn product_name(&self) -> &str {
        &self.product_name
    }

    pub fn product_uri(&self) -> &str {
        &self.product_uri
    }

    pub fn product_path(&self) -> &PathBuf {
        &self.product_path
    }

    pub fn output_path(&self) -> PathBuf {
        self.product_path.join(TARGET_DIR)
    }

    pub fn network_name(&self) -> &str {
        &self.network_name
    }

    pub fn environment(&self) -> &str {
        &self.environment
    }

    pub fn domain_template(&self) -> &str {
        &self.domain_template
    }

    pub fn kube_context(&self) -> &str {
        &self.kube_context
    }

    pub fn infrastructure_repository(&self) -> &str {
        &self.infrastructure_repository
    }

    pub fn docker_registry(&self) -> &str {
        &self.docker_registry
    }

    pub fn one_password_account(&self) -> Option<&String> {
        self.one_password_account.as_ref()
    }

    pub fn json_vault_dir(&self) -> Option<&String> {
        self.json_vault_dir.as_ref()
    }

    pub fn domain(&self, subdomain: Option<String>) -> String {
        let ctx = DomainContext {
            product_name: self.product_name.clone(),
            product_uri: self.product_uri.clone(),
            subdomain,
        };
        let context = Context::from_serialize(&ctx).expect("Could not create config context");
        match Tera::one_off(&self.domain_template, &context, false) {
            Ok(d) => d,
            Err(e) => panic!("Could not render domain template: {e}"),
        }
    }

    pub fn root_path(&self) -> &str {
        &self.root_path
    }

    pub fn new(
        root_path: &str,
        product_name: &str,
        environment: &str,
        docker_registry: &str,
        start_port: u16,
    ) -> Result<Arc<Self>, String> {
        let product_name = product_name.to_string();
        let environment = environment.to_string();
        let docker_registry = docker_registry.to_string();

        let valid_environments = VALID_ENVIRONMENTS
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>();
        let product_uri = slug::slugify(&product_name).to_string();
        let product_uri = product_uri.to_lowercase();
        if !valid_environments.contains(&environment) {
            eprintln!("Invalid environment: {environment}");
            eprintln!("Valid environments: {valid_environments:#?}");
            return Err(format!("Invalid environment: {environment}"));
        }

        let kube_context = match environment.as_str() {
            ENV_DEV => std::env::var(DEV_CTX_VAR).expect("DEV_CTX environment variable not found"),
            ENV_PROD => {
                std::env::var(PROD_CTX_VAR).expect("PROD_CTX environment variable not found")
            }
            ENV_STAGING => {
                std::env::var(STAGING_CTX_VAR).expect("STAGING_CTX environment variable not found")
            }
            ENV_LOCAL => {
                std::env::var(LOCAL_CTX_VAR).expect("LOCAL_CTX environment variable not found")
            }
            _ => panic!("Invalid environment"),
        };

        let vault_name =
            match environment.as_str() {
                ENV_DEV => {
                    std::env::var(DEV_VAULT_VAR).expect("DEV_VAULT environment variable not found")
                }
                ENV_PROD => std::env::var(PROD_VAULT_VAR)
                    .expect("PROD_VAULT environment variable not found"),
                ENV_STAGING => std::env::var(STAGING_VAULT_VAR)
                    .expect("STAGING_VAULT environment variable not found"),
                ENV_LOCAL => std::env::var(LOCAL_VAULT_VAR)
                    .expect("LOCAL_VAULT environment variable not found"),
                _ => panic!("Invalid environment"),
            };

        let k8s_encoder = match environment.as_str() {
            ENV_DEV => std::env::var(K8S_ENCODER_DEV_VAR)
                .expect("K8S_ENCODER_DEV environment variable not found"),
            ENV_PROD => std::env::var(K8S_ENCODER_PROD_VAR)
                .expect("K8S_ENCODER_PROD environment variable not found"),
            ENV_STAGING => std::env::var(K8S_ENCODER_STAGING_VAR)
                .expect("K8S_ENCODER_STAGING environment variable not found"),
            ENV_LOCAL => std::env::var(K8S_ENCODER_LOCAL_VAR)
                .expect("K8S_ENCODER_LOCAL environment variable not found"),
            _ => panic!("Invalid environment"),
        };

        let k8s_validator = match environment.as_str() {
            ENV_DEV => std::env::var(K8S_VALIDATOR_DEV_VAR)
                .expect("K8S_VALIDATOR_DEV environment variable not found"),
            ENV_PROD => std::env::var(K8S_VALIDATOR_PROD_VAR)
                .expect("K8S_VALIDATOR_PROD environment variable not found"),
            ENV_STAGING => std::env::var(K8S_VALIDATOR_STAGING_VAR)
                .expect("K8S_VALIDATOR_STAGING environment variable not found"),
            ENV_LOCAL => std::env::var(K8S_VALIDATOR_LOCAL_VAR)
                .expect("K8S_VALIDATOR_LOCAL environment variable not found"),
            _ => panic!("Invalid environment"),
        };

        let k8s_version = match environment.as_str() {
            ENV_DEV => std::env::var(K8S_VERSION_DEV_VAR)
                .expect("K8S_VERSION_DEV environment variable not found"),
            ENV_PROD => std::env::var(K8S_VERSION_PROD_VAR)
                .expect("K8S_VERSION_PROD environment variable not found"),
            ENV_STAGING => std::env::var(K8S_VERSION_STAGING_VAR)
                .expect("K8S_VERSION_STAGING environment variable not found"),
            ENV_LOCAL => std::env::var(K8S_VERSION_LOCAL_VAR)
                .expect("K8S_VERSION_LOCAL environment variable not found"),
            _ => panic!("Invalid environment"),
        };

        let domain_template =
            match environment.as_str() {
                ENV_DEV => std::env::var(DEV_DOMAIN_VAR)
                    .expect("DEV_DOMAIN environment variable not found"),
                ENV_PROD => std::env::var(PROD_DOMAIN_VAR)
                    .expect("PROD_DOMAIN environment variable not found"),
                ENV_STAGING => std::env::var(STAGING_DOMAIN_VAR)
                    .expect("STAGING_DOMAIN environment variable not found"),
                ENV_LOCAL => std::env::var(LOCAL_DOMAIN_VAR)
                    .expect("LOCAL_DOMAIN environment variable not found"),
                _ => panic!("Invalid environment"),
            };

        let infrastructure_repository = std::env::var(INFRASTRUCTURE_REPOSITORY_VAR)
            .expect("INFRASTRUCTURE_REPOSITORY environment variable not found");

        // We assume in the rest of the code that the product path does not end with /
        let mut product_dirname = product_name
            .split('.')
            .rev()
            .collect::<Vec<&str>>()
            .join(".");
        // Use the root_path that was passed in instead of current_dir
        let products_dir = PathBuf::from(root_path).join(PRODUCTS_DIR);
        trace!("Products directory: {:?}", products_dir);

        // To support the Apple quirk that ".app" is an "App", we allow for using _ in the product name
        if let Ok(entries) = std::fs::read_dir(&products_dir) {
            let mut dirnames: Vec<(String, String)> = Vec::new();
            for entry in entries {
                if let Ok(entry) = entry {
                    if entry.path().is_dir() {
                        if let Some(dirname) = entry.file_name().to_str() {
                            dirnames.push((dirname.to_string(), dirname.replace('_', ".")));
                        }
                    }
                }
            }
            trace!("Searching for product path in {:#?}", dirnames);
            trace!(
                "Candidate product name: {} and {}",
                product_name,
                product_dirname
            );
            if let Some(normalized_name) = dirnames.iter().find(|&name| name.1 == product_dirname) {
                product_dirname = normalized_name.0.clone();
            } else if let Some(normalized_name) =
                dirnames.iter().find(|&name| name.1 == product_name)
            {
                product_dirname = normalized_name.0.clone();
            } else {
                panic!(
                    "Product
 path does not exist for product_dirname: {product_dirname}"
                );
            }
        }

        let product_path = products_dir.join(&product_dirname);
        if !product_path.exists() {
            panic!("Product path does not exist for product_dirname: {product_dirname}");
        }

        let network_name = format!("{NETWORK_PREFIX}{product_uri}");
        trace!("Product dirname: {}", product_dirname);

        let one_password_account = std::env::var(ONE_PASSWORD_ACCOUNT_VAR).ok();
        let json_vault_dir = std::env::var(JSON_VAULT_DIR_VAR).ok();

        let ret = Self {
            root_path: root_path.to_string(),
            product_name,
            product_uri,
            product_dirname,
            product_path,
            network_name,
            environment,
            domain_template: domain_template.to_string(),
            kube_context,
            infrastructure_repository,
            docker_registry,
            vault_name,
            k8s_encoder,
            k8s_validator,
            k8s_version,
            one_password_account,
            json_vault_dir,
            start_port,
        };

        Ok(Arc::new(ret))
    }

    /// Creates a test configuration suitable for testing
    /// This is available for integration tests and benchmarks
    pub fn test_default() -> Arc<Self> {
        Arc::new(Self {
            product_name: "test-product".to_string(),
            product_uri: "test-app".to_string(),
            product_dirname: "test_app".to_string(),
            product_path: PathBuf::from("/tmp/test_product"),
            network_name: "test-network".to_string(),
            environment: "dev".to_string(),
            domain_template: "{{subdomain}}.{{product_uri}}".to_string(),
            kube_context: "test-context".to_string(),
            infrastructure_repository: "git@github.com:test/infra.git".to_string(),
            docker_registry: "ghcr.io/test".to_string(),
            root_path: "/tmp".to_string(),
            vault_name: "test-vault".to_string(),
            k8s_encoder: "default".to_string(),
            k8s_validator: "default".to_string(),
            k8s_version: "v1.25.0".to_string(),
            one_password_account: None,
            json_vault_dir: None,
            start_port: 8000,
        })
    }
}

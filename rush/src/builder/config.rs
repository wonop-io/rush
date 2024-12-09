use log::trace;
use serde::{Deserialize, Serialize};
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
    product_path: String,
    network_name: String,
    environment: String,
    domain_template: String,
    kube_context: String,
    infrastructure_repository: String,
    docker_registry: String,
    root_path: String,
    vault_name: String,
    k8s_encoder: String,
    one_password_account: Option<String>,
    start_port: u16,
}

impl Config {
    pub fn start_port(&self) -> u16 {
        self.start_port
    }
    pub fn k8s_encoder(&self) -> &str {
        &self.k8s_encoder
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
    pub fn product_path(&self) -> &str {
        &self.product_path
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
    pub fn domain(&self, subdomain: Option<String>) -> String {
        let ctx = DomainContext {
            product_name: self.product_name.clone(),
            product_uri: self.product_uri.clone(),
            subdomain,
        };
        let context = Context::from_serialize(&ctx).expect("Could not create config context");
        match Tera::one_off(&self.domain_template, &context, false) {
            Ok(d) => d,
            Err(e) => panic!("Could not render domain template: {}", e),
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

        let valid_environments = ["local", "dev", "prod", "staging"]
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>();
        let product_uri = slug::slugify(&product_name).to_string();
        let product_uri = product_uri.to_lowercase();
        if !valid_environments.contains(&environment) {
            eprintln!("Invalid environment: {}", environment);
            eprintln!("Valid environments: {:#?}", valid_environments);
            return Err(format!("Invalid environment: {}", environment));
        }

        let kube_context = match environment.as_str() {
            "dev" => std::env::var("DEV_CTX").expect("DEV_CTX environment variable not found"),
            "prod" => std::env::var("PROD_CTX").expect("PROD_CTX environment variable not found"),
            "staging" => {
                std::env::var("STAGING_CTX").expect("STAGING_CTX environment variable not found")
            }
            "local" => {
                std::env::var("LOCAL_CTX").expect("LOCAL_CTX environment variable not found")
            }
            _ => panic!("Invalid environment"),
        };

        let vault_name = match environment.as_str() {
            "dev" => std::env::var("DEV_VAULT").expect("DEV_VAULT environment variable not found"),
            "prod" => {
                std::env::var("PROD_VAULT").expect("PROD_VAULT environment variable not found")
            }
            "staging" => std::env::var("STAGING_VAULT")
                .expect("STAGING_VAULT environment variable not found"),
            "local" => {
                std::env::var("LOCAL_VAULT").expect("LOCAL_VAULT environment variable not found")
            }
            _ => panic!("Invalid environment"),
        };

        let k8s_encoder = match environment.as_str() {
            "dev" => std::env::var("K8S_ENCODER_DEV")
                .expect("K8S_ENCODER_DEV environment variable not found"),
            "prod" => std::env::var("K8S_ENCODER_PROD")
                .expect("K8S_ENCODER_PROD environment variable not found"),
            "staging" => std::env::var("K8S_ENCODER_STAGING")
                .expect("K8S_ENCODER_STAGING environment variable not found"),
            "local" => std::env::var("K8S_ENCODER_LOCAL")
                .expect("K8S_ENCODER_LOCAL environment variable not found"),
            _ => panic!("Invalid environment"),
        };

        let domain_template =
            match environment.as_str() {
                "dev" => {
                    std::env::var("DEV_DOMAIN").expect("DEV_DOMAIN environment variable not found")
                }
                "prod" => std::env::var("PROD_DOMAIN")
                    .expect("PROD_DOMAIN environment variable not found"),
                "staging" => std::env::var("STAGING_DOMAIN")
                    .expect("STAGING_DOMAIN environment variable not found"),
                "local" => std::env::var("LOCAL_DOMAIN")
                    .expect("LOCAL_DOMAIN environment variable not found"),
                _ => panic!("Invalid environment"),
            };

        let infrastructure_repository = std::env::var("INFRASTRUCTURE_REPOSITORY")
            .expect("INFRASTRUCTURE_REPOSITORY environment variable not found");
        // We assume in the rest of the code that the product path does not end with /
        let mut product_dirname = product_name
            .split('.')
            .rev()
            .collect::<Vec<&str>>()
            .join(".");
        let products_dir = std::env::current_dir().unwrap().join("products");

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
                    "Product path does not exist for product_dirname: {}",
                    product_dirname
                );
            }
        }

        let product_path = products_dir.join(&product_dirname);
        if !product_path.exists() {
            panic!(
                "Product path does not exist for product_dirname: {}",
                product_dirname
            );
        }

        let product_path = product_path.to_str().unwrap().to_string();
        let network_name = format!("net-{}", product_uri);
        trace!("Product dirname: {}", product_dirname);

        let one_password_account = std::env::var("ONE_PASSWORD_ACCOUNT").ok();

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
            one_password_account,
            start_port,
        };

        Ok(Arc::new(ret))
    }
}

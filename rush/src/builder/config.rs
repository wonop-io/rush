use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tera::Context;
use tera::Tera;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    product_name: String,
    product_uri: String,
    product_path: String,
    network_name: String,
    environment: String,
    domain_template: String,
    domain: String,
    kube_context: String,
    infrastructure_repository: String,
    docker_registry: String,
    root_path: String,
    vault_name: String,
    k8s_encoder: String,
}

impl Config {
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
    pub fn domain(&self) -> &str {
        &self.domain
    }
    pub fn root_path(&self) -> &str {
        &self.root_path
    }

    pub fn new(
        root_path: &str,
        product_name: &str,
        environment: &str,
        docker_registry: &str,
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

        let k8s_encoder = std::env::var("K8S_ENCODER")
            .expect("K8S_ENCODER environment variable not found")
            .to_string();

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
        let product_path = format!("./products/{}", product_name); // TODO: hard coded path
        let network_name = format!("net-{}", product_uri);

        let mut ret = Self {
            root_path: root_path.to_string(),
            product_name,
            product_uri,
            product_path,
            network_name,
            environment,
            domain_template: domain_template.to_string(),
            domain: "".to_string(),
            kube_context,
            infrastructure_repository,
            docker_registry,
            vault_name,
            k8s_encoder,
        };

        let context = Context::from_serialize(&ret).expect("Could not create config context");
        ret.domain = match Tera::one_off(&domain_template, &context, false) {
            Ok(d) => d,
            Err(e) => panic!("Could not render domain template: {}", e),
        };

        Ok(Arc::new(ret))
    }
}

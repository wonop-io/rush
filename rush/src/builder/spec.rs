use super::Variables;
use crate::builder::Artefact;
use crate::builder::BuildContext;
use crate::builder::Config;
use crate::builder::{BuildScript, BuildType};
use crate::container::{ServiceSpec, ServicesSpec};
use crate::dotenv_utils::load_dotenv;
use crate::path_matcher::PathMatcher;
use crate::vault::Vault;
use crate::ToolchainContext;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct ComponentBuildSpec {
    pub build_type: BuildType,
    pub product_name: String,
    pub component_name: String,
    pub color: String,
    pub depends_on: Vec<String>,

    pub build: Option<String>,
    pub mount_point: Option<String>,
    pub subdomain: Option<String>,
    pub artefacts: Option<std::collections::HashMap<String, String>>,
    pub artefact_output_dir: String,
    pub docker_extra_run_args: Vec<String>,
    pub env: Option<HashMap<String, String>>, // TODO: Deprecated
    pub volumes: Option<HashMap<String, String>>,
    pub port: Option<u16>,
    pub target_port: Option<u16>,
    pub k8s: Option<String>, // TODO: Refactor to k8s_dir
    pub priority: u64,
    pub watch: Option<Arc<PathMatcher>>,

    // Set after loading
    pub config: Arc<Config>,
    pub variables: Arc<Variables>,
    pub services: Option<Arc<ServicesSpec>>,
    pub domains: Option<Arc<HashMap<String, String>>>,
    pub tagged_image_name: Option<String>,

    // Environment and secrets from the component
    pub dotenv: HashMap<String, String>,
    pub dotenv_secrets: HashMap<String, String>,
    pub domain: String,
}

impl ComponentBuildSpec {
    pub fn set_services(&mut self, services: Arc<ServicesSpec>) {
        self.services = Some(services);
    }

    pub fn set_domains(&mut self, domains: Arc<HashMap<String, String>>) {
        self.domains = Some(domains);
    }

    pub fn set_tagged_image_name(&mut self, tagged_image_name: String) {
        self.tagged_image_name = Some(tagged_image_name);
    }

    pub fn config(&self) -> Arc<Config> {
        self.config.clone()
    }

    pub fn from_yaml(
        config: Arc<Config>,
        variables: Arc<Variables>,
        yaml_section: &serde_yaml::Value,
    ) -> Self {
        let product_name = config.product_name();
        let build_type = match yaml_section
            .get("build_type")
            .expect("build_type is required")
            .as_str()
            .unwrap()
        {
            "TrunkWasm" => BuildType::TrunkWasm {
                context_dir: None,
                location: yaml_section
                    .get("location")
                    .expect("location is required for TrunkWasm")
                    .as_str()
                    .unwrap()
                    .to_string(),
                dockerfile_path: yaml_section
                    .get("dockerfile")
                    .expect("dockerfile_path is required")
                    .as_str()
                    .unwrap()
                    .to_string(),
            },
            "RustBinary" => BuildType::RustBinary {
                context_dir: Some(
                    yaml_section
                        .get("context_dir")
                        .map_or(".".to_string(), |v| v.as_str().unwrap().to_string()),
                ),
                location: yaml_section
                    .get("location")
                    .expect("location is required for RustBinary")
                    .as_str()
                    .unwrap()
                    .to_string(),
                dockerfile_path: yaml_section
                    .get("dockerfile")
                    .expect("dockerfile_path is required")
                    .as_str()
                    .unwrap()
                    .to_string(),
            },
            "Zola" => BuildType::Zola {
                context_dir: Some(
                    yaml_section
                        .get("context_dir")
                        .map_or(".".to_string(), |v| v.as_str().unwrap().to_string()),
                ),
                location: yaml_section
                    .get("location")
                    .expect("location is required for Book")
                    .as_str()
                    .unwrap()
                    .to_string(),
                dockerfile_path: yaml_section
                    .get("dockerfile")
                    .expect("dockerfile_path is required")
                    .as_str()
                    .unwrap()
                    .to_string(),
            },
            "Book" => BuildType::Book {
                context_dir: Some(
                    yaml_section
                        .get("context_dir")
                        .map_or(".".to_string(), |v| v.as_str().unwrap().to_string()),
                ),
                location: yaml_section
                    .get("location")
                    .expect("location is required for Book")
                    .as_str()
                    .unwrap()
                    .to_string(),
                dockerfile_path: yaml_section
                    .get("dockerfile")
                    .expect("dockerfile_path is required")
                    .as_str()
                    .unwrap()
                    .to_string(),
            },
            "Script" => BuildType::Script {
                context_dir: Some(
                    yaml_section
                        .get("context_dir")
                        .map_or(".".to_string(), |v| v.as_str().unwrap().to_string()),
                ),
                location: yaml_section
                    .get("location")
                    .expect("location is required for Script")
                    .as_str()
                    .unwrap()
                    .to_string(),
                dockerfile_path: yaml_section
                    .get("dockerfile")
                    .expect("dockerfile_path is required")
                    .as_str()
                    .unwrap()
                    .to_string(),
            },
            "Ingress" => BuildType::Ingress {
                context_dir: Some(
                    yaml_section
                        .get("context_dir")
                        .map_or(".".to_string(), |v| v.as_str().unwrap().to_string()),
                ),
                components: yaml_section
                    .get("components")
                    .expect("components are required for Ingress")
                    .as_sequence()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect(),
                dockerfile_path: yaml_section
                    .get("dockerfile")
                    .expect("dockerfile_path is required")
                    .as_str()
                    .unwrap()
                    .to_string(),
            },
            "Image" => BuildType::PureDockerImage {
                image_name_with_tag: yaml_section
                    .get("image")
                    .expect("image is required for PureDockerImage")
                    .as_str()
                    .unwrap()
                    .to_string(),
                command: yaml_section
                    .get("command")
                    .map(|v| v.as_str().unwrap().to_string()),
                entrypoint: yaml_section
                    .get("entrypoint")
                    .map(|v| v.as_str().unwrap().to_string()),
            },
            "K8sOnly" => BuildType::PureKubernetes,
            "K8sInstall" => BuildType::KubernetesInstallation {
                namespace: yaml_section
                    .get("namespace")
                    .expect("namespace is required for KubernetesInstallation")
                    .as_str()
                    .unwrap()
                    .to_string(),
            },

            _ => panic!("Invalid build_type"),
        };

        let cwd = std::env::current_dir()
            .expect("Failed to get current working directory")
            .to_str()
            .unwrap()
            .to_string();

        // Loading environment
        let location = match &build_type {
            BuildType::TrunkWasm { location, .. } => Some(location.clone()),
            BuildType::RustBinary { location, .. } => Some(location.clone()),
            BuildType::Zola { location, .. } => Some(location.clone()),
            BuildType::Book { location, .. } => Some(location.clone()),
            BuildType::Script { location, .. } => Some(location.clone()),
            _ => None,
        };
        let component_path = match location {
            Some(location) => {
                let binding = std::path::Path::new(&cwd).join(location);
                Some(binding.to_str().unwrap().to_string())
            }
            None => None,
        };

        let dotenv = match &component_path {
            Some(path) => {
                let dotenv_path = std::path::Path::new(&path).join(".env");
                if dotenv_path.exists() {
                    match load_dotenv(&dotenv_path) {
                        Ok(env) => env,
                        Err(e) => {
                            panic!("Failed to load .env file: {}", e);
                        }
                    }
                } else {
                    HashMap::new()
                }
            }
            None => HashMap::new(),
        };
        let dotenv_secrets = match &component_path {
            Some(path) => {
                let dotenv_secrets_path = std::path::Path::new(&path).join(".env.secrets");
                if dotenv_secrets_path.exists() {
                    match load_dotenv(&dotenv_secrets_path) {
                        Ok(env) => env,
                        Err(e) => {
                            panic!("Failed to load .env file: {}", e);
                        }
                    }
                } else {
                    HashMap::new()
                }
            }
            None => HashMap::new(),
        };
        let subdomain = yaml_section
            .get("subdomain")
            .map(|v| Self::process_template_string(v.as_str().unwrap(), &variables));
        let domain = config.domain(subdomain.clone());

        let watch = yaml_section.get("watch").map(|v| {
            let paths: Vec<String> = v
                .as_sequence()
                .unwrap()
                .iter()
                .map(|item| Self::process_template_string(item.as_str().unwrap(), &variables))
                .collect();
            Arc::new(PathMatcher::new(std::path::Path::new(&cwd), paths))
        });

        ComponentBuildSpec {
            build_type,
            build: yaml_section
                .get("build")
                .map(|v| Self::process_template_string(v.as_str().unwrap(), &variables)),

            color: yaml_section.get("color").map_or("blue".to_string(), |v| {
                Self::process_template_string(v.as_str().unwrap(), &variables)
            }),
            depends_on: yaml_section.get("depends_on").map_or(Vec::new(), |v| {
                v.as_sequence()
                    .unwrap()
                    .iter()
                    .map(|item| Self::process_template_string(item.as_str().unwrap(), &variables))
                    .collect()
            }),
            product_name: product_name.to_string(),
            component_name: Self::process_template_string(
                yaml_section
                    .get("component_name")
                    .expect("component_name is required")
                    .as_str()
                    .unwrap(),
                &variables,
            ),
            mount_point: yaml_section
                .get("mount_point")
                .map(|v| Self::process_template_string(v.as_str().unwrap(), &variables)),
            subdomain,
            artefacts: yaml_section.get("artefacts").map(|v| {
                v.as_mapping()
                    .unwrap()
                    .iter()
                    .map(|(k, val)| {
                        (
                            Self::process_template_string(k.as_str().unwrap(), &variables),
                            Self::process_template_string(val.as_str().unwrap(), &variables),
                        )
                    })
                    .collect()
            }),
            artefact_output_dir: yaml_section
                .get("artefact_output_dir")
                .map_or("target/rushd".to_string(), |v| {
                    Self::process_template_string(v.as_str().unwrap(), &variables)
                }),
            docker_extra_run_args: yaml_section.get("docker_extra_run_args").map_or_else(
                Vec::new,
                |v| {
                    v.as_sequence()
                        .unwrap()
                        .iter()
                        .map(|item| {
                            Self::process_template_string(item.as_str().unwrap(), &variables)
                        })
                        .collect()
                },
            ),
            env: yaml_section.get("env").map(|v| {
                v.as_mapping()
                    .unwrap()
                    .iter()
                    .map(|(k, val)| {
                        let v = Self::process_template_string(val.as_str().unwrap(), &variables);
                        (
                            Self::process_template_string(k.as_str().unwrap(), &variables),
                            v,
                        )
                    })
                    .collect()
            }),
            volumes: yaml_section.get("volumes").map(|v| {
                v.as_mapping()
                    .unwrap()
                    .iter()
                    .map(|(k, val)| {
                        let absolute_path = std::path::Path::new(&cwd)
                            .join(Self::process_template_string(
                                k.as_str().unwrap(),
                                &variables,
                            ))
                            .to_str()
                            .unwrap()
                            .to_string();
                        (
                            absolute_path,
                            Self::process_template_string(val.as_str().unwrap(), &variables),
                        )
                    })
                    .collect()
            }),
            port: yaml_section.get("port").map(|v| {
                if let Some(port_str) = v.as_str() {
                    let processed_str = Self::process_template_string(port_str, &variables);
                    processed_str
                        .parse::<u16>()
                        .unwrap_or_else(|_| panic!("Could not parse {}", processed_str))
                } else {
                    v.as_u64().unwrap() as u16
                }
            }),
            target_port: yaml_section.get("target_port").map(|v| {
                if let Some(target_port_str) = v.as_str() {
                    let processed_str = Self::process_template_string(target_port_str, &variables);
                    processed_str
                        .parse::<u16>()
                        .unwrap_or_else(|_| panic!("Could not parse {}", processed_str))
                } else {
                    v.as_u64().unwrap() as u16
                }
            }),
            k8s: yaml_section
                .get("k8s")
                .map(|v| Self::process_template_string(v.as_str().unwrap(), &variables)),
            priority: yaml_section
                .get("priority")
                .map_or(100, |v| v.as_u64().unwrap()),
            watch,
            config,
            variables,
            services: None,
            tagged_image_name: None,
            dotenv,
            dotenv_secrets,
            domain,
            domains: None,
        }
    }

    fn process_template_string(input: &str, variables: &Arc<Variables>) -> String {
        if input.starts_with("{{") && input.ends_with("}}") {
            let var_name = input.trim_start_matches("{{").trim_end_matches("}}").trim();
            variables
                .get(var_name)
                .unwrap_or_else(|| panic!("Variable `{}` not found", var_name))
                .to_string()
        } else {
            input.to_string()
        }
    }

    pub fn build_script(&self, ctx: &BuildContext) -> String {
        match &self.build {
            Some(build) => build.clone(),
            None => BuildScript::new(self.build_type.clone()).render(ctx),
        }
    }

    pub fn build_artefacts(&self) -> HashMap<String, Artefact> {
        let mut ret = HashMap::new();
        match &self.artefacts {
            Some(artefacts) => {
                for (k, v) in artefacts.iter() {
                    let artefact = Artefact::new(k.to_string(), v.to_string());
                    // let script = artefact.render(ctx);
                    ret.insert(k.to_string(), artefact);
                }
            }
            None => {}
        }
        ret
    }

    pub fn generate_build_context(
        &self,
        toolchain: Option<Arc<ToolchainContext>>,
        secrets: HashMap<String, String>,
    ) -> BuildContext {
        let services = self
            .services
            .clone()
            .expect("No services found for docker image");
        let domains = (*self
            .domains
            .clone()
            .expect("No services found for docker image"))
        .clone();
        let (location, services) = match &self.build_type {
            BuildType::TrunkWasm { location, .. } => (Some(location.clone()), None),
            BuildType::RustBinary { location, .. } => (Some(location.clone()), None),
            BuildType::Zola { location, .. } => (Some(location.clone()), None),
            BuildType::Book { location, .. } => (Some(location.clone()), None),
            BuildType::Script { location, .. } => (Some(location.clone()), None),
            BuildType::Ingress { components, .. } => {
                let services = services
                    .iter()
                    .map(|(domain, service_specs)| {
                        let filtered_service_specs: Vec<ServiceSpec> = service_specs
                            .iter()
                            .filter(|service_spec| components.contains(&service_spec.name))
                            .cloned()
                            .collect();
                        (domain.clone(), filtered_service_specs)
                    })
                    .filter(|(_, service_specs)| !service_specs.is_empty())
                    .collect();
                (None, Some(services))
            }
            BuildType::PureDockerImage { .. } => (None, None),
            BuildType::PureKubernetes => (None, None),
            BuildType::KubernetesInstallation { .. } => (None, None),
        };
        let toolchain = toolchain.clone().expect("No toolchain available");

        let product_name = self.product_name.clone();
        let product_uri = slug::slugify(&product_name);

        BuildContext {
            toolchain: (*toolchain).clone(),
            build_type: self.build_type.clone(),
            location,
            target: toolchain.target().clone(),
            host: toolchain.host().clone(),
            rust_target: toolchain.target().to_rust_target(),
            services: services.unwrap_or_default(),
            environment: self.config.environment().to_string(),
            domain: self.domain.clone(),
            product_name,
            product_uri,
            component: self.component_name.clone(),
            docker_registry: self.config.docker_registry().to_string(),
            image_name: self.tagged_image_name.clone().unwrap_or_default(),
            secrets,
            domains,
            env: self.dotenv.clone(),
        }
    }
}

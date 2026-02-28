use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use rush_config::Config;
use rush_core::dotenv::load_dotenv;
use rush_toolchain::ToolchainContext;
use rush_utils::PathMatcher;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::health_check::{parse_health_check, HealthCheckConfig};
use crate::{Artefact, BuildContext, BuildScript, BuildType, Variables};

/// Represents the build specification for a component
#[derive(Debug, Clone)]
pub struct ComponentBuildSpec {
    /// Type of build for this component
    pub build_type: BuildType,

    /// Product name the component belongs to
    pub product_name: String,

    /// Component name
    pub component_name: String,

    /// Color for console output formatting
    pub color: String,

    /// List of components this one depends on
    pub depends_on: Vec<String>,

    /// Optional custom build script
    pub build: Option<String>,

    /// Optional mount point for the component
    pub mount_point: Option<String>,

    /// Optional subdomain configuration
    pub subdomain: Option<String>,

    /// Optional map of artefact templates to render
    pub artefacts: Option<HashMap<String, String>>,

    /// Output directory for rendered artefacts
    pub artefact_output_dir: String,

    /// Extra Docker run arguments
    pub docker_extra_run_args: Vec<String>,

    /// Optional environment variables
    pub env: Option<HashMap<String, String>>,

    /// Optional volume mappings
    pub volumes: Option<HashMap<String, String>>,

    /// Optional container port
    pub port: Option<u16>,

    /// Optional target port
    pub target_port: Option<u16>,

    /// Optional Kubernetes manifest directory
    pub k8s: Option<String>,

    /// Deployment priority
    pub priority: u64,

    /// Optional file watching configuration
    pub watch: Option<Arc<PathMatcher>>,

    /// Configuration reference
    pub config: Arc<Config>,

    /// Variables reference
    pub variables: Arc<Variables>,

    /// Optional service spec
    pub services: Option<Arc<HashMap<String, Vec<ServiceSpec>>>>,

    /// Optional domain mapping
    pub domains: Option<Arc<HashMap<String, String>>>,

    /// Tagged image name for Docker
    pub tagged_image_name: Option<String>,

    /// Environment variables from .env file
    pub dotenv: HashMap<String, String>,

    /// Secret environment variables from .env.secrets file
    pub dotenv_secrets: HashMap<String, String>,

    /// Computed domain for the component
    pub domain: String,

    /// Cross-compilation method for Rust builds ("native" or "cross-rs")
    pub cross_compile: String,

    /// Health check configuration for verifying container readiness
    pub health_check: Option<HealthCheckConfig>,

    /// Startup probe configuration (used before health check during initial startup)
    pub startup_probe: Option<HealthCheckConfig>,
}

/// Represents a service specification for a component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSpec {
    /// Name of the service
    pub name: String,

    /// Host to bind to
    pub host: String,

    /// Port number
    pub port: u16,

    /// Target port (container)
    pub target_port: u16,

    /// Optional mount point
    pub mount_point: Option<String>,

    /// Domain the service is served on
    pub domain: String,

    /// Docker host name
    pub docker_host: String,
}

impl ComponentBuildSpec {
    /// Gets the Docker local name for this component
    pub fn docker_local_name(&self) -> String {
        rush_core::naming::NamingConvention::container_name(
            &self.product_name,
            &self.component_name,
        )
    }

    /// Sets the service specification
    pub fn set_services(&mut self, services: Arc<HashMap<String, Vec<ServiceSpec>>>) {
        self.services = Some(services);
    }

    /// Sets the domain mapping
    pub fn set_domains(&mut self, domains: Arc<HashMap<String, String>>) {
        self.domains = Some(domains);
    }

    /// Sets the tagged image name
    pub fn set_tagged_image_name(&mut self, tagged_image_name: String) {
        self.tagged_image_name = Some(tagged_image_name);
    }

    /// Gets the configuration reference
    pub fn config(&self) -> Arc<Config> {
        self.config.clone()
    }

    /// Creates a ComponentBuildSpec from a YAML section
    pub fn from_yaml(config: Arc<Config>, variables: Arc<Variables>, yaml_section: &Value) -> Self {
        let product_name = config.product_name().to_string();

        // Parse build type
        let build_type = match yaml_section
            .get("build_type")
            .expect("build_type is required")
            .as_str()
            .unwrap()
        {
            "TrunkWasm" => BuildType::TrunkWasm {
                context_dir: yaml_section
                    .get("context_dir")
                    .map(|v| v.as_str().unwrap().to_string()),
                location: yaml_section
                    .get("location")
                    .expect("location is required for TrunkWasm")
                    .as_str()
                    .unwrap()
                    .to_string(),
                ssr: yaml_section
                    .get("ssr")
                    .is_some_and(|v| v.as_bool().unwrap_or(false)),
                dockerfile_path: yaml_section
                    .get("dockerfile")
                    .expect("dockerfile_path is required")
                    .as_str()
                    .unwrap()
                    .to_string(),
                features: yaml_section.get("features").map(|v| {
                    v.as_sequence()
                        .unwrap()
                        .iter()
                        .map(|item| {
                            Self::process_template_string(item.as_str().unwrap(), &variables)
                        })
                        .collect()
                }),
                precompile_commands: yaml_section.get("precompile_commands").map(|v| {
                    v.as_sequence()
                        .unwrap()
                        .iter()
                        .map(|item| {
                            Self::process_template_string(item.as_str().unwrap(), &variables)
                        })
                        .collect()
                }),
            },
            "DixiousWasm" => BuildType::DixiousWasm {
                context_dir: yaml_section
                    .get("context_dir")
                    .map(|v| v.as_str().unwrap().to_string()),
                location: yaml_section
                    .get("location")
                    .expect("location is required for DixiousWasm")
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
                context_dir: yaml_section
                    .get("context_dir")
                    .map(|v| v.as_str().unwrap().to_string()),
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
                features: yaml_section.get("features").map(|v| {
                    v.as_sequence()
                        .unwrap()
                        .iter()
                        .map(|item| {
                            Self::process_template_string(item.as_str().unwrap(), &variables)
                        })
                        .collect()
                }),
                precompile_commands: yaml_section.get("precompile_commands").map(|v| {
                    v.as_sequence()
                        .unwrap()
                        .iter()
                        .map(|item| {
                            Self::process_template_string(item.as_str().unwrap(), &variables)
                        })
                        .collect()
                }),
            },
            "Zola" => BuildType::Zola {
                context_dir: yaml_section
                    .get("context_dir")
                    .map(|v| v.as_str().unwrap().to_string()),
                location: yaml_section
                    .get("location")
                    .expect("location is required for Zola")
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
                context_dir: yaml_section
                    .get("context_dir")
                    .map(|v| v.as_str().unwrap().to_string()),
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
                context_dir: yaml_section
                    .get("context_dir")
                    .map(|v| v.as_str().unwrap().to_string()),
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
                context_dir: yaml_section
                    .get("context_dir")
                    .map(|v| v.as_str().unwrap().to_string()),
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
            "LocalService" => Self::parse_local_service(yaml_section, &variables),
            "Bazel" => BuildType::Bazel {
                location: yaml_section
                    .get("location")
                    .expect("location is required for Bazel")
                    .as_str()
                    .unwrap()
                    .to_string(),
                output_dir: yaml_section
                    .get("output_dir")
                    .map(|v| v.as_str().unwrap().to_string())
                    .unwrap_or_else(|| "target/bazel-out".to_string()),
                context_dir: yaml_section
                    .get("context_dir")
                    .map(|v| v.as_str().unwrap().to_string()),
                targets: yaml_section.get("targets").map(|v| {
                    v.as_sequence()
                        .unwrap()
                        .iter()
                        .map(|item| item.as_str().unwrap().to_string())
                        .collect()
                }),
                additional_args: yaml_section.get("additional_args").map(|v| {
                    v.as_sequence()
                        .unwrap()
                        .iter()
                        .map(|item| item.as_str().unwrap().to_string())
                        .collect()
                }),
                base_image: yaml_section
                    .get("base_image")
                    .map(|v| v.as_str().unwrap().to_string()),
                oci_load_target: yaml_section
                    .get("oci_load_target")
                    .map(|v| v.as_str().unwrap().to_string()),
            },
            _ => panic!("Invalid build_type"),
        };

        // Use product directory as base for resolving relative paths
        let product_dir = config.product_path().to_str().unwrap().to_string();

        // Determine component path based on build type
        let location = match &build_type {
            BuildType::TrunkWasm { location, .. } => Some(location.clone()),
            BuildType::DixiousWasm { location, .. } => Some(location.clone()),
            BuildType::RustBinary { location, .. } => Some(location.clone()),
            BuildType::Zola { location, .. } => Some(location.clone()),
            BuildType::Book { location, .. } => Some(location.clone()),
            BuildType::Script { location, .. } => Some(location.clone()),
            BuildType::Bazel { location, .. } => Some(location.clone()),
            _ => None,
        };

        let component_path = location.as_ref().map(|l| {
            let binding = Path::new(&product_dir).join(l);
            binding.to_str().unwrap().to_string()
        });

        // Load environment variables
        let dotenv = match &component_path {
            Some(path) => {
                let dotenv_path = Path::new(path).join(".env");
                if dotenv_path.exists() {
                    match load_dotenv(&dotenv_path) {
                        Ok(env) => env,
                        Err(e) => {
                            panic!("Failed to load .env file: {e}");
                        }
                    }
                } else {
                    HashMap::new()
                }
            }
            None => HashMap::new(),
        };

        // Load secret environment variables
        let dotenv_secrets = match &component_path {
            Some(path) => {
                let dotenv_secrets_path = Path::new(path).join(".env.secrets");
                if dotenv_secrets_path.exists() {
                    match load_dotenv(&dotenv_secrets_path) {
                        Ok(env) => {
                            log::debug!(
                                "Loaded {} secrets from .env.secrets for component",
                                env.len()
                            );
                            for key in env.keys() {
                                log::debug!("  Secret key: {key}");
                            }
                            env
                        }
                        Err(e) => {
                            panic!("Failed to load .env.secrets file: {e}");
                        }
                    }
                } else {
                    log::debug!(
                        "No .env.secrets file found at: {}",
                        dotenv_secrets_path.display()
                    );
                    HashMap::new()
                }
            }
            None => HashMap::new(),
        };

        // Process subdomain and compute domain
        let subdomain = yaml_section
            .get("subdomain")
            .map(|v| Self::process_template_string(v.as_str().unwrap(), &variables));
        let domain = config.domain(subdomain.clone());

        // Configure file watching
        let watch = yaml_section.get("watch").map(|v| {
            let paths: Vec<String> = v
                .as_sequence()
                .unwrap()
                .iter()
                .map(|item| Self::process_template_string(item.as_str().unwrap(), &variables))
                .collect();
            Arc::new(PathMatcher::new(Path::new(&product_dir), paths))
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
                        (
                            Self::process_template_string(k.as_str().unwrap(), &variables),
                            Self::process_template_string(val.as_str().unwrap(), &variables),
                        )
                    })
                    .collect()
            }),
            volumes: yaml_section.get("volumes").map(|v| {
                v.as_mapping()
                    .unwrap()
                    .iter()
                    .map(|(k, val)| {
                        let absolute_path = Path::new(&product_dir)
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
                        .unwrap_or_else(|_| panic!("Could not parse port: {processed_str}"))
                } else {
                    v.as_u64().unwrap() as u16
                }
            }),
            target_port: yaml_section.get("target_port").map(|v| {
                if let Some(target_port_str) = v.as_str() {
                    let processed_str = Self::process_template_string(target_port_str, &variables);
                    processed_str
                        .parse::<u16>()
                        .unwrap_or_else(|_| panic!("Could not parse target_port: {processed_str}"))
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
            cross_compile: yaml_section
                .get("cross_compile")
                .map(|v| v.as_str().unwrap().to_string())
                .unwrap_or_else(|| "native".to_string()),
            health_check: yaml_section
                .get("health_check")
                .and_then(parse_health_check),
            startup_probe: yaml_section
                .get("startup_probe")
                .and_then(parse_health_check),
        }
    }

    /// Parse a LocalService build type from YAML
    fn parse_local_service(yaml_section: &Value, _variables: &Arc<Variables>) -> BuildType {
        use log::{debug, warn};
        use rush_core::service_constants::version_validation;

        let service_type = yaml_section
            .get("service_type")
            .and_then(|v| v.as_str())
            .ok_or("service_type is required for LocalService and must be a string")
            .unwrap()
            .to_string();

        debug!("Parsing LocalService with service_type: {service_type}");

        // Parse and validate version
        let version = yaml_section
            .get("version")
            .and_then(|v| v.as_str())
            .map(|v| {
                if let Err(e) = version_validation::validate_version(v) {
                    warn!(
                        "Version validation warning for LocalService '{}': {}",
                        yaml_section
                            .get("component_name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("unknown"),
                        e
                    );
                }
                v.to_string()
            });

        // Parse environment variables with error handling
        let env = Self::parse_env_variables(yaml_section);

        // Parse persist_data with default
        let persist_data = yaml_section
            .get("persist_data")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| {
                debug!("persist_data not specified for LocalService, defaulting to false");
                false
            });

        BuildType::LocalService {
            service_type,
            version,
            env,
            persist_data,
            health_check: yaml_section
                .get("health_check")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            init_scripts: Self::parse_string_sequence(yaml_section, "init_scripts"),
            post_startup_tasks: Self::parse_string_sequence(yaml_section, "post_startup_tasks"),
            depends_on: Self::parse_string_sequence(yaml_section, "depends_on"),
            command: yaml_section
                .get("command")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        }
    }

    /// Parse environment variables from YAML with error handling
    fn parse_env_variables(yaml_section: &Value) -> Option<HashMap<String, String>> {
        use log::warn;

        yaml_section.get("env").map(|v| match v.as_mapping() {
            Some(map) => map
                .iter()
                .filter_map(|(k, val)| match (k.as_str(), val.as_str()) {
                    (Some(key), Some(value)) => Some((key.to_string(), value.to_string())),
                    _ => {
                        warn!("Skipping invalid environment variable in LocalService");
                        None
                    }
                })
                .collect(),
            None => {
                warn!("Environment variables for LocalService should be a mapping");
                HashMap::new()
            }
        })
    }

    /// Parse a sequence of strings from YAML
    fn parse_string_sequence(yaml_section: &Value, field_name: &str) -> Option<Vec<String>> {
        yaml_section
            .get(field_name)
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .collect()
            })
    }

    /// Process a template string with variables
    fn process_template_string(input: &str, variables: &Arc<Variables>) -> String {
        if input.starts_with("{{") && input.ends_with("}}") {
            let var_name = input.trim_start_matches("{{").trim_end_matches("}}").trim();
            variables
                .get(var_name)
                .unwrap_or_else(|| panic!("Variable `{var_name}` not found"))
                .to_string()
        } else {
            input.to_string()
        }
    }

    /// Get the build script for this component
    pub fn build_script(&self, ctx: &BuildContext) -> String {
        match &self.build {
            Some(build) => build.clone(),
            None => BuildScript::new(self.build_type.clone()).render(ctx),
        }
    }

    /// Get the build artefacts for this component
    pub fn build_artefacts(&self) -> Result<HashMap<String, Artefact>, rush_core::error::Error> {
        let mut ret = HashMap::new();
        if let Some(artefacts) = &self.artefacts {
            for (k, v) in artefacts.iter() {
                let artefact = Artefact::new(k.to_string(), v.to_string())?;
                ret.insert(k.to_string(), artefact);
            }
        }
        Ok(ret)
    }

    /// Generate a build context for this component
    pub fn generate_build_context(
        &self,
        toolchain: Option<Arc<ToolchainContext>>,
        secrets: HashMap<String, String>,
    ) -> BuildContext {
        // Make services optional with default empty map
        let services = self
            .services
            .clone()
            .unwrap_or_else(|| Arc::new(HashMap::new()));

        // Make domains optional with default empty map
        let domains = self
            .domains
            .clone()
            .map(|d| (*d).clone())
            .unwrap_or_default();

        let (location, filtered_services) = match &self.build_type {
            BuildType::TrunkWasm { location, .. } => (Some(location.clone()), None),
            BuildType::DixiousWasm { location, .. } => (Some(location.clone()), None),
            BuildType::RustBinary { location, .. } => (Some(location.clone()), None),
            BuildType::Zola { location, .. } => (Some(location.clone()), None),
            BuildType::Book { location, .. } => (Some(location.clone()), None),
            BuildType::Script { location, .. } => (Some(location.clone()), None),
            BuildType::Bazel { location, .. } => (Some(location.clone()), None),
            BuildType::Ingress { components, .. } => {
                let filtered = services
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
                (None, Some(filtered))
            }
            BuildType::PureDockerImage { .. } => (None, None),
            BuildType::PureKubernetes => (None, None),
            BuildType::KubernetesInstallation { .. } => (None, None),
            BuildType::LocalService { .. } => (None, None),
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
            services: filtered_services.unwrap_or_default(),
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
            cross_compile: self.cross_compile.clone(),
            skip_host_build: false, // This is determined at build time by orchestrator
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rush_config::Config;
    use serde_yaml;

    use super::*;
    use crate::{BuildType, Variables};

    fn create_test_config() -> Arc<Config> {
        Config::test_default()
    }

    fn create_test_variables() -> Arc<Variables> {
        Variables::empty()
    }

    #[test]
    fn test_parse_local_service_flat_structure() {
        let yaml_str = r#"
        build_type: "LocalService"
        component_name: "postgres"
        service_type: "postgresql"
        version: "15"
        persist_data: true
        env:
          POSTGRES_USER: "testuser"
          POSTGRES_PASSWORD: "testpass"
          POSTGRES_DB: "testdb"
          POSTGRES_PORT: "5432"
        health_check: "pg_isready -U testuser -p 5432"
        init_scripts:
          - "init.sql"
        depends_on:
          - "redis"
        command: "postgres -c max_connections=200"
        "#;

        let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let config = create_test_config();
        let variables = create_test_variables();

        let spec = ComponentBuildSpec::from_yaml(config, variables, &yaml_value);

        // Verify the build type is LocalService
        match &spec.build_type {
            BuildType::LocalService {
                service_type,
                version,
                persist_data,
                env,
                health_check,
                init_scripts,
                post_startup_tasks: _,
                depends_on,
                command,
            } => {
                assert_eq!(service_type, "postgresql");
                assert_eq!(version.as_deref(), Some("15"));
                assert!(*persist_data);

                // Check environment variables
                let env = env.as_ref().unwrap();
                assert_eq!(env.get("POSTGRES_USER"), Some(&"testuser".to_string()));
                assert_eq!(env.get("POSTGRES_PASSWORD"), Some(&"testpass".to_string()));
                assert_eq!(env.get("POSTGRES_DB"), Some(&"testdb".to_string()));
                assert_eq!(env.get("POSTGRES_PORT"), Some(&"5432".to_string()));

                assert_eq!(
                    health_check.as_deref(),
                    Some("pg_isready -U testuser -p 5432")
                );

                let init_scripts = init_scripts.as_ref().unwrap();
                assert_eq!(init_scripts.len(), 1);
                assert_eq!(init_scripts[0], "init.sql");

                let depends_on = depends_on.as_ref().unwrap();
                assert_eq!(depends_on.len(), 1);
                assert_eq!(depends_on[0], "redis");

                assert_eq!(command.as_deref(), Some("postgres -c max_connections=200"));
            }
            _ => panic!("Expected LocalService build type"),
        }
    }

    #[test]
    fn test_parse_local_service_minimal() {
        let yaml_str = r#"
        build_type: "LocalService"
        component_name: "redis"
        service_type: "redis"
        persist_data: false
        "#;

        let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let config = create_test_config();
        let variables = create_test_variables();

        let spec = ComponentBuildSpec::from_yaml(config, variables, &yaml_value);

        match &spec.build_type {
            BuildType::LocalService {
                service_type,
                version,
                persist_data,
                env,
                health_check,
                init_scripts,
                post_startup_tasks: _,
                depends_on,
                command,
            } => {
                assert_eq!(service_type, "redis");
                assert!(version.is_none());
                assert!(!(*persist_data));
                assert!(env.is_none());
                assert!(health_check.is_none());
                assert!(init_scripts.is_none());
                assert!(depends_on.is_none());
                assert!(command.is_none());
            }
            _ => panic!("Expected LocalService build type"),
        }
    }

    #[test]
    fn test_parse_local_service_with_version() {
        let yaml_str = r#"
        build_type: "LocalService"
        component_name: "mongodb"
        service_type: "mongodb"
        version: "6.0"
        persist_data: true
        env:
          MONGO_INITDB_ROOT_USERNAME: "admin"
          MONGO_INITDB_ROOT_PASSWORD: "secret"
          MONGO_PORT: "27017"
        "#;

        let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let config = create_test_config();
        let variables = create_test_variables();

        let spec = ComponentBuildSpec::from_yaml(config, variables, &yaml_value);

        match &spec.build_type {
            BuildType::LocalService {
                service_type,
                version,
                persist_data,
                env,
                ..
            } => {
                assert_eq!(service_type, "mongodb");
                assert_eq!(version.as_deref(), Some("6.0"));
                assert!(*persist_data);

                let env = env.as_ref().unwrap();
                assert_eq!(env.get("MONGO_PORT"), Some(&"27017".to_string()));
            }
            _ => panic!("Expected LocalService build type"),
        }
    }

    #[test]
    fn test_local_service_no_docker_fields() {
        // This test ensures that Docker-specific fields are not present in LocalService
        let yaml_str = r#"
        build_type: "LocalService"
        component_name: "stripe"
        service_type: "stripe-cli"
        persist_data: false
        env:
          STRIPE_API_KEY: "test_key"
        command: "stripe listen --forward-to localhost:8080"
        "#;

        let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let config = create_test_config();
        let variables = create_test_variables();

        let spec = ComponentBuildSpec::from_yaml(config, variables, &yaml_value);

        match &spec.build_type {
            BuildType::LocalService { service_type, .. } => {
                assert_eq!(service_type, "stripe-cli");
                // The BuildType::LocalService variant doesn't have image, ports, volumes, or docker_args fields
                // This is enforced by the type system
            }
            _ => panic!("Expected LocalService build type"),
        }
    }

    #[test]
    #[should_panic(expected = "service_type is required for LocalService")]
    fn test_local_service_missing_service_type() {
        let yaml_str = r#"
        build_type: "LocalService"
        component_name: "test"
        persist_data: true
        "#;

        let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let config = create_test_config();
        let variables = create_test_variables();

        // This should panic because service_type is required
        ComponentBuildSpec::from_yaml(config, variables, &yaml_value);
    }
}

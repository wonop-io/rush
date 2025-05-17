// File: src/core/environment/loader.rs
// use super::types::{ComponentEnvironment, GenerationMethod, PublicEnvironmentDefinitions};
use log::warn;
// use std::collections::HashMap;
use crate::core::environment::ComponentEnvironment;

use crate::core::environment::GenerationMethod;
use crate::core::environment::PublicEnvironmentDefinitions;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
// use std::path::PathBuf;

impl PublicEnvironmentDefinitions {
    /// Create a new environment definition by loading and merging base and specialization files
    pub fn new(product_name: String, base_yaml: &str, specialisation_yaml: &str) -> Self {
        let product_dir = PathBuf::from(base_yaml).parent().unwrap().to_path_buf();

        let base_components = Self::load_components(base_yaml, true);
        let specialisation_components = Self::load_components(specialisation_yaml, false);
        let components = Self::merge_components(base_components, specialisation_components);

        Self {
            product_name,
            components,
            product_dir,
        }
    }

    /// Load component definitions from a YAML file
    fn load_components(
        yaml_path: &str,
        is_base: bool,
    ) -> HashMap<String, HashMap<String, GenerationMethod>> {
        match File::open(yaml_path) {
            Ok(mut file) => {
                let mut contents = String::new();
                match file.read_to_string(&mut contents) {
                    Ok(_) => {
                        match serde_yaml::from_str(&contents) {
                            Ok(parsed_components) => parsed_components,
                            Err(e) => {
                                if is_base {
                                    panic!("Unable to parse YAML file '{}': {}. Returning empty definition.", yaml_path, e)
                                } else {
                                    warn!("Unable to parse YAML file '{}': {}. Ignoring specialisation.", yaml_path, e);
                                    HashMap::new()
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if is_base {
                            panic!(
                                "Unable to read YAML file '{}': {}. Returning empty definition.",
                                yaml_path, e
                            )
                        } else {
                            warn!(
                                "Unable to read YAML file '{}': {}. Ignoring specialisation.",
                                yaml_path, e
                            );
                            HashMap::new()
                        }
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Unable to open YAML file '{}': {}. Returning empty definition.",
                    yaml_path, e
                );
                HashMap::new()
            }
        }
    }

    /// Merge base and specialization components
    fn merge_components(
        mut base: HashMap<String, HashMap<String, GenerationMethod>>,
        specialisation: HashMap<String, HashMap<String, GenerationMethod>>,
    ) -> HashMap<String, ComponentEnvironment> {
        for (component_name, env_vars) in specialisation {
            match base.get_mut(&component_name) {
                Some(base_env_vars) => base_env_vars.extend(env_vars),
                None => {
                    base.insert(component_name, env_vars);
                }
            }
        }

        base.into_iter()
            .map(|(component_name, environment_variables)| {
                (
                    component_name,
                    ComponentEnvironment {
                        environment_variables,
                    },
                )
            })
            .collect()
    }

    /// Add a new component to the environment definitions
    pub fn add_component(&mut self, component_name: String) {
        self.components.insert(
            component_name,
            ComponentEnvironment {
                environment_variables: HashMap::new(),
            },
        );
    }

    /// Add an environment variable to a component
    pub fn add_environment_variable(
        &mut self,
        component_name: &str,
        variable_name: String,
        generation_method: GenerationMethod,
    ) {
        if let Some(component) = self.components.get_mut(component_name) {
            component
                .environment_variables
                .insert(variable_name, generation_method);
        } else {
            panic!("Component {} not found", component_name);
        }
    }
}

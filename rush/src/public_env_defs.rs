use crate::dotenv_utils::load_dotenv;
use crate::dotenv_utils::save_dotenv;
use chrono::Local;
use colored::Colorize;
use log::{error, trace, warn};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicEnvironmentDefinitions {
    product_name: String,
    components: HashMap<String, ComponentEnvironment>,
    product_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentEnvironment {
    environment_variables: HashMap<String, GenerationMethod>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GenerationMethod {
    Static(String),
    Ask(String),
    AskWithDefault(String, String),
    Timestamp(String),
}

impl PublicEnvironmentDefinitions {
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
                                let message = if is_base {
                                    panic!("Unable to parse YAML file '{}': {}. Returning empty definition.", yaml_path, e)
                                } else {
                                    warn!("Unable to parse YAML file '{}': {}. Ignoring specialisation.", yaml_path, e);
                                    HashMap::new()
                                };
                                message
                            }
                        }
                    }
                    Err(e) => {
                        let message = if is_base {
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
                        };
                        message
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

    pub fn add_component(&mut self, component_name: String) {
        self.components.insert(
            component_name,
            ComponentEnvironment {
                environment_variables: HashMap::new(),
            },
        );
    }

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

    pub fn generate_value(&self, component_name: &str, variable_name: &str) -> Option<String> {
        if let Some(component) = self.components.get(component_name) {
            if let Some(generation_method) = component.environment_variables.get(variable_name) {
                match generation_method {
                    GenerationMethod::Static(value) => Some(value.clone()),
                    GenerationMethod::Ask(prompt) => {
                        print!("{}", prompt.bold().white());
                        let mut input = String::new();
                        std::io::stdin()
                            .read_line(&mut input)
                            .expect("Failed to read input");
                        Some(input.trim().to_string())
                    }
                    GenerationMethod::AskWithDefault(prompt, default) => {
                        print!(
                            "{}",
                            format!("{} (default: {}): ", prompt, default)
                                .bold()
                                .white()
                        );
                        let mut input = String::new();
                        std::io::stdin()
                            .read_line(&mut input)
                            .expect("Failed to read input");
                        let input = input.trim();
                        if input.is_empty() {
                            Some(default.clone())
                        } else {
                            Some(input.to_string())
                        }
                    }
                    GenerationMethod::Timestamp(format) => {
                        Some(Local::now().format(format).to_string())
                    }
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn generate_dotenv_files(&self) -> Result<(), std::io::Error> {
        // TODO: Get from config
        let stack_yaml_path = self.product_dir.join("stack.spec.yaml");
        let stack_yaml_content = match fs::read_to_string(&stack_yaml_path) {
            Ok(content) => content,
            Err(e) => {
                error!("Failed to read stack.spec.yaml: {}", e);
                return Err(e);
            }
        };
        let stack_yaml: Value =
            serde_yaml::from_str(&stack_yaml_content).expect("Unable to parse stack.spec.yaml");

        if let Some(components_map) = stack_yaml.as_mapping() {
            for (component_name, component_info) in components_map {
                if let (Some(component_name), Some(location)) = (
                    component_name.as_str(),
                    component_info.get("location").and_then(|v| v.as_str()),
                ) {
                    let component_dir = self.product_dir.join(location);
                    if !component_dir.exists() {
                        trace!("Component {} directory not found, skipping", component_name);
                        continue;
                    }
                    let env_path = component_dir.join(".env");

                    if let Some(component) = self.components.get(component_name) {
                        let mut env_map = if env_path.exists() {
                            load_dotenv(&env_path)?
                        } else {
                            HashMap::new()
                        };

                        for (var_name, generation_method) in &component.environment_variables {
                            if !env_map.contains_key(var_name)
                                || matches!(generation_method, GenerationMethod::Static(_))
                            {
                                if let Some(value) = self.generate_value(component_name, var_name) {
                                    env_map.insert(var_name.clone(), value);
                                } else {
                                    error!("Failed to generate value for {}", var_name);
                                }
                            }
                        }

                        save_dotenv(&env_path, env_map)?;
                    }
                }
            }
        }
        Ok(())
    }
}

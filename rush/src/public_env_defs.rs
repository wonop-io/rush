use crate::dotenv_utils::load_dotenv;
use crate::dotenv_utils::save_dotenv;
use chrono::Local;
use log::{error, warn};
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
    Timestamp(String),
}

impl PublicEnvironmentDefinitions {
    pub fn new(product_name: String, yaml_filename: &str) -> Self {
        let product_dir = PathBuf::from(yaml_filename).parent().unwrap().to_path_buf();
        let components = match File::open(yaml_filename) {
            Ok(mut file) => {
                let mut contents = String::new();
                match file.read_to_string(&mut contents) {
                    Ok(_) => match serde_yaml::from_str(&contents) {
                        Ok(parsed_components) => parsed_components,
                        Err(e) => {
                            panic!(
                                "Unable to parse YAML file '{}': {}. Returning empty definition.",
                                yaml_filename, e
                            );
                        }
                    },
                    Err(e) => {
                        panic!(
                            "Unable to read YAML file '{}': {}. Returning empty definition.",
                            yaml_filename, e
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
            .map(|(component_name, environment_variables)| {
                (
                    component_name,
                    ComponentEnvironment {
                        environment_variables,
                    },
                )
            })
            .collect();

        Self {
            product_name,
            components,
            product_dir,
        }
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
                        println!("{}", prompt);
                        let mut input = String::new();
                        std::io::stdin()
                            .read_line(&mut input)
                            .expect("Failed to read input");
                        Some(input.trim().to_string())
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
        let stack_yaml_content = fs::read_to_string(&stack_yaml_path)?;
        let stack_yaml: Value =
            serde_yaml::from_str(&stack_yaml_content).expect("Unable to parse stack.spec.yaml");

        if let Some(components_map) = stack_yaml.as_mapping() {
            for (component_name, component_info) in components_map {
                if let (Some(component_name), Some(location)) = (
                    component_name.as_str(),
                    component_info.get("location").and_then(|v| v.as_str()),
                ) {
                    let component_dir = self.product_dir.join(location);
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

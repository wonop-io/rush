// File: src/core/environment/generator.rs
// use super::types::{GenerationMethod, PublicEnvironmentDefinitions};
use crate::core::dotenv::{load_dotenv, save_dotenv};
use chrono::Local;
use colored::Colorize;
use log::{error, trace};
// use std::collections::HashMap;
use crate::core::environment::GenerationMethod;
use crate::core::environment::PublicEnvironmentDefinitions;
use std::collections::HashMap;
use std::fs;
use std::io::Error;
use std::path::PathBuf;
// use std::path::PathBuf;

impl PublicEnvironmentDefinitions {
    /// Generate a value for an environment variable
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
                        Some(Local::now().format(&format).to_string())
                    }
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Generate .env files for all components
    pub fn generate_dotenv_files(&self) -> Result<(), Error> {
        let stack_yaml_path = self.product_dir.join("stack.spec.yaml");
        let components_map = read_stack_spec(&stack_yaml_path)?;

        for (component_name, location) in components_map {
            let component_dir = self.product_dir.join(location);
            if !component_dir.exists() {
                trace!("Component {} directory not found, skipping", component_name);
                continue;
            }

            generate_component_env_file(self, &component_name, &component_dir)?;
        }

        Ok(())
    }
}

/// Read and parse the stack specification file
fn read_stack_spec(stack_yaml_path: &PathBuf) -> Result<Vec<(String, String)>, Error> {
    let stack_yaml_content = match fs::read_to_string(stack_yaml_path) {
        Ok(content) => content,
        Err(e) => {
            error!("Failed to read stack.spec.yaml: {}", e);
            return Err(e);
        }
    };

    let stack_yaml: serde_yaml::Value =
        serde_yaml::from_str(&stack_yaml_content).expect("Unable to parse stack.spec.yaml");

    let mut components = Vec::new();

    if let Some(components_map) = stack_yaml.as_mapping() {
        for (component_name, component_info) in components_map {
            if let (Some(component_name), Some(location)) = (
                component_name.as_str(),
                component_info.get("location").and_then(|v| v.as_str()),
            ) {
                components.push((component_name.to_string(), location.to_string()));
            }
        }
    }

    Ok(components)
}

/// Generate a .env file for a single component
fn generate_component_env_file(
    env_defs: &PublicEnvironmentDefinitions,
    component_name: &str,
    component_dir: &PathBuf,
) -> Result<(), Error> {
    let env_path = component_dir.join(".env");

    if let Some(component) = env_defs.get_components().get(component_name) {
        let mut env_map = if env_path.exists() {
            load_dotenv(&env_path)?
        } else {
            HashMap::new()
        };

        for (var_name, generation_method) in &component.environment_variables {
            if !env_map.contains_key(var_name)
                || matches!(generation_method, GenerationMethod::Static(_))
            {
                if let Some(value) = env_defs.generate_value(component_name, var_name) {
                    env_map.insert(var_name.clone(), value);
                } else {
                    error!("Failed to generate value for {}", var_name);
                }
            }
        }

        save_dotenv(&env_path, env_map)?;
    }

    Ok(())
}

//! Environment-specific configurations and definitions
//!
//! This module provides functionality for managing environment-specific
//! configuration settings across different deployment environments.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};

use log::{debug, error, info, trace, warn};
use serde::{Deserialize, Serialize};

/// Represents environment-specific variable definitions for a product
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentDefinitions {
    /// The name of the product these definitions belong to
    product_name: String,
    /// Component-level environment configurations
    components: HashMap<String, ComponentEnvironment>,
    /// The root directory of the product
    product_dir: PathBuf,
}

/// Represents environment-specific configuration for a component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentEnvironment {
    /// Environment variables for the component
    environment_variables: HashMap<String, GenerationMethod>,
}

/// Methods for generating environment variable values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GenerationMethod {
    /// A static, predefined value
    Static(String),
    /// Request user input with a prompt
    Ask(String),
    /// Request user input with a prompt and default value
    AskWithDefault(String, String),
    /// Current timestamp with specified format
    Timestamp(String),
}

impl EnvironmentDefinitions {
    /// Creates a new EnvironmentDefinitions instance by loading configuration from YAML files
    ///
    /// # Arguments
    ///
    /// * `product_name` - The name of the product
    /// * `base_yaml` - Path to the base environment configuration file
    /// * `env_specific_yaml` - Path to the environment-specific configuration file
    pub fn new(product_name: String, base_yaml: &str, env_specific_yaml: &str) -> Self {
        info!("Loading environment definitions for {product_name}");
        info!("  Base config: {base_yaml}");
        info!("  Environment override: {env_specific_yaml}");
        let product_dir = PathBuf::from(base_yaml).parent().unwrap().to_path_buf();

        let base_components = Self::load_components(base_yaml, true);
        info!(
            "  Loaded {} components from base config",
            base_components.len()
        );
        for (comp_name, vars) in &base_components {
            debug!("    Component '{}': {} variables", comp_name, vars.len());
        }

        let env_components = Self::load_components(env_specific_yaml, false);
        if env_components.is_empty() {
            info!("  No environment-specific overrides loaded (file may not exist or be empty)");
        } else {
            info!(
                "  Loaded {} components from environment override",
                env_components.len()
            );
            for (comp_name, vars) in &env_components {
                debug!(
                    "    Component '{}': {} variable overrides",
                    comp_name,
                    vars.len()
                );
                for var_name in vars.keys() {
                    debug!("      - {}", var_name);
                }
            }
        }

        let components = Self::merge_components(base_components, env_components);

        info!(
            "  Merged configuration: {} total components",
            components.len()
        );
        Self {
            product_name,
            components,
            product_dir,
        }
    }

    /// Loads component configurations from a YAML file
    ///
    /// # Arguments
    ///
    /// * `yaml_path` - Path to the YAML configuration file
    /// * `is_base` - Whether this is the base configuration file
    fn load_components(
        yaml_path: &str,
        is_base: bool,
    ) -> HashMap<String, HashMap<String, GenerationMethod>> {
        let file_type = if is_base { "base" } else { "environment override" };
        debug!("Loading {} components from {yaml_path}", file_type);

        match File::open(yaml_path) {
            Ok(mut file) => {
                let mut contents = String::new();
                match file.read_to_string(&mut contents) {
                    Ok(_) => {
                        // Check if file is empty or only whitespace
                        if contents.trim().is_empty() {
                            debug!("File '{yaml_path}' is empty");
                            return HashMap::new();
                        }

                        match serde_yaml::from_str(&contents) {
                            Ok(parsed_components) => {
                                debug!(
                                    "Successfully parsed {} component configurations from {yaml_path}",
                                    file_type
                                );
                                parsed_components
                            }
                            Err(e) => {
                                if is_base {
                                    error!("Unable to parse YAML file '{yaml_path}': {e}");
                                    panic!("Unable to parse base YAML file: {e}");
                                } else {
                                    // More prominent warning for env-specific file parse failures
                                    error!(
                                        "Failed to parse environment override file '{yaml_path}': {e}"
                                    );
                                    error!(
                                        "Environment-specific settings will NOT be applied! Check YAML syntax."
                                    );
                                    error!(
                                        "Expected format: component_name:\\n  VAR_NAME: !Static \"value\""
                                    );
                                    HashMap::new()
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if is_base {
                            error!("Unable to read YAML file '{yaml_path}': {e}");
                            panic!("Unable to read base YAML file: {e}");
                        } else {
                            warn!(
                                "Unable to read YAML file '{yaml_path}': {e}. Ignoring specialization."
                            );
                            HashMap::new()
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if is_base {
                    error!("Base YAML file not found: '{yaml_path}'");
                    panic!("Base YAML file not found: {e}");
                } else {
                    // File not found is expected for env-specific files that don't exist
                    debug!(
                        "Environment override file '{yaml_path}' not found (this is OK if not needed)"
                    );
                    HashMap::new()
                }
            }
            Err(e) => {
                if is_base {
                    error!("Unable to open YAML file '{yaml_path}': {e}");
                    panic!("Unable to open base YAML file: {e}");
                } else {
                    warn!(
                        "Unable to open YAML file '{yaml_path}': {e}. Ignoring specialization."
                    );
                    HashMap::new()
                }
            }
        }
    }

    /// Merges base and environment-specific component configurations
    ///
    /// # Arguments
    ///
    /// * `base` - Base component configurations
    /// * `specialization` - Environment-specific component configurations
    fn merge_components(
        mut base: HashMap<String, HashMap<String, GenerationMethod>>,
        specialization: HashMap<String, HashMap<String, GenerationMethod>>,
    ) -> HashMap<String, ComponentEnvironment> {
        trace!("Merging base and environment-specific component configurations");

        // Overlay specialization on top of base
        for (component_name, env_vars) in specialization {
            match base.get_mut(&component_name) {
                Some(base_env_vars) => {
                    // Merge env vars for existing component
                    base_env_vars.extend(env_vars);
                }
                None => {
                    // Add new component
                    base.insert(component_name, env_vars);
                }
            }
        }

        // Convert to ComponentEnvironment structs
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

    /// Adds a new component to the environment definitions
    ///
    /// # Arguments
    ///
    /// * `component_name` - The name of the component to add
    pub fn add_component(&mut self, component_name: String) {
        debug!("Adding component: {component_name}");
        self.components.insert(
            component_name,
            ComponentEnvironment {
                environment_variables: HashMap::new(),
            },
        );
    }

    /// Adds an environment variable to a component
    ///
    /// # Arguments
    ///
    /// * `component_name` - The name of the component
    /// * `variable_name` - The name of the environment variable
    /// * `generation_method` - Method for generating the value
    pub fn add_environment_variable(
        &mut self,
        component_name: &str,
        variable_name: String,
        generation_method: GenerationMethod,
    ) {
        if let Some(component) = self.components.get_mut(component_name) {
            debug!("Adding environment variable '{variable_name}' to component '{component_name}'");
            component
                .environment_variables
                .insert(variable_name, generation_method);
        } else {
            error!("Component '{component_name}' not found");
            panic!("Component '{component_name}' not found");
        }
    }

    /// Generates a value for an environment variable
    ///
    /// # Arguments
    ///
    /// * `component_name` - The name of the component
    /// * `variable_name` - The name of the environment variable
    ///
    /// # Returns
    ///
    /// The generated value or None if the variable doesn't exist
    pub fn generate_value(&self, component_name: &str, variable_name: &str) -> Option<String> {
        if let Some(component) = self.components.get(component_name) {
            if let Some(generation_method) = component.environment_variables.get(variable_name) {
                match generation_method {
                    GenerationMethod::Static(value) => {
                        trace!("Using static value for {component_name}.{variable_name}");
                        Some(value.clone())
                    }
                    GenerationMethod::Ask(prompt) => {
                        print!("{prompt}: ");
                        std::io::stdout().flush().unwrap();
                        let mut input = String::new();
                        if std::io::stdin().read_line(&mut input).is_ok() {
                            Some(input.trim().to_string())
                        } else {
                            error!("Failed to read input for {component_name}.{variable_name}");
                            None
                        }
                    }
                    GenerationMethod::AskWithDefault(prompt, default) => {
                        print!("{prompt} (default: {default}): ");
                        std::io::stdout().flush().unwrap();
                        let mut input = String::new();
                        if std::io::stdin().read_line(&mut input).is_ok() {
                            let input = input.trim();
                            if input.is_empty() {
                                Some(default.clone())
                            } else {
                                Some(input.to_string())
                            }
                        } else {
                            error!("Failed to read input for {component_name}.{variable_name}");
                            None
                        }
                    }
                    GenerationMethod::Timestamp(format) => {
                        let now = chrono::Local::now();
                        Some(now.format(format).to_string())
                    }
                }
            } else {
                trace!("Variable '{variable_name}' not found in component '{component_name}'");
                None
            }
        } else {
            trace!("Component '{component_name}' not found");
            None
        }
    }

    /// Generates dotenv files for all components
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub fn generate_dotenv_files(&self) -> Result<(), std::io::Error> {
        info!("Generating .env files for product: {}", self.product_name);

        // Read stack spec to find component locations
        let stack_yaml_path = self.product_dir.join("stack.spec.yaml");
        let stack_yaml_content = fs::read_to_string(&stack_yaml_path)?;
        let stack_yaml: serde_yaml::Value = serde_yaml::from_str(&stack_yaml_content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Process each component
        if let Some(components_map) = stack_yaml.as_mapping() {
            for (component_name, component_info) in components_map {
                if let (Some(component_name), Some(location)) = (
                    component_name.as_str(),
                    component_info.get("location").and_then(|v| v.as_str()),
                ) {
                    let component_dir = self.product_dir.join(location);
                    if !component_dir.exists() {
                        warn!("Component '{component_name}' directory not found, skipping");
                        continue;
                    }

                    self.process_component_env_file(component_name, &component_dir)?;
                }
            }
        }

        info!("Successfully generated all environment files");
        Ok(())
    }

    /// Processes the .env file for a specific component
    ///
    /// # Arguments
    ///
    /// * `component_name` - The name of the component
    /// * `component_dir` - The directory of the component
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    fn process_component_env_file(
        &self,
        component_name: &str,
        component_dir: &Path,
    ) -> Result<(), std::io::Error> {
        let env_path = component_dir.join(".env");

        // Get the component configuration
        if let Some(component) = self.components.get(component_name) {
            // Read existing .env file if it exists
            let mut env_map = if env_path.exists() {
                debug!("Reading existing .env file for component '{component_name}'");
                self.load_dotenv(&env_path)?
            } else {
                debug!("Creating new .env file for component '{component_name}'");
                HashMap::new()
            };

            // Generate and add each variable
            for (var_name, generation_method) in &component.environment_variables {
                // Only add if not already present or if it's a static value
                if !env_map.contains_key(var_name)
                    || matches!(generation_method, GenerationMethod::Static(_))
                {
                    if let Some(value) = self.generate_value(component_name, var_name) {
                        trace!("Setting {}={} in {}", var_name, value, env_path.display());
                        env_map.insert(var_name.clone(), value);
                    } else {
                        error!("Failed to generate value for {component_name}.{var_name}");
                    }
                }
            }

            // Write the updated .env file
            self.save_dotenv(&env_path, env_map)?;
            debug!("Saved .env file for component '{component_name}'");
        }

        Ok(())
    }

    /// Loads environment variables from a .env file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the .env file
    ///
    /// # Returns
    ///
    /// A map of environment variable names to values, or an error
    fn load_dotenv(&self, path: &Path) -> Result<HashMap<String, String>, std::io::Error> {
        trace!("Loading .env file: {}", path.display());
        let file = File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let mut env_map = HashMap::new();

        for line in reader.lines() {
            let line = line?;
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse key=value pairs
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                // Handle quoted values
                let value = if (value.starts_with('"') && value.ends_with('"'))
                    || (value.starts_with('\'') && value.ends_with('\''))
                {
                    &value[1..value.len() - 1]
                } else {
                    value
                };

                env_map.insert(key.to_string(), value.to_string());
            }
        }

        debug!(
            "Loaded {} environment variables from {}",
            env_map.len(),
            path.display()
        );
        Ok(env_map)
    }

    /// Saves environment variables to a .env file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the .env file
    /// * `env_map` - Map of environment variable names to values
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    fn save_dotenv(
        &self,
        path: &Path,
        env_map: HashMap<String, String>,
    ) -> Result<(), std::io::Error> {
        trace!("Saving .env file: {}", path.display());
        let mut file = File::create(path)?;

        for (key, value) in &env_map {
            writeln!(file, "{key}=\"{value}\"")?;
        }

        debug!(
            "Saved {} environment variables to {}",
            env_map.len(),
            path.display()
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_merge_components() {
        // Base components
        let mut base = HashMap::new();
        let mut base_comp1 = HashMap::new();
        base_comp1.insert(
            "BASE_VAR".to_string(),
            GenerationMethod::Static("base_value".to_string()),
        );
        base.insert("component1".to_string(), base_comp1);

        // Environment-specific components
        let mut env_specific = HashMap::new();
        let mut env_comp1 = HashMap::new();
        env_comp1.insert(
            "ENV_VAR".to_string(),
            GenerationMethod::Static("env_value".to_string()),
        );
        env_specific.insert("component1".to_string(), env_comp1);

        let mut env_comp2 = HashMap::new();
        env_comp2.insert(
            "NEW_COMP_VAR".to_string(),
            GenerationMethod::Static("new_comp_value".to_string()),
        );
        env_specific.insert("component2".to_string(), env_comp2);

        // Merge
        let merged = EnvironmentDefinitions::merge_components(base, env_specific);

        // Verify results
        assert_eq!(merged.len(), 2);

        // Check component1
        let comp1 = merged.get("component1").unwrap();
        assert_eq!(comp1.environment_variables.len(), 2);
        assert!(matches!(
            comp1.environment_variables.get("BASE_VAR").unwrap(),
            GenerationMethod::Static(val) if val == "base_value"
        ));
        assert!(matches!(
            comp1.environment_variables.get("ENV_VAR").unwrap(),
            GenerationMethod::Static(val) if val == "env_value"
        ));

        // Check component2
        let comp2 = merged.get("component2").unwrap();
        assert_eq!(comp2.environment_variables.len(), 1);
        assert!(matches!(
            comp2.environment_variables.get("NEW_COMP_VAR").unwrap(),
            GenerationMethod::Static(val) if val == "new_comp_value"
        ));
    }

    #[test]
    fn test_load_and_save_dotenv() {
        let temp_dir = TempDir::new().unwrap();
        let env_path = temp_dir.path().join(".env");

        // Create a test .env file
        let mut file = File::create(&env_path).unwrap();
        writeln!(file, "KEY1=\"value1\"").unwrap();
        writeln!(file, "KEY2=\"value2\"").unwrap();
        writeln!(file, "# Comment").unwrap();
        writeln!(file, "KEY3=value3").unwrap();

        // Create a minimal EnvironmentDefinitions instance
        let env_defs = EnvironmentDefinitions {
            product_name: "test".to_string(),
            components: HashMap::new(),
            product_dir: temp_dir.path().to_path_buf(),
        };

        // Load the .env file
        let env_map = env_defs.load_dotenv(&env_path).unwrap();

        // Verify loaded values
        assert_eq!(env_map.len(), 3);
        assert_eq!(env_map.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(env_map.get("KEY2"), Some(&"value2".to_string()));
        assert_eq!(env_map.get("KEY3"), Some(&"value3".to_string()));

        // Modify and save
        let mut modified_map = env_map.clone();
        modified_map.insert("KEY4".to_string(), "value4".to_string());
        modified_map.remove("KEY2");

        let new_env_path = temp_dir.path().join(".env.new");
        env_defs.save_dotenv(&new_env_path, modified_map).unwrap();

        // Verify saved file
        let saved_map = env_defs.load_dotenv(&new_env_path).unwrap();
        assert_eq!(saved_map.len(), 3);
        assert_eq!(saved_map.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(saved_map.get("KEY3"), Some(&"value3".to_string()));
        assert_eq!(saved_map.get("KEY4"), Some(&"value4".to_string()));
        assert!(!saved_map.contains_key("KEY2"));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::path::PathBuf;
    
    use rush_cli::core::Config;
    use rush_cli::build::Variables;
    
    #[test]
    fn test_minimal_config() {
        // Create a minimal test config
        let config = Config {
            product_name: "test-product".to_string(),
            product_uri: "test.app".to_string(),
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
        };
        
        assert_eq!(config.product_name(), "test-product");
        assert_eq!(config.environment(), "dev");
        
        // Create test variables using the new() method
        let variables_arc = Variables::new("/nonexistent/path", "dev");
        
        // Get a reference to the variables
        let variables = Arc::as_ref(&variables_arc);
        
        // Verify the environment is set correctly
        assert_eq!(variables.env, "dev");
    }
}
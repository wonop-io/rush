#[cfg(test)]
mod tests {
    
    
    use std::sync::Arc;

    use rush_build::Variables;
    use rush_config::Config;

    #[test]
    fn test_minimal_config() {
        // Create a minimal test config
        let config = Config::test_default();

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

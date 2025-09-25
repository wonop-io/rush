//! Tests for Docker registry configuration

#[cfg(test)]
mod tests {
    use crate::reactor::modular_core::{ModularReactorConfig, RegistryConfig};

    #[test]
    fn test_registry_config_defaults() {
        let config = RegistryConfig::default();

        assert_eq!(config.url, None);
        assert_eq!(config.namespace, None);
        assert_eq!(config.username, None);
        assert_eq!(config.password, None);
        assert!(config.use_credentials_helper);
    }

    #[test]
    fn test_registry_config_with_credentials() {
        let config = RegistryConfig {
            url: Some("gcr.io".to_string()),
            namespace: Some("my-project".to_string()),
            username: Some("user@example.com".to_string()),
            password: Some("secret-password".to_string()),
            use_credentials_helper: false,
        };

        assert_eq!(config.url, Some("gcr.io".to_string()));
        assert_eq!(config.namespace, Some("my-project".to_string()));
        assert_eq!(config.username, Some("user@example.com".to_string()));
        assert_eq!(config.password, Some("secret-password".to_string()));
        assert!(!config.use_credentials_helper);
    }

    #[test]
    fn test_modular_config_includes_registry() {
        let mut config = ModularReactorConfig::default();
        config.registry.url = Some("docker.io".to_string());
        config.registry.namespace = Some("myorg".to_string());

        assert_eq!(config.registry.url, Some("docker.io".to_string()));
        assert_eq!(config.registry.namespace, Some("myorg".to_string()));
    }

    #[test]
    fn test_registry_tag_formatting() {
        // Test various registry URL formats
        let test_cases = vec![
            (
                "gcr.io",
                Some("my-project"),
                "app:v1.0",
                "gcr.io/my-project/app:v1.0",
            ),
            (
                "docker.io",
                Some("library"),
                "nginx:latest",
                "docker.io/library/nginx:latest",
            ),
            (
                "localhost:5000",
                None,
                "test:dev",
                "localhost:5000/test:dev",
            ),
            ("", Some("myorg"), "service:1.2.3", "myorg/service:1.2.3"),
        ];

        for (registry, namespace, image, expected) in test_cases {
            let result = format_registry_tag(
                Some(registry.to_string()),
                namespace.map(|s| s.to_string()),
                image,
            );
            assert_eq!(
                result, expected,
                "Failed for registry={:?}, namespace={:?}, image={}",
                registry, namespace, image
            );
        }
    }

    /// Helper function to format registry tags
    fn format_registry_tag(url: Option<String>, namespace: Option<String>, image: &str) -> String {
        match (url.as_ref(), namespace.as_ref()) {
            (Some(u), Some(n)) if !u.is_empty() => format!("{}/{}/{}", u, n, image),
            (Some(u), None) if !u.is_empty() => format!("{}/{}", u, image),
            (Some(u), Some(n)) if u.is_empty() => format!("{}/{}", n, image),
            (None, Some(n)) => format!("{}/{}", n, image),
            _ => image.to_string(),
        }
    }
}

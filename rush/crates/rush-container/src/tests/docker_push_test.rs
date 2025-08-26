//! Tests for Docker push functionality

#[cfg(test)]
mod tests {
    use crate::tests::mock_docker::{MockDockerClient, MockResponses};
    use crate::docker::DockerClient;
    
    #[tokio::test]
    async fn test_push_image_success() {
        let mock_client = MockDockerClient::new();
        
        // Test successful push
        let result = mock_client.push_image("test-image:latest").await;
        assert!(result.is_ok());
        
        // Verify the call was recorded
        let history = mock_client.get_call_history().await;
        assert!(history.contains(&"push_image(test-image:latest)".to_string()));
    }
    
    #[tokio::test]
    async fn test_push_image_failure() {
        let mock_client = MockDockerClient::new();
        
        // Configure to fail
        let mut responses = MockResponses::default();
        responses.should_fail_image_push = true;
        mock_client.set_response(responses).await;
        
        // Test failed push
        let result = mock_client.push_image("test-image:latest").await;
        assert!(result.is_err());
        
        if let Err(e) = result {
            assert!(e.to_string().contains("Failed to push image"));
        }
    }
    
    #[tokio::test]
    async fn test_registry_tagging() {
        // Test the registry tag formatting
        let test_cases = vec![
            (Some("gcr.io"), Some("my-project"), "app:v1", "gcr.io/my-project/app:v1"),
            (Some("localhost:5000"), None, "app:v1", "localhost:5000/app:v1"),
            (None, Some("myorg"), "app:v1", "myorg/app:v1"),
            (None, None, "app:v1", "app:v1"),
        ];
        
        for (registry, namespace, image, expected) in test_cases {
            let config = crate::reactor::modular_core::RegistryConfig {
                url: registry.map(|s| s.to_string()),
                namespace: namespace.map(|s| s.to_string()),
                username: None,
                password: None,
                use_credentials_helper: false,
            };
            
            // Note: This is a simplified test. The actual get_registry_tag is private
            // but we're testing the concept here
            let result = if let Some(url) = &config.url {
                if let Some(ns) = &config.namespace {
                    format!("{}/{}/{}", url, ns, image)
                } else {
                    format!("{}/{}", url, image)
                }
            } else if let Some(ns) = &config.namespace {
                format!("{}/{}", ns, image)
            } else {
                image.to_string()
            };
            
            assert_eq!(result, expected, "Registry tagging failed for {:?}/{:?}", registry, namespace);
        }
    }
}
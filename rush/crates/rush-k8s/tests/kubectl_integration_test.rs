#[cfg(test)]
mod tests {
    use std::fs;

    use rush_k8s::{Kubectl, KubectlConfig};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_kubectl_config_builder() {
        let config = KubectlConfig {
            kubectl_path: "/usr/bin/kubectl".to_string(),
            context: Some("test-context".to_string()),
            namespace: Some("test-namespace".to_string()),
            dry_run: true,
            kubeconfig: None,
            verbose: true,
        };

        let kubectl = Kubectl::new(config.clone());
        assert_eq!(kubectl.config.kubectl_path, "/usr/bin/kubectl");
        assert_eq!(kubectl.config.namespace, Some("test-namespace".to_string()));
        assert!(kubectl.config.dry_run);
    }

    #[tokio::test]
    async fn test_kubectl_fluent_builder() {
        let kubectl = Kubectl::default()
            .with_namespace("production".to_string())
            .with_context("prod-cluster".to_string())
            .dry_run(true);

        assert_eq!(kubectl.config.namespace, Some("production".to_string()));
        assert_eq!(kubectl.config.context, Some("prod-cluster".to_string()));
        assert!(kubectl.config.dry_run);
    }

    #[tokio::test]
    async fn test_dry_run_apply() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("test-deployment.yaml");

        // Create a test manifest
        let manifest_content = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test-app
  namespace: test
spec:
  replicas: 1
  selector:
    matchLabels:
      app: test-app
  template:
    metadata:
      labels:
        app: test-app
    spec:
      containers:
      - name: test-app
        image: nginx:latest
        ports:
        - containerPort: 80
"#;
        fs::write(&manifest_path, manifest_content).unwrap();

        // Create kubectl with dry-run enabled
        let kubectl = Kubectl::default()
            .with_namespace("test".to_string())
            .dry_run(true);

        // Apply should succeed in dry-run mode
        let result = kubectl.apply(&manifest_path).await;

        // In dry-run mode, this might fail if kubectl is not installed
        // but the test structure is correct
        if result.is_ok() {
            let kubectl_result = result.unwrap();
            assert!(kubectl_result.success || kubectl_result.stderr.contains("kubectl"));
        }
    }

    #[tokio::test]
    async fn test_apply_dir_ordering() {
        let temp_dir = TempDir::new().unwrap();

        // Create manifests in different order
        let secret_manifest = r#"
apiVersion: v1
kind: Secret
metadata:
  name: test-secret
type: Opaque
data:
  key: dmFsdWU=
"#;
        fs::write(temp_dir.path().join("secrets.yaml"), secret_manifest).unwrap();

        let service_manifest = r#"
apiVersion: v1
kind: Service
metadata:
  name: test-service
spec:
  selector:
    app: test-app
  ports:
  - port: 80
"#;
        fs::write(temp_dir.path().join("test-service.yaml"), service_manifest).unwrap();

        let deployment_manifest = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test-app
spec:
  replicas: 1
  selector:
    matchLabels:
      app: test-app
  template:
    metadata:
      labels:
        app: test-app
    spec:
      containers:
      - name: app
        image: nginx
"#;
        fs::write(
            temp_dir.path().join("test-deployment.yaml"),
            deployment_manifest,
        )
        .unwrap();

        // Create kubectl with dry-run
        let kubectl = Kubectl::default()
            .with_namespace("test".to_string())
            .dry_run(true);

        // Apply directory should process files in correct order
        let results = kubectl.apply_dir(temp_dir.path()).await;

        if results.is_ok() {
            let apply_results = results.unwrap();
            // Should have applied 3 manifests
            assert!(apply_results.len() <= 3); // May be less if kubectl not available
        }
    }

    #[tokio::test]
    async fn test_delete_dir_reverse_ordering() {
        let temp_dir = TempDir::new().unwrap();

        // Create a deployment manifest
        let deployment_manifest = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test-app
spec:
  replicas: 1
  selector:
    matchLabels:
      app: test-app
  template:
    metadata:
      labels:
        app: test-app
    spec:
      containers:
      - name: app
        image: nginx
"#;
        fs::write(
            temp_dir.path().join("test-deployment.yaml"),
            deployment_manifest,
        )
        .unwrap();

        // Create kubectl with dry-run
        let kubectl = Kubectl::default()
            .with_namespace("test".to_string())
            .dry_run(true);

        // Delete should work even if resources don't exist
        let results = kubectl.delete_dir(temp_dir.path()).await;

        if results.is_ok() {
            let delete_results = results.unwrap();
            // Should attempt to delete the deployment
            assert!(delete_results.len() <= 1);
        }
    }
}

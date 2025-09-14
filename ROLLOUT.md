# Rush CLI Rollout Feature Implementation Guide

## Overview

The `rollout` command in Rush CLI is designed to deploy applications to staging or production environments using a GitOps workflow. It automates the process of building images, pushing them to a registry, generating Kubernetes manifests, and committing them to an infrastructure repository for deployment.

## Current State Analysis

### What Already Exists

The new codebase already has:

1. **Command Structure** (`rush/crates/rush-cli/src/commands/rollout.rs`):
   - Basic rollout command implementation
   - Integration with CLI context
   - Calls `reactor.rollout()` method

2. **Reactor Implementation** (`rush/crates/rush-container/src/reactor/modular_core.rs`):
   - Basic `rollout()` method that:
     - Stops existing containers
     - Builds components
     - Starts services
   - However, this is for local development, NOT GitOps deployment

3. **Infrastructure Repository Support** (`rush/crates/rush-k8s/src/infrastructure.rs`):
   - Full `InfrastructureRepo` implementation with:
     - Git operations (clone, pull, commit, push)
     - Manifest copying
     - Directory structure management

4. **Configuration Support** (`rush/crates/rush-config/src/types.rs`):
   - `infrastructure_repository` field in Config
   - Environment variable: `INFRASTRUCTURE_REPOSITORY`

5. **Security Infrastructure**:
   - **Vault System** (`rush/crates/rush-security/src/vault/`):
     - `DotenvVault` - reads secrets from `.env` files
     - `FileVault` - JSON-based secret storage (`.rush/vault/`)
     - `OnePassword` - integration with 1Password CLI
   - **Secret Encoders** (`rush/crates/rush-security/src/secrets/`):
     - `Base64SecretsEncoder` - encodes secrets in base64
     - `NoopEncoder` - passes secrets through unchanged
   - **K8s Secret Encoders** (`rush/crates/rush-k8s/src/encoder.rs`):
     - `SealedSecretsEncoder` - uses kubeseal for encrypted secrets

6. **Manifest Generation** (`rush/crates/rush-k8s/src/generator.rs`):
   - `ManifestGenerator` - generates K8s manifests from templates
   - Template-based manifest generation with Tera
   - Secrets injection into manifests

### What's Missing

The current `rollout()` implementation in the Reactor is incorrect for production deployments. It needs to be completely rewritten to match the reference implementation's GitOps workflow with proper secrets handling.

## Required Implementation Details

### 1. Update Reactor's rollout() Method

**Location**: `rush/crates/rush-container/src/reactor/modular_core.rs`

The current implementation needs to be replaced with a proper GitOps workflow that includes secrets handling:

```rust
pub async fn rollout(&mut self) -> Result<()> {
    info!("Starting GitOps rollout...");

    // Step 1: Build and push images to registry
    self.build_and_push().await?;

    // Step 2: Build Kubernetes manifests with secrets
    // Note: build_manifests() already handles:
    // - Fetching secrets from vault for each component
    // - Encoding secrets with the configured encoder (Base64 or Noop)
    // - Generating manifests with secrets injected
    // - Optionally applying SealedSecrets with kubeseal
    self.build_manifests().await?;

    // Step 3: Initialize infrastructure repository
    let infra_repo = self.create_infrastructure_repo()?;

    // Step 4: Checkout/clone infrastructure repository
    infra_repo.checkout().await?;

    // Step 5: Copy manifests to infrastructure repository
    let source_directory = self.k8s_manifest_dir
        .as_ref()
        .ok_or_else(|| Error::InvalidState("Manifests not built".into()))?;
    infra_repo.copy_manifests(source_directory).await?;

    // Step 6: Commit and push to trigger GitOps deployment
    let commit_message = format!(
        "Deploying {} for {}",
        self.config.base.environment,
        self.config.build.product_name
    );
    infra_repo.commit_and_push(&commit_message).await?;

    info!("GitOps rollout completed successfully");
    Ok(())
}
```

### 2. Add Infrastructure Repository Creation Method

**Location**: `rush/crates/rush-container/src/reactor/modular_core.rs`

Add a helper method to create the infrastructure repository:

```rust
fn create_infrastructure_repo(&self) -> Result<rush_k8s::infrastructure::InfrastructureRepo> {
    let config = rush_config::Config::load()?;
    let toolchain = Arc::new(rush_toolchain::ToolchainContext::default());

    let local_path = std::path::PathBuf::from(&self.config.base.root_path)
        .join(".infra");

    Ok(rush_k8s::infrastructure::InfrastructureRepo::new(
        config.infrastructure_repository().to_string(),
        local_path,
        self.config.base.environment.clone(),
        self.config.build.product_name.clone(),
        toolchain,
    ))
}
```

### 3. Add Required Dependencies

**Location**: `rush/crates/rush-container/Cargo.toml`

Ensure the following dependency is added:
```toml
rush-k8s = { path = "../rush-k8s" }
```

### 4. Update Reactor Struct

**Location**: `rush/crates/rush-container/src/reactor/modular_core.rs`

No struct changes needed - the `k8s_manifest_dir` field already exists and stores the manifest output directory.

### 5. Environment Configuration

The following environment variables need to be properly set:

- `INFRASTRUCTURE_REPOSITORY`: Git URL of the infrastructure repository
- `K8S_NAMESPACE`: Target Kubernetes namespace
- `DOCKER_REGISTRY`: Docker registry URL
- `RUSH_ENV`: Environment name (staging, production, etc.)

These should be configured in `rushd.yaml`:

```yaml
env:
  INFRASTRUCTURE_REPOSITORY: "git@github.com:your-org/infrastructure.git"
  K8S_NAMESPACE: "your-namespace"
  DOCKER_REGISTRY: "your-registry.com"
```

## GitOps Workflow

The rollout command implements the following GitOps workflow:

1. **Build Phase**:
   - Build all component images
   - Tag images with appropriate versions

2. **Push Phase**:
   - Push all built images to the configured Docker registry
   - Ensure images are accessible for Kubernetes deployments

3. **Manifest Generation with Secrets**:
   - Fetch secrets from configured vault (1Password, JSON, or .env)
   - Apply secret encoder (Base64SecretsEncoder or NoopEncoder)
   - Generate Kubernetes manifests from templates in `products/{product}/*/infrastructure/`
   - Inject secrets and environment variables into manifests
   - Optionally encode secrets with kubeseal if `K8S_USE_SEALED_SECRETS=true`
   - Output manifests to `.rush/k8s/` directory

4. **Infrastructure Repository Operations**:
   - Clone or update the infrastructure repository
   - Copy generated manifests to the appropriate directory:
     ```
     products/{product_name}/{environment}/
     ```
   - Stage all changes

5. **GitOps Trigger**:
   - Commit changes with descriptive message
   - Push to infrastructure repository
   - This triggers the GitOps operator (e.g., ArgoCD, Flux) to deploy

## Directory Structure

### Product Repository Structure
```
products/
├── {product_name}/
│   ├── stack.spec.yaml              # Component definitions
│   ├── stack.env.base.yaml          # Base environment variables
│   ├── stack.env.{env}.yaml         # Environment-specific variables
│   ├── stack.env.secrets.yaml       # Secret definitions
│   ├── {component}/
│   │   ├── infrastructure/          # K8s manifest templates
│   │   │   ├── 01_namespace.yaml
│   │   │   ├── 02_secrets.yaml
│   │   │   ├── 10_deployment.yaml
│   │   │   ├── 20_svc.yaml
│   │   │   └── 30_ingress.yaml
│   │   └── ...
```

### Generated Manifests (Local)
```
.rush/k8s/
├── {priority}_{component}/
│   ├── 01_namespace.yaml
│   ├── 02_secrets.yaml
│   ├── 10_deployment.yaml
│   ├── 20_svc.yaml
│   └── 30_ingress.yaml
```

### Infrastructure Repository Structure
```
infrastructure-repo/
├── products/
│   ├── {product_name}/
│   │   ├── {environment}/
│   │   │   ├── {priority}_{component}/
│   │   │   │   ├── 01_namespace.yaml
│   │   │   │   ├── 02_secrets.yaml
│   │   │   │   ├── 10_deployment.yaml
│   │   │   │   ├── 20_svc.yaml
│   │   │   │   └── 30_ingress.yaml
```

## Error Handling

The rollout process should handle the following error scenarios:

1. **Build Failures**: Stop if any component fails to build
2. **Registry Push Failures**: Retry with exponential backoff
3. **Git Operation Failures**:
   - Handle authentication issues
   - Handle network timeouts
   - Handle merge conflicts (fail and notify)
4. **Manifest Generation Failures**: Validate templates before generation

## Testing Requirements

### Unit Tests

1. Test infrastructure repository operations:
   - Git clone/pull
   - File copying
   - Commit/push operations

2. Test manifest generation:
   - Template rendering
   - Secret encoding
   - Output directory structure

### Integration Tests

1. Mock Docker registry interactions
2. Mock Git operations
3. Test full rollout workflow with test fixtures

## Implementation Checklist

- [ ] Update `rollout()` method in `rush/crates/rush-container/src/reactor/modular_core.rs`
- [ ] Add `create_infrastructure_repo()` helper method
- [ ] Add `rush-k8s` dependency to `rush-container/Cargo.toml`
- [ ] Add import for `rush_k8s::infrastructure::InfrastructureRepo`
- [ ] Test with local Git repository
- [ ] Test with remote Git repository
- [ ] Add unit tests for new functionality
- [ ] Add integration tests for rollout workflow
- [ ] Update CLI help text if needed
- [ ] Document environment variables in README

## Migration Notes

When migrating from the old implementation:

1. The core logic remains the same - only the integration points change
2. The `InfrastructureRepo` class already exists and matches the old implementation
3. The manifest generation is already implemented via `build_manifests()`
4. The main work is connecting these pieces in the correct sequence

## Security Considerations

### Vault System Integration

Rush supports multiple vault backends for secure secret management:

1. **1Password Integration**:
   - Set `vault_name: "1Password"` in rushd.yaml
   - Configure `one_password_account` in rushd.yaml
   - Uses 1Password CLI (`op`) for secure secret retrieval
   - Secrets never stored on disk in plain text

2. **JSON File Vault**:
   - Set `vault_name: "json"` in rushd.yaml
   - Configure `json_vault_dir` (default: `.rush/vault/`)
   - Stores secrets in JSON format locally
   - Should be excluded from version control

3. **Environment File Vault**:
   - Set `vault_name: ".env"` in rushd.yaml
   - Reads from `.env` files in product directory
   - Useful for local development

### Secret Encoding Pipeline

1. **Vault Retrieval**: Secrets fetched from configured vault
2. **Base64 Encoding**: Applied via `Base64SecretsEncoder` for K8s compatibility
3. **Manifest Injection**: Secrets injected into manifest templates
4. **SealedSecrets (Optional)**:
   - Set `K8S_USE_SEALED_SECRETS=true`
   - Applies `kubeseal` to encrypt secrets
   - Ensures secrets are encrypted at rest in Git

### Git Repository Security

1. **Authentication**:
   - Use SSH keys for Git operations
   - Configure SSH agent forwarding for CI/CD
   - Never embed credentials in code

2. **Secret Safety**:
   - Plain secrets never committed to infrastructure repo
   - Only SealedSecrets or references committed
   - Audit trail via Git history

3. **Registry Authentication**:
   - Docker credentials stored in vault
   - Service accounts for registry access
   - Temporary tokens where possible

## Future Enhancements

1. **Rollback Support**: Add ability to rollback to previous versions
2. **Dry Run Mode**: Preview changes without committing
3. **Approval Workflow**: Require manual approval for production deployments
4. **Deployment Status**: Check deployment status after GitOps trigger
5. **Multi-Environment Support**: Deploy to multiple environments in sequence
6. **Canary Deployments**: Support gradual rollouts
7. **Deployment Metrics**: Track deployment success/failure rates
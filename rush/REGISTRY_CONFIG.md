# Docker Registry Configuration Guide

## Overview
Rush now supports comprehensive Docker registry configuration for pushing images to public and private registries. This guide explains how to configure and use different registry types.

## Configuration Methods

### 1. Environment Variables

#### Global Configuration
Set these environment variables to configure registry access globally:

```bash
# Registry URL (optional, defaults to Docker Hub)
export DOCKER_REGISTRY="gcr.io"

# Registry namespace/organization
export DOCKER_REGISTRY_NAMESPACE="my-project"

# Authentication credentials
export DOCKER_REGISTRY_USERNAME="user@example.com"
export DOCKER_REGISTRY_PASSWORD="your-secure-password"
```

#### Environment-Specific Configuration
You can also set registry configuration per environment (dev, staging, prod):

```bash
# Development environment
export DEV_DOCKER_NAMESPACE="dev-project"
export DEV_DOCKER_USERNAME="dev-user"
export DEV_DOCKER_PASSWORD="dev-password"

# Production environment
export PROD_DOCKER_NAMESPACE="prod-project"
export PROD_DOCKER_USERNAME="prod-user"
export PROD_DOCKER_PASSWORD="prod-password"
```

### 2. Programmatic Configuration
When using Rush as a library, you can configure the registry programmatically:

```rust
use rush_container::reactor::factory::ModularReactorConfigBuilder;

let reactor = ModularReactorConfigBuilder::new()
    .with_registry(
        Some("gcr.io".to_string()),
        Some("my-project".to_string())
    )
    .with_registry_credentials(
        "user@example.com".to_string(),
        "secure-password".to_string()
    )
    .create_reactor(docker_client, component_specs)
    .await?;
```

## Registry Examples

### Docker Hub
```bash
export DOCKER_REGISTRY="docker.io"
export DOCKER_REGISTRY_NAMESPACE="myusername"
export DOCKER_REGISTRY_USERNAME="myusername"
export DOCKER_REGISTRY_PASSWORD="mypassword"
```

Images will be tagged as: `docker.io/myusername/image-name:tag`

### Google Container Registry (GCR)
```bash
export DOCKER_REGISTRY="gcr.io"
export DOCKER_REGISTRY_NAMESPACE="my-gcp-project"
export DOCKER_REGISTRY_USERNAME="_json_key"
export DOCKER_REGISTRY_PASSWORD="$(cat service-account-key.json)"
```

Images will be tagged as: `gcr.io/my-gcp-project/image-name:tag`

### Amazon ECR
```bash
export DOCKER_REGISTRY="123456789.dkr.ecr.us-west-2.amazonaws.com"
export DOCKER_REGISTRY_NAMESPACE="my-repo"
# ECR uses AWS credentials, typically handled by aws ecr get-login
```

Images will be tagged as: `123456789.dkr.ecr.us-west-2.amazonaws.com/my-repo/image-name:tag`

### GitHub Container Registry
```bash
export DOCKER_REGISTRY="ghcr.io"
export DOCKER_REGISTRY_NAMESPACE="myorg"
export DOCKER_REGISTRY_USERNAME="myusername"
export DOCKER_REGISTRY_PASSWORD="$GITHUB_TOKEN"
```

Images will be tagged as: `ghcr.io/myorg/image-name:tag`

### Local Registry
```bash
export DOCKER_REGISTRY="localhost:5000"
# No authentication needed for local registry
```

Images will be tagged as: `localhost:5000/image-name:tag`

## Security Best Practices

### 1. Never hardcode credentials
Always use environment variables or secret management systems for credentials.

### 2. Use service accounts
For CI/CD systems, use service accounts with limited permissions:
- GCR: Service account with `Storage Admin` role
- ECR: IAM user with ECR push permissions
- Docker Hub: Access tokens instead of passwords

### 3. Rotate credentials regularly
Set up automated credential rotation for production environments.

### 4. Use credentials helpers
Rush supports Docker credential helpers. When enabled (default), Docker will use the system's configured credential helper:

```bash
# For GCR
gcloud auth configure-docker

# For ECR
aws ecr get-login-password | docker login --username AWS --password-stdin $REGISTRY_URL

# For Azure
az acr login --name myregistry
```

## Workflow Example

### Development to Production Pipeline

1. **Development** - Push to dev registry:
```bash
export DEV_DOCKER_NAMESPACE="dev-team"
rush --env dev myapp build-and-push
```

2. **Staging** - Push to staging registry:
```bash
export STAGING_DOCKER_NAMESPACE="staging"
rush --env staging myapp build-and-push
```

3. **Production** - Push to production registry:
```bash
export PROD_DOCKER_NAMESPACE="production"
export PROD_DOCKER_USERNAME="prod-service-account"
export PROD_DOCKER_PASSWORD="$PROD_SECRET"
rush --env prod myapp build-and-push
```

## Troubleshooting

### Authentication Failed
- Verify credentials are correct
- Check if credentials have push permissions
- For GCR/ECR, ensure service accounts have proper IAM roles

### Network Timeout
- Check if registry URL is accessible
- Verify firewall/proxy settings
- For private registries, ensure VPN connection if required

### Image Not Found After Push
- Verify the full image tag is correct
- Check namespace/project permissions
- Ensure the registry URL format is correct

### Rate Limiting
Docker Hub has rate limits for anonymous and free users:
- Anonymous: 100 pulls per 6 hours
- Authenticated: 200 pulls per 6 hours
- Consider using a paid plan or alternative registry for production

## Testing Your Configuration

1. **Test authentication**:
```bash
docker login $DOCKER_REGISTRY -u $DOCKER_REGISTRY_USERNAME -p $DOCKER_REGISTRY_PASSWORD
```

2. **Test push manually**:
```bash
docker tag test-image:latest $DOCKER_REGISTRY/$DOCKER_REGISTRY_NAMESPACE/test-image:latest
docker push $DOCKER_REGISTRY/$DOCKER_REGISTRY_NAMESPACE/test-image:latest
```

3. **Test with Rush**:
```bash
rush myapp build-and-push
```

## Integration with CI/CD

### GitHub Actions
```yaml
env:
  DOCKER_REGISTRY: ghcr.io
  DOCKER_REGISTRY_NAMESPACE: ${{ github.repository_owner }}
  DOCKER_REGISTRY_USERNAME: ${{ github.actor }}
  DOCKER_REGISTRY_PASSWORD: ${{ secrets.GITHUB_TOKEN }}

steps:
  - name: Build and push
    run: rush myapp build-and-push
```

### GitLab CI
```yaml
variables:
  DOCKER_REGISTRY: $CI_REGISTRY
  DOCKER_REGISTRY_NAMESPACE: $CI_PROJECT_PATH
  DOCKER_REGISTRY_USERNAME: $CI_REGISTRY_USER
  DOCKER_REGISTRY_PASSWORD: $CI_REGISTRY_PASSWORD

build:
  script:
    - rush myapp build-and-push
```

### Jenkins
```groovy
withCredentials([usernamePassword(
    credentialsId: 'docker-registry-creds',
    usernameVariable: 'DOCKER_REGISTRY_USERNAME',
    passwordVariable: 'DOCKER_REGISTRY_PASSWORD'
)]) {
    sh 'rush myapp build-and-push'
}
```
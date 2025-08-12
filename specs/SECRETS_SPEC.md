# Rush Secrets Management Specification

## Overview

Rush's secrets management system provides a secure way to manage and generate secrets for different components across multiple environments. The system uses a vault abstraction to store secrets and a definition system to specify how secrets should be generated.

## Architecture

### Key Components

1. **SecretsDefinitions**: Manages secret definitions loaded from `stack.env.secrets.yaml`
2. **Vault**: Abstract interface for secret storage (FileVault, DotenvVault, 1Password, etc.)
3. **GenerationMethod**: Different ways to generate secret values
4. **Component Structure**: Secrets are organized by product → component → environment
   - Each component has its own set of secrets
   - Secrets are stored in the component's location directory (as defined in `stack.spec.yaml`)

## Command: `rush secrets init`

### Purpose
Initialize secrets for all components defined in a product's `stack.env.secrets.yaml` file.

### Requirements

#### 1. Product Context Required
- `rush` can be ran from anywhere in the git repository. It will identify the product path by first identifying the `rushd.yaml` file and then identifying the product path from the product name provided as a commandline argument.a
- **MUST** have a valid `stack.spec.yaml` file in the product directory
- **MUST** have a `stack.env.secrets.yaml` file defining the secrets

#### 2. Product Name Resolution
The product name is determined from:
- The positional argument if provided: `rush <product_name> secrets init`
- The current directory name if inside a products directory
- Example: If in `/products/io.wonop.helloworld/`, product name is `io.wonop.helloworld`

#### 3. Environment Selection
- Default environment is `local`
- Can be overridden with `--env` flag: `rush --env staging secrets init`
- Environment determines which vault backend to use (configured in rushd.yaml)

#### 4. Vault Creation
- Creates a vault for the product if it doesn't exist
- Vault type on depends the `rushd.yaml` configuration and the environment. `rushd.yaml` defines what vault is used for each environment. For instance, it could be:
  - `local`: FileVault (.env files)
  - `dev`: 1Password or FileVault
  - `staging/prod`: Cloud-based vaults

### Process Flow

1. **Load Definitions**: Read `stack.env.secrets.yaml` from product directory
2. **Load Component Specs**: Read `stack.spec.yaml` to get component locations
3. **Create Vault**: Ensure vault exists for the product
4. **Process Each Component**: For each component defined in `stack.env.secrets.yaml`:
   - Display component name as a header
   - Check existing secrets for that component
   - For each secret in the component:
     - If secret exists, show masked preview and ask if user wants to override
     - If not exists or override confirmed, generate new value based on generation method
   - Handle references to other component's secrets
5. **Store in Vault**: Save generated secrets to the appropriate location:
   - For DotenvVault: Creates `.env.secrets` file in each component's location directory
   - For other vaults: Stores in vault's structure

## Secret Definition File Format

### Location
`<product_directory>/stack.env.secrets.yaml`

### Structure
```yaml
<component_name>:
  <secret_name>: <generation_method>
  <secret_name>: <generation_method>
```

### Example
```yaml
backend:
  AUTH_SALT_SECRET: !RandomString 128
  DATABASE_URL: !AskWithDefault ["Enter database URL", "postgres://localhost/db"]
  API_KEY: !RandomAlphanumeric 32

frontend:
  API_URL: !Static "http://localhost:8080"
  SESSION_SECRET: !RandomBase64 64
```

## Generation Methods

### Static Values
- `!Static "value"` - Use a fixed value
- `!Base64EncodedStatic "value"` - Base64 encode a fixed value

### Interactive Input
- `!Ask "prompt"` - Prompt user for value
- `!AskWithDefault ["prompt", "default"]` - Prompt with default
- `!AskPassword "prompt"` - Hidden password input

### Random Generation
- `!RandomString <length>` - Random ASCII string
- `!RandomAlphanumeric <length>` - Random alphanumeric
- `!RandomHex <length>` - Random hexadecimal
- `!RandomBase64 <length>` - Random base64 encoded
- `!RandomUUID` - Generate UUID v4

### Special Methods
- `!Timestamp` - Current timestamp
- `!FromFile [ask_for_path, base64_encode, "default_path"]` - Read from file
- `!Ref "component.secret"` - Reference another secret

## Secret Storage

### DotenvVault Organization
For local development with DotenvVault, secrets are stored as `.env.secrets` files directly in each component's location directory:
```
products/
  <product_name>/
    <component1_location>/
      .env.secrets          # Component 1's secrets
    <component2_location>/
      .env.secrets          # Component 2's secrets
```

Example:
```
products/
  io.wonop.helloworld/
    backend/server/
      .env.secrets          # Backend secrets (AUTH_SALT_SECRET, DATABASE_URL, etc.)
    frontend/webui/
      .env.secrets          # Frontend secrets (API_KEY, SESSION_SECRET, etc.)
```

### Other Vault Organizations
For FileVault, 1Password, and cloud vaults:
```
<vault_root>/
  <product_name>/
    <environment>/
      <component_name>/
        secret_key=secret_value
        secret_key2=secret_value2
```

### Accessing Secrets
Secrets are loaded automatically when running Rush commands:
- `rush dev` - Loads `.env.secrets` from each component's location directory into container environment variables
- `rush build` - Makes secrets available as build-time environment variables
- `rush deploy` - Encodes secrets for Kubernetes deployment

The loading process:
1. ComponentSpec reads `.env.secrets` from the component's location path
2. These are merged with `.env` (public environment variables) if present
3. Secrets take precedence over public env vars if there are conflicts
4. All variables are passed to Docker containers as environment variables

## Implementation Details

### Context Requirements
The `secrets init` command requires:
1. **Product Name**: Identifies which product's secrets to initialize
2. **Environment**: Determines vault backend and storage location
3. **SecretsDefinitions**: Loaded from `stack.env.secrets.yaml`
4. **Vault Instance**: Created based on environment configuration

### Error Conditions
The command will fail if:
- Not run from a valid product directory
- `stack.env.secrets.yaml` is missing or malformed
- Vault backend is unavailable
- User cancels during interactive prompts

## Security Considerations

1. **Never commit secrets** to version control
2. **Use appropriate vault** for each environment
3. **Rotate secrets regularly** using override feature
4. **Limit access** to production vaults
5. **Audit secret usage** through vault logs

## Examples

### Initialize secrets for a product
```bash
rush helloworld.wonop.io secrets init
```
Output:
```
backend
=======
Secret 'AUTH_SALT_SECRET' [PFq****Ymv] already exists. Override? (y/N)
Secret 'DATABASE_URL' [pos****end] already exists. Override? (y/N)
Saving secrets for backend
  AUTH_SALT_SECRET: ***
  DATABASE_URL: ***

frontend
========
Generating new secret 'API_KEY'...
Generating new secret 'SESSION_SECRET'...
Saving secrets for frontend
  API_KEY: ***
  SESSION_SECRET: ***
```

### Initialize with specific environment
```bash
rush --env staging io.wonop.helloworld secrets init
```

### Override existing secrets
```bash
rush helloworld.wonop.io secrets init
# When prompted: "Secret 'API_KEY' already exists. Override? (y/N)"
# Type 'y' to regenerate
```

### Result
After running `secrets init`, each component will have a `.env.secrets` file in its location directory:
- `backend/server/.env.secrets` - Contains AUTH_SALT_SECRET, DATABASE_URL
- `frontend/webui/.env.secrets` - Contains API_KEY, SESSION_SECRET

## Secret Re-encoding for Different Targets

Rush automatically re-encodes secrets based on the deployment target and environment configuration. The re-encoding process ensures secrets are in the correct format for each system.

### Encoding Types

#### 1. NoopEncoder (Local Development with Docker)
- **Used for**: Local development with Docker containers
- **Format**: Plain text environment variables in `.env.secrets` files
- **Storage**: `.env.secrets` files in each component's location directory
- **Loading**: ComponentSpec automatically loads from `<component_location>/.env.secrets`
- **Access**: Passed to Docker containers as environment variables via `docker run -e`

Example `.env.secrets` file in `backend/server/`:
```
AUTH_SALT_SECRET="random128characterstring..."
DATABASE_URL="postgres://admin:admin@database:5432/backend"
```

Example `.env.secrets` file in `frontend/webui/`:
```
API_KEY="abc123xyz789"
SESSION_SECRET="base64encodedstring..."
```

Docker receives these as:
```bash
docker run -e AUTH_SALT_SECRET="..." -e DATABASE_URL="..." backend_image
```

#### 2. Base64SecretsEncoder (Kubernetes)
- **Used for**: Kubernetes Secret manifests
- **Format**: Base64 encoded values
- **Purpose**: Kubernetes requires secret data to be base64 encoded

Example Kubernetes Secret:
```yaml
apiVersion: v1
kind: Secret
metadata:
  name: backend-secrets
  namespace: myapp-dev
type: Opaque
data:
  AUTH_SALT_SECRET: cmFuZG9tMTI4Y2hhcmFjdGVyc3RyaW5nLi4u
  DATABASE_URL: cG9zdGdyZXM6Ly9hZG1pbjphZG1pbkBkYXRhYmFzZTo1NDMyL2JhY2tlbmQ=
  API_KEY: YWJjMTIzeHl6Nzg5
```

#### 3. SealedSecretsEncoder (Production Kubernetes)
- **Used for**: Production/staging Kubernetes deployments
- **Tool**: Bitnami Sealed Secrets (`kubeseal`)
- **Format**: Encrypted secrets that can be safely stored in Git
- **Purpose**: Allows secrets to be version controlled while remaining secure

Example Sealed Secret:
```yaml
apiVersion: bitnami.com/v1alpha1
kind: SealedSecret
metadata:
  name: backend-secrets
  namespace: myapp-prod
spec:
  encryptedData:
    AUTH_SALT_SECRET: AgBvF2Xk9w3Yt1... (encrypted)
    DATABASE_URL: AgCxM8Qp2v5Rt7... (encrypted)
```

### Configuration in rushd.yaml

The encoder for each environment is configured in `rushd.yaml`:

```yaml
env:
  K8S_ENCODER_LOCAL: noop          # Local: plain text
  K8S_ENCODER_DEV: kubeseal        # Dev: sealed secrets
  K8S_ENCODER_STAGING: kubeseal    # Staging: sealed secrets
  K8S_ENCODER_PROD: kubeseal       # Prod: sealed secrets
```

### Re-encoding Process

#### For Local Development (`rush dev`)
1. Secrets loaded from vault (e.g., `.env.secrets` files)
2. NoopEncoder passes secrets through unchanged
3. Secrets injected as environment variables in Docker containers

#### For Kubernetes Deployment (`rush deploy`)
1. Secrets loaded from vault
2. Manifest templates rendered with secret values
3. Secret manifests encoded based on environment:
   - Local: Base64 encoding only
   - Dev/Staging/Prod: Sealed Secrets encryption
4. Encoded manifests applied to cluster

### Encoder Selection Logic

```rust
// From rush-k8s/src/encoder.rs
pub fn create_encoder(encoder_type: &str) -> Box<dyn K8sEncoder> {
    match encoder_type {
        "kubeseal" => Box::new(SealedSecretsEncoder),
        "noop" => Box::new(NoopEncoder),
        _ => Box::new(NoopEncoder), // Default to noop
    }
}
```

### Secret Flow by Command

#### `rush secrets init`
1. Generate/collect secret values
2. Store in vault (format depends on vault type)
3. No encoding at this stage

#### `rush dev`
1. Load secrets from vault
2. Apply NoopEncoder (no transformation)
3. Pass as environment variables to containers

#### `rush build`
1. Load secrets from vault
2. Apply appropriate encoder for build environment
3. Make available as build-time environment variables

#### `rush deploy --env staging`
1. Load secrets from vault
2. Generate Kubernetes manifests with secrets
3. Apply SealedSecretsEncoder (kubeseal)
4. Deploy encrypted secrets to cluster

### Vault Storage Formats

Different vaults store secrets in different formats:

#### FileVault (`.env.secrets`)
```
KEY=value
KEY2=value2
```

#### 1Password Vault
Stored as secure notes with JSON structure:
```json
{
  "AUTH_SALT_SECRET": "value",
  "DATABASE_URL": "value"
}
```

#### Cloud Vaults (AWS Secrets Manager, Azure Key Vault)
Stored as key-value pairs in the cloud provider's format

### Best Practices

1. **Never commit plain secrets** - Only commit sealed/encrypted secrets
2. **Use appropriate encoder** - Match encoder to deployment target
3. **Rotate secrets regularly** - Use `rush secrets init` with override
4. **Separate vaults by environment** - Don't mix prod/dev secrets
5. **Audit encoder configuration** - Ensure production uses encryption

## Testing

To test the secrets system:

### 1. Setup
Create a test product with:
- `stack.spec.yaml` defining components and their locations
- `stack.env.secrets.yaml` defining secrets for each component

### 2. Initialize Secrets
```bash
rush <product> secrets init
```
- Should prompt for each component's secrets
- Should create `.env.secrets` in each component's location directory

### 3. Verify Storage
For DotenvVault (local development):
```bash
# Check backend secrets
cat products/<product>/backend/server/.env.secrets

# Check frontend secrets  
cat products/<product>/frontend/webui/.env.secrets
```

### 4. Test Loading in Development
```bash
rush <product> dev
```
- Verify containers receive environment variables
- Check with: `docker exec <container> env | grep SECRET`

### 5. Test Kubernetes Deployment
```bash
rush <product> deploy --env local
```
- Verify Secret manifests are created with base64 encoding
- Check generated YAML files for proper secret encoding

### 6. Test Production Deployment
```bash
rush <product> deploy --env prod
```
- Verify sealed secrets are properly encrypted with `kubeseal`
- Ensure no plain text secrets in generated manifests

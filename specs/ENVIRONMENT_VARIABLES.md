# Rush Environment Variables Specification

## Overview

Rush's environment variable system provides a way to manage configuration values for different components across multiple deployment environments (local, dev, staging, production). The system uses YAML files with special tags to define how environment variables are generated and manages them separately from secrets.

## Architecture

### Key Components

1. **EnvironmentGenerator** (rush-config): Simple generator that creates a single `.env` file at the product root
2. **EnvironmentDefinitions** (rush-security): Advanced generator that creates component-specific `.env` files
3. **GenerationMethod**: Different ways to generate environment variable values
4. **Component Structure**: Environment variables are organized by product → component

### File Structure

```
products/
  <product_name>/
    stack.env.base.yaml       # Base environment variables (all environments)
    stack.env.public.yaml     # Alternative name for base (legacy)
    stack.env.local.yaml      # Local environment overrides
    stack.env.dev.yaml        # Development environment overrides  
    stack.env.staging.yaml    # Staging environment overrides
    stack.env.prod.yaml       # Production environment overrides
    .env                      # Generated product-level env file (if using simple generator)
    
    <component1_location>/
      .env                    # Generated component-level env file
      .env.secrets           # Secrets file (managed separately)
    
    <component2_location>/
      .env                    # Generated component-level env file
      .env.secrets           # Secrets file (managed separately)
```

## Environment Variable Definition Files

### File Format

Environment variable files use YAML format with special tags to define generation methods:

```yaml
# stack.env.base.yaml or stack.env.public.yaml
backend:
  RUST_LOG: !Static "trace"
  PORT: !Static "8000"
  CLIENT_ORIGIN: !Static "http://localhost:9000"
  
frontend:
  API_URL: !Static "http://localhost:8000"
  NODE_ENV: !Static "development"
```

### Generation Methods

#### Static Values
- `!Static "value"` - Use a fixed value

#### Interactive Input
- `!Ask "prompt"` - Prompt user for value
- `!AskWithDefault ["prompt", "default"]` - Prompt with default value

#### Timestamps
- `!Timestamp "%Y-%m-%d %H:%M:%S"` - Current timestamp with format

### Environment-Specific Overrides

Environment-specific files override base values:

```yaml
# stack.env.prod.yaml
backend:
  RUST_LOG: !Static "error"  # Override trace with error for production
  CLIENT_ORIGIN: !Static "https://app.example.com"
  
frontend:
  API_URL: !Static "https://api.example.com"
  NODE_ENV: !Static "production"
```

## Generation Process

### Simple Generator (EnvironmentGenerator in rush-config)

Used for quick product-level environment setup:

1. **Load Base**: Read `stack.env.base.yaml` (or fallback to empty if not exists)
2. **Load Override**: Read `stack.env.<environment>.yaml` (or fallback to empty)
3. **Merge**: Override values take precedence over base values
4. **Save**: Write merged values to `<product_root>/.env`

**Note**: This generator creates a single `.env` file at the product root, not component-specific files.

### Advanced Generator (EnvironmentDefinitions in rush-security)

Used for component-specific environment files:

1. **Load Base**: Read `stack.env.base.yaml` or `stack.env.public.yaml`
2. **Load Override**: Read environment-specific file (e.g., `stack.env.local.yaml`)
3. **Merge Components**: For each component, merge environment-specific values over base
4. **Read Component Locations**: Parse `stack.spec.yaml` to find component directories
5. **Process Each Component**:
   - Check if component directory exists
   - Load existing `.env` file if present
   - Generate values using generation methods
   - Only override existing values if they're `!Static`
   - Save updated `.env` to component's location directory

### Value Generation

For each environment variable:
1. If value already exists in `.env` and generation method is not `!Static`, keep existing
2. If value doesn't exist or is `!Static`, generate new value:
   - `!Static`: Use the provided value
   - `!Ask`: Prompt user interactively
   - `!AskWithDefault`: Prompt with default option
   - `!Timestamp`: Generate current timestamp

## Loading Environment Variables

### During Development (`rush dev`)

1. **ComponentSpec** loads environment variables from multiple sources:
   - `.env` file from component's location directory (public env vars)
   - `.env.secrets` file from component's location directory (secrets)
   - `env` section from `stack.spec.yaml` (static component config)

2. **Merge Order** (later sources override earlier):
   - `.env` file values
   - `.env.secrets` file values  
   - `stack.spec.yaml` env section

3. **Docker Integration**:
   - All variables are passed to Docker containers via `-e` flags
   - Example: `docker run -e RUST_LOG="trace" -e PORT="8000" ...`

### During Build (`rush build`)

Environment variables are made available as build-time variables in the Docker build context.

### During Deployment (`rush deploy`)

Environment variables are encoded into Kubernetes ConfigMaps or included in deployment manifests.

## Relationship with Secrets

Environment variables (`.env`) and secrets (`.env.secrets`) are managed separately:

- **Environment Variables**: Configuration values that can be shared or committed
  - Stored in `.env` files
  - Defined in `stack.env.*.yaml` files
  - Can be safely committed to version control (no sensitive data)

- **Secrets**: Sensitive values that must be protected
  - Stored in `.env.secrets` files  
  - Defined in `stack.env.secrets.yaml`
  - Never committed to version control
  - Managed via `rush secrets init` command

Both are loaded and merged when running containers, with secrets taking precedence if there are conflicts.

## Examples

### Basic Setup

1. Create `stack.env.base.yaml`:
```yaml
backend:
  LOG_LEVEL: !Static "info"
  PORT: !Static "8000"
  
frontend:
  API_URL: !Static "http://localhost:8000"
```

2. Create `stack.env.local.yaml` for local overrides:
```yaml
backend:
  LOG_LEVEL: !Static "debug"  # More verbose for local dev
```

3. Run Rush commands:
```bash
# This happens automatically when running rush dev
# but shows the generation process
rush <product> dev
```

4. Result - `.env` files created:
- `backend/server/.env`: Contains LOG_LEVEL="debug", PORT="8000"
- `frontend/webui/.env`: Contains API_URL="http://localhost:8000"

### Interactive Variables

```yaml
database:
  DB_HOST: !Ask "Enter database host"
  DB_PORT: !AskWithDefault ["Enter database port", "5432"]
  MIGRATION_TIMESTAMP: !Timestamp "%Y%m%d%H%M%S"
```

When generated, will prompt:
```
Enter database host: postgres.example.com
Enter database port (default: 5432): [press enter for default]
```

### Environment-Specific Configuration

```yaml
# stack.env.base.yaml
backend:
  API_TIMEOUT: !Static "30"
  CACHE_TTL: !Static "3600"

# stack.env.prod.yaml  
backend:
  API_TIMEOUT: !Static "10"     # Stricter timeout in production
  CACHE_TTL: !Static "86400"    # Longer cache in production
  MONITORING: !Static "enabled"  # Additional prod-only variable
```

## Implementation Details

### File Loading Priority

1. Base configuration file (`stack.env.base.yaml` or `stack.env.public.yaml`)
2. Environment-specific override (`stack.env.<environment>.yaml`)
3. Existing `.env` files in component directories (preserved for non-static values)

### Error Handling

- Missing base file: Warning logged, continues with empty base
- Missing override file: Silent fallback to base values only
- Missing component directory: Warning logged, skips component
- Invalid YAML syntax: Error for base file, warning for override file

### Security Considerations

1. **No Secrets in Environment Files**: Never put passwords, API keys, or tokens in `stack.env.*.yaml`
2. **Use Secrets System**: Sensitive values belong in `stack.env.secrets.yaml`
3. **Environment Appropriate Values**: Use restrictive values in production (e.g., `LOG_LEVEL: error`)
4. **Version Control**: Safe to commit `stack.env.*.yaml` files, never commit `.env` or `.env.secrets`

## Testing

### Manual Testing

1. Create test environment files:
```bash
# Create base configuration
cat > products/<product>/stack.env.base.yaml << EOF
backend:
  TEST_VAR: !Static "base_value"
  PORT: !Static "8000"
EOF

# Create environment override
cat > products/<product>/stack.env.local.yaml << EOF  
backend:
  TEST_VAR: !Static "local_value"
EOF
```

2. Generate environment files:
```bash
rush <product> dev
```

3. Verify generated files:
```bash
cat products/<product>/backend/server/.env
# Should show: TEST_VAR="local_value" and PORT="8000"
```

4. Test in running container:
```bash
docker exec <container> env | grep TEST_VAR
# Should show: TEST_VAR=local_value
```

### Automated Testing

Both generators include comprehensive unit tests:
- `rush-config/src/environment/generator.rs`: Tests simple generation
- `rush-security/src/env_defs.rs`: Tests component-specific generation

## Migration Guide

### From Single .env to Component .env Files

If you have a single `.env` file at the product root:

1. Identify which variables belong to which component
2. Create `stack.env.base.yaml` with component sections
3. Remove the product-level `.env` file
4. Let Rush generate component-specific `.env` files

### From Manual .env Management

If you're manually managing `.env` files:

1. Extract common values to `stack.env.base.yaml`
2. Extract environment-specific values to `stack.env.<env>.yaml`
3. Use `!Static` tags for fixed values
4. Use `!Ask` or `!AskWithDefault` for values that vary per developer
5. Delete manually managed `.env` files and let Rush generate them

## Best Practices

1. **Use Base + Override Pattern**: Put common values in base, environment-specific in overrides
2. **Component Isolation**: Each component should have its own environment variables
3. **Meaningful Defaults**: Use `!AskWithDefault` with sensible defaults for better developer experience
4. **Environment Parity**: Keep environments as similar as possible, only change what's necessary
5. **Documentation**: Comment your YAML files to explain what each variable controls
6. **Validation**: Test environment files in all target environments before deployment
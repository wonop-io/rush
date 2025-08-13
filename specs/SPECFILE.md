# Rush Stack Specification File Format

## Overview

The `stack.spec.yaml` file defines the components that make up a Rush product. Each component represents a service, application, or infrastructure element that Rush will build, deploy, and manage.

## File Location

Each product must have a `stack.spec.yaml` file in its root directory:
```
products/
└── <product-name>/
    ├── stack.spec.yaml
    └── ... (component directories)
```

## Basic Structure

The spec file uses a flat YAML structure where each top-level key is a component name:

```yaml
<component-name>:
  build_type: <BuildType>
  <type-specific-fields>
  <common-fields>
```

## Build Types

### TrunkWasm
WebAssembly application built with Trunk:
```yaml
frontend:
  build_type: "TrunkWasm"
  location: "frontend/webui"           # Required: Source directory
  dockerfile: "frontend/Dockerfile"    # Required: Dockerfile path
  ssr: false                          # Optional: Server-side rendering
  features: ["feature1"]              # Optional: Rust features
  precompile_commands: ["cmd"]        # Optional: Pre-build commands
  mount_point: "/"                    # Optional: URL mount point
```

### RustBinary
Rust binary application:
```yaml
backend:
  build_type: "RustBinary"
  location: "backend/server"           # Required: Source directory
  dockerfile: "backend/Dockerfile"     # Required: Dockerfile path
  context_dir: "backend"               # Optional: Docker build context
  features: ["feature1"]               # Optional: Rust features
  precompile_commands: ["cmd"]        # Optional: Pre-build commands
  mount_point: "/api"                  # Optional: URL mount point
```

### DixiousWasm
Dioxus WebAssembly application:
```yaml
app:
  build_type: "DixiousWasm"
  location: "app/src"                  # Required: Source directory
  dockerfile: "app/Dockerfile"         # Required: Dockerfile path
```

### Script
Generic script-based build:
```yaml
service:
  build_type: "Script"
  location: "service"                  # Required: Source directory
  dockerfile: "service/Dockerfile"     # Required: Dockerfile path
  context_dir: "."                    # Optional: Docker build context
```

### Zola
Static site generator:
```yaml
docs:
  build_type: "Zola"
  location: "docs"                     # Required: Source directory
  dockerfile: "docs/Dockerfile"        # Required: Dockerfile path
  context_dir: "."                    # Optional: Docker build context
```

### Book
mdBook documentation:
```yaml
handbook:
  build_type: "Book"
  location: "handbook"                 # Required: Source directory
  dockerfile: "handbook/Dockerfile"    # Required: Dockerfile path
  context_dir: "."                    # Optional: Docker build context
```

### Ingress
Reverse proxy for routing:
```yaml
ingress:
  build_type: "Ingress"
  components: ["backend", "frontend"]  # Required: Components to route
  dockerfile: "ingress/Dockerfile"     # Required: Dockerfile path
  location: "./ingress"                # Optional: Config directory
  context_dir: "../target"             # Optional: Docker build context
  port: 9000                          # Required: External port
  target_port: 80                     # Required: Container port
```

### Image
Pre-built Docker image:
```yaml
database:
  build_type: "Image"
  image: "postgres:latest"             # Required: Docker image
  command: "custom-entrypoint"        # Optional: Override command
  entrypoint: "/bin/sh"              # Optional: Override entrypoint
  port: 5432                          # Optional: External port
  target_port: 5432                   # Optional: Container port
```

### LocalService
Persistent local development service (managed by Rush, not as a container):
```yaml
postgres:
  build_type: "LocalService"
  service_type: "postgresql"           # Required: Service type
  version: "15"                        # Optional: Service version
  persist_data: true                   # Required: Data persistence
  env:                                # Optional: Environment variables
    POSTGRES_USER: "user"
    POSTGRES_PASSWORD: "pass"
    POSTGRES_DB: "mydb"
    POSTGRES_PORT: "5432"            # Port configuration via env var
  health_check: "pg_isready -U user -p 5432"  # Optional: Health check command
  init_scripts: ["init.sql"]          # Optional: Initialization scripts
  depends_on: ["redis"]               # Optional: Service dependencies
  command: "postgres -c max_connections=200"  # Optional: Override command
```

Note: LocalService is a managed service where Rush decides the implementation details. The service may run as a Docker container, native executable, or other method - this is determined by Rush's implementation for each service type, not by user configuration. Users cannot specify Docker-related fields (image, volumes, docker_args) for LocalService. If custom Docker configuration is needed, use the `Image` build type instead. The `version` field allows specifying which version of the service to use. Ports should be configured through environment variables specific to each service type.

### K8sOnly
Kubernetes-only component (no container):
```yaml
config:
  build_type: "K8sOnly"
  k8s: "config/k8s"                   # Required: K8s manifest directory
```

### K8sInstall
Kubernetes installation package:
```yaml
monitoring:
  build_type: "K8sInstall"
  namespace: "monitoring"              # Required: Target namespace
```

## Common Fields

These fields can be used with most build types (see notes for exceptions):

```yaml
component:
  # Build configuration
  color: "blue"                       # Console output color
  priority: 50                        # Deployment priority (lower = earlier)
  depends_on: ["database"]            # Component dependencies
  cross_compile: "native"             # Cross-compilation method (native/cross-rs)
  
  # Networking
  port: 8080                          # External port
  target_port: 8080                   # Container port (not for LocalService)
  mount_point: "/api"                 # URL path for routing
  subdomain: "api"                    # Subdomain configuration
  
  # Environment
  env:                                # Environment variables
    KEY: "value"
    DATABASE_URL: "postgres://..."
  
  # Volumes (not available for LocalService)
  volumes:                            # Volume mappings (host:container)
    "./data": "/app/data"
    "./config.yaml": "/app/config.yaml"
  
  # Docker (not available for LocalService)
  docker_extra_run_args:              # Extra Docker run arguments
    - "--cap-add=SYS_ADMIN"
    - "--security-opt=apparmor:unconfined"
  
  # Kubernetes
  k8s: "backend/infrastructure"       # K8s manifest directory
  
  # Development
  watch:                              # File patterns to watch for changes
    - "src/**/*.rs"
    - "Cargo.toml"
  
  # Custom build
  build: "custom-build-script.sh"     # Override build script
  
  # Artifacts
  artefacts:                          # Template files to render
    "config.yaml": "config.yaml.tmpl"
  artefact_output_dir: "target/rushd" # Output directory for artifacts
```

## Variable Substitution

The spec file supports variable substitution using `{{variable}}` syntax:

```yaml
backend:
  build_type: "RustBinary"
  location: "backend/server"
  env:
    VERSION: "{{version}}"
    ENVIRONMENT: "{{environment}}"
```

Variables are defined in Rush configuration or passed via command line.

## Example: Complete Multi-Service Application

```yaml
# Frontend application
frontend:
  build_type: "TrunkWasm"
  location: "frontend/webui"
  dockerfile: "frontend/Dockerfile"
  mount_point: "/"
  color: "purple"
  k8s: "frontend/infrastructure"

# Backend API
backend:
  build_type: "RustBinary"
  location: "backend/server"
  dockerfile: "backend/Dockerfile"
  mount_point: "/api"
  port: 8080
  target_port: 8080
  color: "blue"
  k8s: "backend/infrastructure"
  env:
    DATABASE_URL: "postgresql://user:pass@database:5432/mydb"
  depends_on: ["database"]

# PostgreSQL database (managed service)
database:
  build_type: "LocalService"
  service_type: "postgresql"
  version: "15"
  persist_data: true
  env:
    POSTGRES_USER: "user"
    POSTGRES_PASSWORD: "pass"
    POSTGRES_DB: "mydb"
    POSTGRES_PORT: "5432"

# Redis cache (managed service)
cache:
  build_type: "LocalService"
  service_type: "redis"
  version: "7.2"
  persist_data: true
  env:
    REDIS_PORT: "6379"

# Ingress router
ingress:
  build_type: "Ingress"
  components: ["backend", "frontend"]
  dockerfile: "ingress/Dockerfile"
  port: 9000
  target_port: 80
  color: "green"
```

## Service Types for LocalService

Available service types for `LocalService` build type (implementation managed by Rush):

- **Databases**: `postgresql`, `mysql`, `mongodb`, `redis`
- **Storage**: `minio` (S3-compatible), `localstack` (AWS services)
- **Message Queues**: `rabbitmq`, `kafka`, `elasticmq` (SQS-compatible)
- **Development Tools**: `stripe-cli`, `mailhog` (email testing)
- **Custom**: Any string for custom services

**Important**: LocalService components are managed services where Rush controls the implementation. Rush may choose to run them as Docker containers, native processes, or other methods based on what's optimal for each service type. This abstraction provides:
- Data persistence through `persist_data` flag (Rush manages data directories)
- Support for various implementations (Docker, native executables, etc.)
- Simplified configuration - users specify intent, not implementation
- No Docker configuration exposed - use `Image` build type for custom Docker needs

**Port Configuration**: LocalService uses environment variables for port configuration instead of Docker port mappings. Each service type has its own standard environment variables:
- PostgreSQL: `POSTGRES_PORT`
- MySQL: `MYSQL_PORT`
- MongoDB: `MONGODB_PORT`
- Redis: `REDIS_PORT`
- MinIO: `MINIO_PORT`, `MINIO_CONSOLE_PORT`
- Custom services: Define your own port environment variables

## Best Practices

1. **Use flat structure**: All fields for a component should be at the same level
2. **Specify required fields**: Always include required fields for each build type
3. **Use LocalService for infrastructure**: Databases and caches should use LocalService for persistence
4. **Order by dependencies**: List components in dependency order when possible
5. **Use descriptive names**: Component names should clearly indicate their purpose
6. **Leverage templates**: Use variable substitution for environment-specific values
7. **Document mount points**: Clearly specify mount points for proper routing
8. **Set appropriate priorities**: Use priority field to control deployment order
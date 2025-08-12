# Rush Local Services Proposal

## Executive Summary

This proposal outlines the implementation of `rush-local-services`, a new crate designed to manage persistent local services that emulate cloud infrastructure (PostgreSQL, S3, SQS) and provide development tools (Stripe webhooks) for Rush-based applications. These services will start before the main application containers and persist across container restarts, providing a stable development environment.

## Problem Statement

Currently, Rush treats all services equally, restarting everything when file changes are detected. This creates several issues:

1. **Data Loss**: Database containers lose data on restart
2. **Slow Feedback Loop**: Waiting for infrastructure services to restart adds unnecessary delay
3. **Connection Issues**: Applications may fail to reconnect to restarting services
4. **Webhook Interruptions**: External services like Stripe lose webhook forwarding

## Proposed Solution

### Architecture Overview

```
┌─────────────────────────────────────────────────┐
│                 Rush CLI                         │
├─────────────────────────────────────────────────┤
│           rush-local-services                    │
│  ┌──────────────┐ ┌──────────────┐ ┌─────────┐ │
│  │  PostgreSQL  │ │    MinIO     │ │ Stripe  │ │
│  │              │ │     (S3)     │ │   CLI   │ │
│  └──────────────┘ └──────────────┘ └─────────┘ │
│  ┌──────────────┐ ┌──────────────┐             │
│  │     Redis    │ │  LocalStack  │             │
│  │              │ │  (SQS, SNS)  │             │
│  └──────────────┘ └──────────────┘             │
├─────────────────────────────────────────────────┤
│           rush-container (reactor)               │
│  ┌──────────────┐ ┌──────────────┐             │
│  │   Frontend   │ │   Backend    │             │
│  │  (restarts)  │ │  (restarts)  │             │
│  └──────────────┘ └──────────────┘             │
└─────────────────────────────────────────────────┘
```

### New BuildType: LocalService

Add a new variant to the `BuildType` enum:

```rust
pub enum BuildType {
    // ... existing variants ...
    
    /// A persistent local service for development
    LocalService {
        /// Service type (postgres, redis, minio, localstack, stripe-cli, etc.)
        service_type: LocalServiceType,
        /// Optional Docker image override
        image: Option<String>,
        /// Configuration specific to the service type
        config: LocalServiceConfig,
        /// Whether to persist data between runs
        persist_data: bool,
        /// Health check configuration
        health_check: Option<HealthCheckConfig>,
        /// Initialization scripts or commands
        init_scripts: Option<Vec<String>>,
        /// Dependencies on other local services
        depends_on: Option<Vec<String>>,
    },
}
```

### LocalServiceType Enum

```rust
pub enum LocalServiceType {
    // Databases
    PostgreSQL,
    MySQL,
    MongoDB,
    Redis,
    
    // AWS Services
    LocalStack,  // Provides S3, SQS, SNS, DynamoDB, etc.
    MinIO,       // S3-compatible storage
    ElasticMQ,   // SQS alternative
    
    // Development Tools
    StripeCLI,
    MailHog,     // Email testing
    
    // Custom
    Custom(String),
}
```

### LocalServiceConfig Structure

```rust
pub struct LocalServiceConfig {
    /// Port mappings
    pub ports: Vec<PortMapping>,
    
    /// Environment variables
    pub env: HashMap<String, String>,
    
    /// Volume mounts for persistence
    pub volumes: Vec<VolumeMapping>,
    
    /// Additional Docker run arguments
    pub docker_args: Option<Vec<String>>,
    
    /// Network configuration
    pub network_mode: Option<String>,
    
    /// Resource limits
    pub resources: Option<ResourceLimits>,
}
```

## Implementation Details

### 1. Service Lifecycle Management

The `rush-local-services` crate will implement a `LocalServiceManager` that:

```rust
pub struct LocalServiceManager {
    /// Docker client for service management
    docker_client: Arc<dyn DockerClient>,
    
    /// Running local services
    services: HashMap<String, LocalServiceHandle>,
    
    /// Service dependencies graph
    dependency_graph: DependencyGraph,
    
    /// Data persistence directory
    data_dir: PathBuf,
}

impl LocalServiceManager {
    /// Start all local services in dependency order
    pub async fn start_all(&mut self) -> Result<()>;
    
    /// Stop specific service (and dependents)
    pub async fn stop(&mut self, name: &str) -> Result<()>;
    
    /// Check health of all services
    pub async fn health_check(&self) -> Result<HealthStatus>;
    
    /// Initialize service data (run init scripts)
    pub async fn initialize(&mut self, name: &str) -> Result<()>;
    
    /// Backup service data
    pub async fn backup(&self, name: &str) -> Result<PathBuf>;
    
    /// Restore service data
    pub async fn restore(&mut self, name: &str, backup: PathBuf) -> Result<()>;
}
```

### 2. Configuration Examples

#### PostgreSQL Service

```yaml
# stack.spec.yaml
postgres:
  build_type: "LocalService"
  service_type: "PostgreSQL"
  persist_data: true
  config:
    ports:
      - "5432:5432"
    env:
      POSTGRES_DB: myapp
      POSTGRES_USER: developer
      POSTGRES_PASSWORD: localdev
    volumes:
      - "./data/postgres:/var/lib/postgresql/data"
  health_check:
    command: "pg_isready -U developer"
    interval: 5s
    retries: 10
  init_scripts:
    - "./scripts/init-db.sql"
```

#### MinIO (S3) Service

```yaml
minio:
  build_type: "LocalService"
  service_type: "MinIO"
  persist_data: true
  config:
    ports:
      - "9000:9000"  # S3 API
      - "9001:9001"  # Console
    env:
      MINIO_ROOT_USER: minioadmin
      MINIO_ROOT_PASSWORD: minioadmin
    volumes:
      - "./data/minio:/data"
  health_check:
    command: "mc ready local"
    interval: 5s
  init_scripts:
    - "mc alias set local http://localhost:9000 minioadmin minioadmin"
    - "mc mb local/my-bucket"
```

#### LocalStack Service

```yaml
localstack:
  build_type: "LocalService"
  service_type: "LocalStack"
  config:
    ports:
      - "4566:4566"  # All AWS services
      - "4571:4571"  # Legacy edge port
    env:
      SERVICES: "s3,sqs,sns,dynamodb"
      AWS_DEFAULT_REGION: "us-east-1"
      EDGE_PORT: "4566"
    volumes:
      - "./data/localstack:/tmp/localstack"
      - "/var/run/docker.sock:/var/run/docker.sock"
```

#### Stripe CLI Service

```yaml
stripe:
  build_type: "LocalService"
  service_type: "StripeCLI"
  config:
    env:
      STRIPE_API_KEY: "${STRIPE_SECRET_KEY}"
      STRIPE_DEVICE_NAME: "rush-dev"
    docker_args:
      - "--network=host"  # Access host services
  command: "listen --forward-to http://backend:8080/webhooks/stripe"
```

### 3. Integration with Rush Reactor

Modify the `ContainerReactor` to:

1. **Pre-launch Phase**: Start local services before main containers
2. **Service Discovery**: Inject service endpoints into application containers
3. **Health Monitoring**: Ensure local services are healthy before starting apps
4. **Selective Restart**: Keep local services running during app rebuilds

```rust
impl ContainerReactor {
    pub async fn launch(&mut self) -> Result<()> {
        // NEW: Start local services first
        self.start_local_services().await?;
        
        // Wait for local services to be healthy
        self.wait_for_local_services().await?;
        
        // Inject local service endpoints into environment
        self.inject_service_endpoints()?;
        
        // Continue with normal launch
        self.launch_loop().await
    }
    
    async fn cleanup_containers(&mut self) -> Result<()> {
        // Only cleanup application containers, not local services
        for service in &self.running_services {
            if !service.is_local_service() {
                service.stop().await?;
            }
        }
    }
}
```

### 4. Service Discovery and Connection Strings

Automatically generate and inject connection strings:

```rust
impl LocalServiceManager {
    pub fn get_connection_strings(&self) -> HashMap<String, String> {
        let mut connections = HashMap::new();
        
        for (name, service) in &self.services {
            match service.service_type {
                LocalServiceType::PostgreSQL => {
                    connections.insert(
                        format!("{}_DATABASE_URL", name.to_uppercase()),
                        format!("postgres://{}:{}@{}:{}/{}",
                            service.config.env.get("POSTGRES_USER").unwrap_or(&"postgres".to_string()),
                            service.config.env.get("POSTGRES_PASSWORD").unwrap_or(&"password".to_string()),
                            service.hostname(),
                            service.port(),
                            service.config.env.get("POSTGRES_DB").unwrap_or(&"app".to_string())
                        )
                    );
                },
                LocalServiceType::Redis => {
                    connections.insert(
                        format!("{}_REDIS_URL", name.to_uppercase()),
                        format!("redis://{}:{}", service.hostname(), service.port())
                    );
                },
                LocalServiceType::MinIO => {
                    connections.insert(
                        format!("{}_S3_ENDPOINT", name.to_uppercase()),
                        format!("http://{}:{}", service.hostname(), service.port())
                    );
                    connections.insert(
                        format!("{}_S3_ACCESS_KEY", name.to_uppercase()),
                        service.config.env.get("MINIO_ROOT_USER").unwrap_or(&"minioadmin".to_string()).clone()
                    );
                },
                // ... other services
            }
        }
        
        connections
    }
}
```

### 5. CLI Commands

New commands for managing local services:

```bash
# Start all local services
rush local-services start

# Stop specific service
rush local-services stop postgres

# Check health status
rush local-services status

# View logs
rush local-services logs minio

# Reset data (with confirmation)
rush local-services reset postgres

# Backup data
rush local-services backup postgres --output ./backups/

# Restore data
rush local-services restore postgres --from ./backups/postgres-2024-01-15.tar
```

## Benefits

1. **Faster Development Cycle**: No waiting for infrastructure to restart
2. **Data Persistence**: Maintain state between application restarts
3. **Consistent Environment**: Same services across team members
4. **Resource Efficiency**: Services start once and stay running
5. **Better Testing**: Realistic cloud service emulation
6. **Simplified Onboarding**: New developers get full stack with one command

## Migration Path

### Phase 1: Core Implementation
- Implement `LocalServiceManager`
- Add `LocalService` BuildType
- Support PostgreSQL, Redis, MinIO

### Phase 2: Extended Services
- Add LocalStack integration
- Implement Stripe CLI support
- Add health check system

### Phase 3: Advanced Features
- Service backup/restore
- Data seeding/fixtures
- Service templates

### Phase 4: Developer Experience
- CLI commands
- Status dashboard
- Auto-discovery of connection strings

## Configuration Migration

Existing configurations can gradually adopt local services:

```yaml
# Before
database:
  build_type: "Image"
  image: "postgres:latest"
  # ... restarts with everything else

# After  
database:
  build_type: "LocalService"
  service_type: "PostgreSQL"
  persist_data: true
  # ... stays running between app restarts
```

## Security Considerations

1. **Credentials**: Use `.env.local` for sensitive data (not committed)
2. **Network Isolation**: Services only accessible from Rush network
3. **Resource Limits**: Prevent runaway containers
4. **Data Encryption**: Optional encryption for persistent volumes

## Performance Considerations

1. **Lazy Loading**: Only start services that are configured
2. **Health Checks**: Efficient polling to minimize overhead
3. **Resource Pooling**: Share connections between services
4. **Caching**: Cache service status to reduce Docker API calls

## Testing Strategy

1. **Unit Tests**: Test service configuration parsing
2. **Integration Tests**: Test service lifecycle management
3. **E2E Tests**: Test full stack with local services
4. **Benchmarks**: Measure startup time improvements

## Success Metrics

- 50% reduction in development cycle time
- Zero data loss during application rebuilds
- 90% reduction in "works on my machine" issues
- Single command setup for new developers

## Conclusion

The `rush-local-services` crate will significantly improve the developer experience by providing stable, persistent infrastructure services that survive application restarts. This approach mirrors production environments more closely while maintaining the rapid iteration benefits of local development.

## Appendix: Service Defaults

### PostgreSQL Defaults
```yaml
image: postgres:15-alpine
ports: ["5432:5432"]
health_check: pg_isready
data_volume: ./data/postgres
```

### Redis Defaults
```yaml
image: redis:7-alpine
ports: ["6379:6379"]
health_check: redis-cli ping
data_volume: ./data/redis
```

### MinIO Defaults
```yaml
image: minio/minio:latest
ports: ["9000:9000", "9001:9001"]
command: server /data --console-address ":9001"
data_volume: ./data/minio
```

### LocalStack Defaults
```yaml
image: localstack/localstack:latest
ports: ["4566:4566"]
services: s3,sqs,sns,dynamodb,lambda
data_volume: ./data/localstack
```

### Stripe CLI Defaults
```yaml
image: stripe/stripe-cli:latest
command: listen --forward-to {backend_url}/webhooks/stripe
network: host
```
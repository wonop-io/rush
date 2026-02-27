# Rush Local Services Example

This example demonstrates how to use Rush's local services feature to run persistent development services like databases and caches alongside your application containers.

## Features

Local services in Rush:
- **Persist between rebuilds** - Your database data remains intact when your app restarts
- **Start automatically** - Services start before your application containers
- **Dependency management** - Services can depend on each other
- **Health checks** - Wait for services to be healthy before starting apps
- **Connection strings** - Automatically generate and inject connection URLs

## Configuration

Local services are defined in `stack.spec.yaml` using the `LocalService` build type:

```yaml
postgres:
  build_type:
    LocalService:
      service_type: postgresql  # Built-in service type
      persist_data: true        # Keep data between restarts
      ports:
        - "5432:5432"          # Port mappings
      env:
        POSTGRES_USER: myuser
        POSTGRES_PASSWORD: mypass
        POSTGRES_DB: mydb
      health_check: "pg_isready -U myuser"
      init_scripts:
        - "psql -U myuser -c 'CREATE EXTENSION IF NOT EXISTS uuid-ossp;'"
```

## Supported Service Types

Built-in service types with sensible defaults:
- `postgresql` / `postgres` - PostgreSQL database
- `redis` - Redis cache
- `minio` - MinIO S3-compatible storage
- `localstack` - LocalStack AWS emulator
- `stripe-cli` - Stripe CLI for webhook forwarding
- Custom services using any Docker image

## Connection Strings

Rush automatically generates connection strings and injects them as environment variables:

- PostgreSQL: `POSTGRES_DATABASE_URL=postgres://user:pass@host:5432/db`
- Redis: `REDIS_REDIS_URL=redis://host:6379`
- MinIO: `MINIO_S3_ENDPOINT=http://host:9000`, `MINIO_S3_ACCESS_KEY`, `MINIO_S3_SECRET_KEY`
- LocalStack: `LOCALSTACK_AWS_ENDPOINT=http://host:4566`

## Running the Example

```bash
# From the rush directory
cd examples/local-services-test

# Start Rush with local services
rush dev

# Services will start first:
# - PostgreSQL on port 5432
# - Redis on port 6379  
# - MinIO on ports 9000/9001
# Then your app will start with connection strings injected

# Stop everything (services persist data)
Ctrl+C

# Restart - data is still there!
rush dev
```

## Advanced Features

### Service Dependencies

Services can depend on each other:

```yaml
app-db:
  build_type:
    LocalService:
      service_type: postgresql
      # ...

app-cache:  
  build_type:
    LocalService:
      service_type: redis
      depends_on:
        - app-db  # Start after app-db
```

### Custom Services

Use any Docker image:

```yaml
custom-service:
  build_type:
    LocalService:
      service_type: custom
      image: "my-custom-image:latest"
      command: "custom-command --with-args"
      # ...
```

### Resource Limits

Control resource usage:

```yaml
postgres:
  build_type:
    LocalService:
      service_type: postgresql
      resources:
        memory: "512m"
        cpus: "0.5"
```

## Benefits

1. **Faster Development** - No need to manually start/stop services
2. **Consistent Environment** - Same services for all developers
3. **Data Persistence** - Keep test data between coding sessions
4. **Automatic Cleanup** - Services stop when Rush stops
5. **Network Integration** - Services join the same Docker network as your apps
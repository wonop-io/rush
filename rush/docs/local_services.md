# Rush Local Services Documentation

## Overview

Rush Local Services provides a powerful way to run infrastructure dependencies alongside your application during development. These services run as Docker containers and are automatically managed by Rush, with support for data persistence, health checks, and automatic environment variable injection.

## Table of Contents

- [Quick Start](#quick-start)
- [Configuration Syntax](#configuration-syntax)
- [Supported Services](#supported-services)
- [Data Persistence](#data-persistence)
- [Environment Variables](#environment-variables)
- [Health Checks](#health-checks)
- [Service Examples](#service-examples)
- [Advanced Configuration](#advanced-configuration)
- [Troubleshooting](#troubleshooting)

## Quick Start

To add a local service to your project, add it to your `stack.spec.yaml`:

```yaml
database:
  build_type: "LocalService"
  service_type: "postgresql"
  version: "16"
  persist_data: true
  env:
    POSTGRES_DB: myapp
    POSTGRES_USER: appuser
    POSTGRES_PASSWORD: secretpass
```

When you run `rush <product> dev`, Rush will:
1. Start the PostgreSQL container
2. Wait for it to be healthy
3. Inject connection strings into your application's environment
4. Keep data persistent between restarts

## Configuration Syntax

### Basic Structure

```yaml
<component_name>:
  build_type: "LocalService"
  service_type: "<service_type>"    # Required: postgresql, mysql, redis, etc.
  version: "<version>"               # Optional: Service version (defaults to latest stable)
  persist_data: <boolean>            # Optional: Keep data between restarts (default: false)
  env:                              # Optional: Environment variables
    KEY: value
  health_check: "<command>"         # Optional: Custom health check command
  init_scripts:                     # Optional: Initialization scripts
    - "SQL or shell commands"
  command: "<custom_command>"       # Optional: Override default container command
  depends_on:                       # Optional: Dependencies on other services
    - other_service
```

## Supported Services

### PostgreSQL

```yaml
database:
  build_type: "LocalService"
  service_type: "postgresql"
  version: "16"  # Versions: 16, 15, 14, 13, 12
  persist_data: true
  env:
    POSTGRES_DB: myapp
    POSTGRES_USER: appuser
    POSTGRES_PASSWORD: secretpass
    POSTGRES_PORT: "5432"
  health_check: "pg_isready -U appuser -p 5432"
  init_scripts:
    - "CREATE EXTENSION IF NOT EXISTS uuid-ossp;"
    - "CREATE SCHEMA IF NOT EXISTS app_schema;"
```

**Injected Environment Variables:**
- `DATABASE_URL`: `postgresql://appuser:secretpass@database:5432/myapp`
- `POSTGRES_HOST`: `database`
- `POSTGRES_PORT`: `5432`

**Data Location:** `./target/local-services/database/postgres.db/`

### MySQL

```yaml
mysql_db:
  build_type: "LocalService"
  service_type: "mysql"
  version: "8.0"  # Versions: 8.0, 5.7
  persist_data: true
  env:
    MYSQL_DATABASE: myapp
    MYSQL_USER: appuser
    MYSQL_PASSWORD: secretpass
    MYSQL_ROOT_PASSWORD: rootpass
  health_check: "mysqladmin ping -h localhost"
  init_scripts:
    - "CREATE TABLE IF NOT EXISTS users (id INT PRIMARY KEY);"
```

**Injected Environment Variables:**
- `MYSQL_URL`: `mysql://appuser:secretpass@mysql_db:3306/myapp`
- `MYSQL_HOST`: `mysql_db`
- `MYSQL_PORT`: `3306`

**Data Location:** `./target/local-services/mysql_db/mysql.db/`

### MongoDB

```yaml
mongodb:
  build_type: "LocalService"
  service_type: "mongodb"
  version: "7.0"  # Versions: 7.0, 6.0, 5.0
  persist_data: true
  env:
    MONGO_INITDB_ROOT_USERNAME: admin
    MONGO_INITDB_ROOT_PASSWORD: adminpass
    MONGO_INITDB_DATABASE: myapp
  health_check: "mongosh --eval 'db.adminCommand(\"ping\")'"
```

**Injected Environment Variables:**
- `MONGODB_URL`: `mongodb://admin:adminpass@mongodb:27017/myapp`
- `MONGO_HOST`: `mongodb`
- `MONGO_PORT`: `27017`

**Data Location:** `./target/local-services/mongodb/mongo.db/`

### Redis

```yaml
redis_cache:
  build_type: "LocalService"
  service_type: "redis"
  version: "7.2"  # Versions: 7.2, 7.0, 6.2
  persist_data: true
  env:
    REDIS_PORT: "6379"
  health_check: "redis-cli -p 6379 ping"
  command: "redis-server --appendonly yes"  # Enable AOF persistence
```

**Injected Environment Variables:**
- `REDIS_URL`: `redis://redis_cache:6379`
- `REDIS_HOST`: `redis_cache`
- `REDIS_PORT`: `6379`

**Data Location:** `./target/local-services/redis_cache/redis.data/`

### MinIO (S3-Compatible Storage)

```yaml
s3_storage:
  build_type: "LocalService"
  service_type: "minio"
  version: "latest"
  persist_data: true
  env:
    MINIO_ROOT_USER: minioadmin
    MINIO_ROOT_PASSWORD: minioadmin123
    MINIO_DEFAULT_BUCKETS: "uploads,media,backups"  # Auto-create buckets
  health_check: "curl -f http://localhost:9000/minio/health/live"
```

**Injected Environment Variables:**
- `S3_ENDPOINT`: `http://s3_storage:9000`
- `S3_ACCESS_KEY`: `minioadmin`
- `S3_SECRET_KEY`: `minioadmin123`
- `AWS_ENDPOINT_URL`: `http://s3_storage:9000`
- `AWS_ACCESS_KEY_ID`: `minioadmin`
- `AWS_SECRET_ACCESS_KEY`: `minioadmin123`

**Data Location:** `./target/local-services/s3_storage/minio.data/`

**Web Console:** http://localhost:9001 (username: minioadmin, password: minioadmin123)

#### Using MinIO with AWS SDK

```python
import boto3

# The endpoint URL is automatically injected
s3 = boto3.client(
    's3',
    endpoint_url=os.environ['S3_ENDPOINT'],
    aws_access_key_id=os.environ['S3_ACCESS_KEY'],
    aws_secret_access_key=os.environ['S3_SECRET_KEY'],
    region_name='us-east-1'  # MinIO doesn't care about region
)

# Create a bucket (if not auto-created)
s3.create_bucket(Bucket='my-bucket')

# Upload a file
s3.upload_file('local_file.txt', 'my-bucket', 'remote_file.txt')

# List objects
response = s3.list_objects_v2(Bucket='my-bucket')
for obj in response.get('Contents', []):
    print(obj['Key'])
```

### LocalStack (AWS Services)

```yaml
aws_local:
  build_type: "LocalService"
  service_type: "localstack"
  version: "3.0"
  persist_data: true
  env:
    SERVICES: "s3,sqs,sns,dynamodb,lambda"
    DEFAULT_REGION: "us-east-1"
    AWS_ACCESS_KEY_ID: "test"
    AWS_SECRET_ACCESS_KEY: "test"
    DOCKER_HOST: "unix:///var/run/docker.sock"
  health_check: "curl -f http://localhost:4566/_localstack/health"
```

**Injected Environment Variables:**
- `LOCALSTACK_ENDPOINT`: `http://aws_local:4566`
- `AWS_ENDPOINT_URL`: `http://aws_local:4566`
- `AWS_DEFAULT_REGION`: `us-east-1`

**Data Location:** `./target/local-services/aws_local/localstack.data/`

### Stripe CLI

```yaml
stripe:
  build_type: "LocalService"
  service_type: "stripe-cli"
  persist_data: false
  env:
    STRIPE_WEBHOOK_URL: "http://localhost:9000/api/stripe/webhook"
    STRIPE_API_KEY: "sk_test_..."  # Set in .env.secrets
  command: "listen --forward-to http://localhost:9000/api/stripe/webhook --skip-verify"
```

**Note:** Stripe CLI runs as a process, not a Docker container.

### MailHog (Email Testing)

```yaml
mailhog:
  build_type: "LocalService"
  service_type: "mailhog"
  persist_data: false
  env:
    MH_SMTP_PORT: "1025"
    MH_API_PORT: "8025"
    MH_UI_PORT: "8025"
```

**Injected Environment Variables:**
- `SMTP_HOST`: `mailhog`
- `SMTP_PORT`: `1025`
- `MAIL_URL`: `smtp://mailhog:1025`

**Web UI:** http://localhost:8025

### Custom Services

```yaml
custom_service:
  build_type: "LocalService"
  service_type: "custom"
  image: "my-custom-image:latest"  # Specify custom Docker image
  persist_data: true
  env:
    CUSTOM_VAR: "value"
  command: "custom-command --with-args"
  health_check: "custom-health-check"
```

## Data Persistence

When `persist_data: true` is set, Rush automatically:

1. Creates a data directory under `./target/local-services/<component_name>/`
2. Mounts this directory as a volume in the container
3. Preserves data between container restarts

### Default Volume Mappings

| Service | Host Path | Container Path |
|---------|-----------|----------------|
| PostgreSQL | `./target/local-services/<name>/postgres.db/` | `/var/lib/postgresql/data` |
| MySQL | `./target/local-services/<name>/mysql.db/` | `/var/lib/mysql` |
| MongoDB | `./target/local-services/<name>/mongo.db/` | `/data/db` |
| Redis | `./target/local-services/<name>/redis.data/` | `/data` |
| MinIO | `./target/local-services/<name>/minio.data/` | `/data` |
| LocalStack | `./target/local-services/<name>/localstack.data/` | `/var/lib/localstack` |

### Clearing Persistent Data

To reset a service's data:

```bash
# Remove specific service data
rm -rf ./target/local-services/database/

# Remove all local service data
rm -rf ./target/local-services/
```

## Environment Variables

### Automatic Injection

Rush automatically injects connection strings and configuration into your application's environment:

1. **Connection URLs**: Standard connection strings for each service
2. **Host and Port**: Individual host and port variables
3. **Credentials**: Username and password variables
4. **Custom Variables**: Any variables defined in the service's `env` section

### Using in Your Application

```javascript
// Node.js example
const pgClient = new Client({
  connectionString: process.env.DATABASE_URL
});

const redis = new Redis(process.env.REDIS_URL);

const s3 = new S3Client({
  endpoint: process.env.S3_ENDPOINT,
  credentials: {
    accessKeyId: process.env.S3_ACCESS_KEY,
    secretAccessKey: process.env.S3_SECRET_KEY
  }
});
```

```python
# Python example
import os
import psycopg2
import redis
import boto3

# PostgreSQL
conn = psycopg2.connect(os.environ['DATABASE_URL'])

# Redis
r = redis.from_url(os.environ['REDIS_URL'])

# S3/MinIO
s3 = boto3.client('s3', endpoint_url=os.environ['S3_ENDPOINT'])
```

## Health Checks

Services are considered healthy when:

1. The container is running
2. The health check command succeeds (if specified)
3. The service responds on its expected port

### Default Health Checks

| Service | Default Health Check |
|---------|---------------------|
| PostgreSQL | `pg_isready -U $POSTGRES_USER` |
| MySQL | `mysqladmin ping -h localhost` |
| MongoDB | `mongosh --eval 'db.adminCommand("ping")'` |
| Redis | `redis-cli ping` |
| MinIO | `curl -f http://localhost:9000/minio/health/live` |

### Custom Health Checks

```yaml
database:
  build_type: "LocalService"
  service_type: "postgresql"
  health_check: "pg_isready -U myuser -d mydb -h localhost -p 5432"
```

## Service Examples

### Full Stack Application

```yaml
# PostgreSQL for main data
database:
  build_type: "LocalService"
  service_type: "postgresql"
  version: "16"
  persist_data: true
  env:
    POSTGRES_DB: myapp
    POSTGRES_USER: appuser
    POSTGRES_PASSWORD: secretpass
  init_scripts:
    - "CREATE EXTENSION IF NOT EXISTS pgcrypto;"
    - "CREATE EXTENSION IF NOT EXISTS uuid-ossp;"

# Redis for caching and sessions
cache:
  build_type: "LocalService"
  service_type: "redis"
  version: "7.2"
  persist_data: false  # Don't persist cache
  command: "redis-server --maxmemory 256mb --maxmemory-policy allkeys-lru"

# MinIO for file storage
storage:
  build_type: "LocalService"
  service_type: "minio"
  persist_data: true
  env:
    MINIO_ROOT_USER: admin
    MINIO_ROOT_PASSWORD: admin123
    MINIO_DEFAULT_BUCKETS: "uploads,avatars,documents"

# MailHog for email testing
mail:
  build_type: "LocalService"
  service_type: "mailhog"
  persist_data: false

# Your application
backend:
  build_type: "RustBinary"
  location: "backend/server"
  dockerfile: "backend/Dockerfile"
  env:
    # These will be overridden by local services
    DATABASE_URL: "will be injected"
    REDIS_URL: "will be injected"
    S3_ENDPOINT: "will be injected"
    SMTP_HOST: "will be injected"
```

### Microservices with Shared Database

```yaml
# Shared PostgreSQL
shared_db:
  build_type: "LocalService"
  service_type: "postgresql"
  version: "16"
  persist_data: true
  env:
    POSTGRES_DB: microservices
    POSTGRES_USER: shared_user
    POSTGRES_PASSWORD: shared_pass
  init_scripts:
    - "CREATE SCHEMA IF NOT EXISTS auth_service;"
    - "CREATE SCHEMA IF NOT EXISTS user_service;"
    - "CREATE SCHEMA IF NOT EXISTS order_service;"

# Auth Service
auth_service:
  build_type: "RustBinary"
  location: "services/auth"
  depends_on:
    - shared_db
  env:
    DATABASE_SCHEMA: "auth_service"

# User Service
user_service:
  build_type: "RustBinary"
  location: "services/user"
  depends_on:
    - shared_db
  env:
    DATABASE_SCHEMA: "user_service"

# Order Service
order_service:
  build_type: "RustBinary"
  location: "services/order"
  depends_on:
    - shared_db
  env:
    DATABASE_SCHEMA: "order_service"
```

### Data Pipeline with Multiple Stores

```yaml
# Source database
source_db:
  build_type: "LocalService"
  service_type: "postgresql"
  persist_data: true
  env:
    POSTGRES_DB: source

# Analytics database
analytics_db:
  build_type: "LocalService"
  service_type: "postgresql"
  persist_data: true
  env:
    POSTGRES_DB: analytics

# Cache layer
cache:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: false

# Object storage for raw data
raw_storage:
  build_type: "LocalService"
  service_type: "minio"
  persist_data: true
  env:
    MINIO_DEFAULT_BUCKETS: "raw-data,processed-data,archives"

# ETL pipeline
etl_pipeline:
  build_type: "RustBinary"
  location: "pipeline"
  depends_on:
    - source_db
    - analytics_db
    - cache
    - raw_storage
```

## Advanced Configuration

### Service Dependencies

Services can depend on other services:

```yaml
app:
  build_type: "RustBinary"
  location: "app"
  depends_on:
    - database
    - cache
    - storage

database:
  build_type: "LocalService"
  service_type: "postgresql"
  persist_data: true

cache:
  build_type: "LocalService"
  service_type: "redis"
  depends_on:
    - database  # Redis depends on database being ready

storage:
  build_type: "LocalService"
  service_type: "minio"
  persist_data: true
```

### Init Scripts

Initialize your database with SQL scripts:

```yaml
database:
  build_type: "LocalService"
  service_type: "postgresql"
  init_scripts:
    # Create extensions
    - "CREATE EXTENSION IF NOT EXISTS pgcrypto;"
    - "CREATE EXTENSION IF NOT EXISTS uuid-ossp;"
    
    # Create schemas
    - "CREATE SCHEMA IF NOT EXISTS app;"
    - "CREATE SCHEMA IF NOT EXISTS audit;"
    
    # Create tables
    - |
      CREATE TABLE IF NOT EXISTS app.users (
        id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
        email VARCHAR(255) UNIQUE NOT NULL,
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
      );
    
    # Insert test data
    - |
      INSERT INTO app.users (email) VALUES
        ('test@example.com'),
        ('admin@example.com')
      ON CONFLICT DO NOTHING;
```

### Custom Commands

Override the default container command:

```yaml
redis_persistent:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: true
  command: "redis-server --appendonly yes --appendfsync everysec"

postgres_with_logging:
  build_type: "LocalService"
  service_type: "postgresql"
  command: "postgres -c log_statement=all -c log_duration=on"
```

### Resource Limits

Control resource usage (coming soon):

```yaml
database:
  build_type: "LocalService"
  service_type: "postgresql"
  resources:
    memory: "512M"
    cpus: "0.5"
```

## Troubleshooting

### Common Issues

#### 1. Service Fails to Start

**Check logs:**
```bash
docker logs rush-local-<service_name>
```

**Common causes:**
- Port already in use
- Invalid environment variables
- Insufficient permissions
- Docker daemon not running

#### 2. Data Not Persisting

**Verify persist_data is set:**
```yaml
database:
  persist_data: true  # Must be true
```

**Check volume mounts:**
```bash
docker inspect rush-local-<service_name> | grep -A 10 Mounts
```

**Verify data directory exists:**
```bash
ls -la ./target/local-services/<service_name>/
```

#### 3. Connection Refused

**Check service health:**
```bash
docker ps | grep rush-local
docker exec rush-local-<service_name> <health_check_command>
```

**Verify network:**
```bash
docker network ls | grep rush
docker network inspect rush-network
```

#### 4. Environment Variables Not Injected

**Check service started successfully:**
```bash
# Look for "Local services started successfully" in logs
rush <product> dev --log-level debug
```

**Verify environment in container:**
```bash
docker exec rush-local-<service_name> env | grep -E "(DATABASE|REDIS|S3)"
```

### Reset Everything

To completely reset all local services:

```bash
# Stop all Rush containers
docker ps | grep rush | awk '{print $1}' | xargs docker stop

# Remove all Rush containers
docker ps -a | grep rush | awk '{print $1}' | xargs docker rm

# Remove Rush network
docker network rm rush-network

# Clear all data
rm -rf ./target/local-services/

# Restart
rush <product> dev
```

### Debug Mode

Run with debug logging to see detailed information:

```bash
RUST_LOG=debug rush <product> dev
```

## Best Practices

1. **Use Specific Versions**: Always specify service versions for consistency
2. **Secure Credentials**: Use `.env.secrets` for sensitive passwords
3. **Health Checks**: Define appropriate health checks for critical services
4. **Data Management**: Only persist data that needs to survive restarts
5. **Resource Limits**: Set memory/CPU limits for resource-intensive services
6. **Init Scripts**: Use init scripts for schema creation, not data seeding
7. **Dependencies**: Properly define service dependencies
8. **Cleanup**: Regularly clean up old data in `./target/local-services/`

## Migration from Docker Compose

If you're migrating from Docker Compose, here's a comparison:

| Docker Compose | Rush Local Services |
|----------------|-------------------|
| `docker-compose.yml` | `stack.spec.yaml` |
| `image: postgres:16` | `service_type: "postgresql"` + `version: "16"` |
| `volumes: - ./data:/var/lib/postgresql/data` | `persist_data: true` |
| `environment:` | `env:` |
| `healthcheck:` | `health_check:` |
| `depends_on:` | `depends_on:` |
| `command:` | `command:` |
| `docker-compose up` | `rush <product> dev` |
| `docker-compose down` | Ctrl+C or `rush cleanup` |

## Summary

Rush Local Services provides a streamlined way to manage development infrastructure:

- **Automatic Setup**: Services start automatically with `rush dev`
- **Data Persistence**: Optional data persistence between restarts
- **Environment Injection**: Connection strings automatically available to your app
- **Health Monitoring**: Services are verified healthy before starting your app
- **Integrated Management**: No separate Docker commands needed
- **Consistent Configuration**: Single `stack.spec.yaml` for all components

For more information, see the [Rush documentation](https://github.com/your-org/rush) or run `rush --help`.
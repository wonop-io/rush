# Redis Local Service Implementation Plan

## Overview

This document outlines the implementation plan for adding Redis as a local service in Rush, following the existing patterns established for PostgreSQL and other local services.

## Current Architecture Analysis

### Existing Infrastructure

Rush already has a robust local services framework with the following components:

1. **LocalServiceType Enum** (`rush-local-services/src/types.rs`)
   - Redis is already defined as a variant
   - Default image: `redis:7-alpine`
   - Default port: 6379
   - Default health check: `redis-cli ping`

2. **DockerLocalService** (`rush-local-services/src/docker_service.rs`)
   - Generic Docker-based service implementation
   - Handles container lifecycle (start, stop, health checks)
   - Generates connection strings for Redis: `redis://container_name:port`

3. **LocalServiceConfig** (`rush-local-services/src/config.rs`)
   - Configuration structure for all local services
   - Service-specific defaults application
   - Environment variable and port management

4. **LocalServiceManager** (`rush-local-services/src/lib.rs`)
   - Orchestrates multiple local services
   - Manages startup order based on dependencies
   - Aggregates environment variables and secrets

## Redis Implementation Status

**Good news:** Redis is already fully implemented in the Rush local services framework!

### Current Features

1. **Type Definition**: Redis is defined in `LocalServiceType::Redis`
2. **Default Configuration**:
   - Image: `redis:7-alpine`
   - Port: 6379
   - Health check: `redis-cli ping`
3. **Connection String Generation**: Automatic generation of `REDIS_URL`
4. **Docker Integration**: Full container lifecycle management
5. **Persistence Support**: Data volume mounting when `persist_data: true`

## Usage Guide

### Basic Configuration

Add to your `stack.spec.yaml`:

```yaml
redis:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: true
```

This minimal configuration will:
- Use `redis:7-alpine` image
- Expose port 6379
- Generate `REDIS_REDIS_URL` environment variable
- Persist data in `./data/redis`

### Advanced Configuration

```yaml
redis:
  build_type: "LocalService"
  service_type: "redis"
  image: "redis:7.2-alpine"  # Custom image version
  persist_data: true
  ports:
    - "6379:6379"
  env:
    # Redis configuration via environment variables
    REDIS_DATABASES: "16"
    REDIS_MAX_MEMORY: "256mb"
  volumes:
    - "./data/redis:/data"
    - "./redis.conf:/usr/local/etc/redis/redis.conf:ro"
  command: "redis-server /usr/local/etc/redis/redis.conf"
  health_check: "redis-cli -p 6379 ping"
  post_startup_tasks:
    # Initialize Redis with sample data
    - "redis-cli SET app:config:version '1.0.0'"
    - "redis-cli HSET user:1 name 'Test User' email 'test@example.com'"
```

### With Redis Configuration File

Create `redis.conf`:

```conf
# Redis configuration
bind 0.0.0.0
protected-mode no
port 6379
databases 16
maxmemory 256mb
maxmemory-policy allkeys-lru
appendonly yes
appendfilename "redis.aof"
dir /data
```

Then reference it in `stack.spec.yaml`:

```yaml
redis:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: true
  volumes:
    - "./data/redis:/data"
    - "./redis.conf:/usr/local/etc/redis/redis.conf:ro"
  command: "redis-server /usr/local/etc/redis/redis.conf"
```

### Redis Cluster Configuration

For Redis cluster setup:

```yaml
redis-master:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: true
  ports:
    - "6379:6379"

redis-replica1:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: true
  ports:
    - "6380:6379"
  command: "redis-server --replicaof redis-master 6379"
  depends_on:
    - "redis-master"

redis-replica2:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: true
  ports:
    - "6381:6379"
  command: "redis-server --replicaof redis-master 6379"
  depends_on:
    - "redis-master"
```

### Redis Stack (with RedisJSON, RedisSearch, etc.)

```yaml
redis-stack:
  build_type: "LocalService"
  service_type: "redis"
  image: "redis/redis-stack:latest"
  persist_data: true
  ports:
    - "6379:6379"    # Redis port
    - "8001:8001"    # RedisInsight web UI
  env:
    REDIS_ARGS: "--loadmodule /opt/redis-stack/lib/redisearch.so --loadmodule /opt/redis-stack/lib/rejson.so"
```

## Integration with Applications

### Environment Variables

The LocalServiceManager automatically generates:
- `REDIS_REDIS_URL`: Full connection string (e.g., `redis://rush-local-redis:6379`)

### Application Configuration

In your application's section of `stack.spec.yaml`:

```yaml
backend:
  build_type: "RustBinary"
  location: "./backend"
  env:
    # These are automatically injected by LocalServiceManager
    CACHE_URL: "${REDIS_REDIS_URL}"
    SESSION_STORE: "${REDIS_REDIS_URL}/0"
    QUEUE_REDIS: "${REDIS_REDIS_URL}/1"
```

### Connection Examples

#### Rust (using redis-rs)
```rust
use redis::Client;

let redis_url = std::env::var("REDIS_REDIS_URL")?;
let client = Client::open(redis_url)?;
let mut conn = client.get_connection()?;
```

#### Node.js (using ioredis)
```javascript
const Redis = require('ioredis');
const redis = new Redis(process.env.REDIS_REDIS_URL);
```

#### Python (using redis-py)
```python
import redis
import os

r = redis.from_url(os.environ['REDIS_REDIS_URL'])
```

## Testing the Implementation

### 1. Create Test Configuration

Create `test-redis/stack.spec.yaml`:

```yaml
redis:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: true
  health_check: "redis-cli ping"
  post_startup_tasks:
    - "redis-cli SET test:key 'Hello Rush'"
    - "redis-cli INCR test:counter"

test-app:
  build_type: "RustBinary"
  location: "./app"
  env:
    REDIS_URL: "${REDIS_REDIS_URL}"
```

### 2. Verify Service Startup

```bash
rush test-redis dev

# Check service status
rush local-services status

# View Redis logs
rush local-services logs redis

# Connect to Redis CLI
docker exec -it rush-local-redis redis-cli
```

### 3. Test Persistence

```bash
# Start services
rush test-redis dev

# Add data
docker exec rush-local-redis redis-cli SET persistent:data "Important"

# Stop services
rush test-redis stop

# Restart services
rush test-redis dev

# Verify data persisted
docker exec rush-local-redis redis-cli GET persistent:data
# Should output: "Important"
```

## Advanced Features

### 1. Redis Sentinel for High Availability

```yaml
redis-master:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: true

redis-sentinel-1:
  build_type: "LocalService"
  service_type: "redis"
  image: "redis:7-alpine"
  command: "redis-sentinel /etc/redis/sentinel.conf"
  volumes:
    - "./sentinel.conf:/etc/redis/sentinel.conf"
  depends_on:
    - "redis-master"
```

### 2. Redis with Authentication

```yaml
redis-auth:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: true
  command: "redis-server --requirepass ${REDIS_PASSWORD}"
  env:
    REDIS_PASSWORD: "secure_password_here"
```

### 3. Redis with Custom Modules

```yaml
redis-timeseries:
  build_type: "LocalService"
  service_type: "redis"
  image: "redislabs/redistimeseries:latest"
  persist_data: true
  ports:
    - "6379:6379"
```

## Performance Optimization

### 1. Memory Configuration

```yaml
redis:
  build_type: "LocalService"
  service_type: "redis"
  resources:
    memory: "512m"
    cpus: "0.5"
  command: "redis-server --maxmemory 400mb --maxmemory-policy allkeys-lru"
```

### 2. Persistence Options

```yaml
# Disable persistence for cache-only use
redis-cache:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: false
  command: "redis-server --save '' --appendonly no"

# AOF persistence for durability
redis-durable:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: true
  command: "redis-server --appendonly yes --appendfsync everysec"
```

## Monitoring and Management

### 1. RedisInsight Integration

```yaml
redis:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: true

redis-insight:
  build_type: "LocalService"
  service_type: "Custom"
  image: "redislabs/redisinsight:latest"
  ports:
    - "8001:8001"
  volumes:
    - "./data/redisinsight:/db"
  depends_on:
    - "redis"
```

### 2. Health Monitoring

```yaml
redis:
  build_type: "LocalService"
  service_type: "redis"
  health_check: |
    redis-cli ping &&
    redis-cli --raw info server | grep uptime_in_seconds
  persist_data: true
```

## Common Use Cases

### 1. Session Store
```yaml
redis-sessions:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: true
  env:
    REDIS_DB_SESSIONS: "0"
```

### 2. Task Queue
```yaml
redis-queue:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: true
  env:
    REDIS_DB_QUEUE: "1"
    REDIS_DB_FAILED: "2"
```

### 3. Cache Layer
```yaml
redis-cache:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: false
  command: "redis-server --maxmemory 256mb --maxmemory-policy volatile-lru"
```

### 4. Pub/Sub Message Broker
```yaml
redis-pubsub:
  build_type: "LocalService"
  service_type: "redis"
  persist_data: false
  env:
    REDIS_DB_PUBSUB: "3"
```

## Troubleshooting

### Common Issues and Solutions

1. **Connection Refused**
   - Ensure Redis container is running: `docker ps | grep redis`
   - Check network connectivity: `docker network ls`
   - Verify port mapping in configuration

2. **Memory Issues**
   - Monitor memory usage: `redis-cli INFO memory`
   - Adjust maxmemory setting
   - Configure eviction policy

3. **Persistence Not Working**
   - Check volume mounts are correct
   - Ensure data directory has proper permissions
   - Verify AOF/RDB settings

4. **Performance Problems**
   - Check for slow queries: `redis-cli SLOWLOG GET`
   - Monitor connections: `redis-cli CLIENT LIST`
   - Review memory fragmentation: `redis-cli INFO memory`

## Migration from Standalone Redis

If migrating from a standalone Redis installation:

1. Export existing data:
   ```bash
   redis-cli --rdb dump.rdb
   ```

2. Place dump file in data directory:
   ```bash
   cp dump.rdb ./data/redis/
   ```

3. Start Rush Redis service:
   ```bash
   rush myapp dev
   ```

## Summary

Redis is fully implemented and ready to use in Rush's local services framework. The implementation provides:

- ✅ Automatic container management
- ✅ Connection string generation
- ✅ Health checking
- ✅ Data persistence
- ✅ Environment variable injection
- ✅ Integration with Rush's development workflow
- ✅ Support for Redis clusters and advanced configurations

No additional code changes are needed - Redis can be used immediately by adding the appropriate configuration to your `stack.spec.yaml` file.
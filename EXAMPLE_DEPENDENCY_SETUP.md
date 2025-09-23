# Dependency-Aware Container Orchestration Example

This document demonstrates how to configure Rush with the new dependency-aware startup system that ensures containers start in the correct order with health checks.

## Problem Solved

Previously, Rush could experience race conditions where:
- The ingress container started before backend services were ready
- Nginx couldn't resolve DNS names because backends weren't running yet
- Services would appear running but connections would fail

The new system ensures:
1. Containers start in dependency order
2. Health checks verify readiness before dependent services start
3. DNS resolution is guaranteed to work
4. Failures are detected and reported early

## Configuration Example

### 1. Define Components with Dependencies

In your `products/myapp/stack.spec.yaml`:

```yaml
# Database - starts first (no dependencies)
postgres:
  build_type: "PureDockerImage"
  image_name_with_tag: "postgres:15"
  port: 5432
  target_port: 5432
  env:
    POSTGRES_DB: myapp
    POSTGRES_USER: myapp
    POSTGRES_PASSWORD: secret
  health_check:
    check_type:
      Tcp:
        port: 5432
    initial_delay: 2
    interval: 5
    timeout: 3
    success_threshold: 1
    failure_threshold: 3
    max_retries: 10

# Redis cache - starts first (no dependencies)
redis:
  build_type: "PureDockerImage"
  image_name_with_tag: "redis:7"
  port: 6379
  target_port: 6379
  health_check:
    check_type:
      Tcp:
        port: 6379
    initial_delay: 1
    interval: 3
    timeout: 2
    success_threshold: 1
    failure_threshold: 3

# Backend API - depends on database and cache
backend:
  build_type: "RustBinary"
  location: "backend"
  dockerfile: "backend/Dockerfile"
  port: 8080
  target_port: 8080
  depends_on:
    - postgres
    - redis
  env:
    DATABASE_URL: "postgresql://myapp:secret@postgres:5432/myapp"
    REDIS_URL: "redis://redis:6379"
  health_check:
    check_type:
      Http:
        path: "/health"
        port: 8080
        expected_status: 200
    initial_delay: 5
    interval: 10
    timeout: 5
    success_threshold: 2
    failure_threshold: 3

# Worker service - depends on database and cache
worker:
  build_type: "RustBinary"
  location: "worker"
  dockerfile: "worker/Dockerfile"
  depends_on:
    - postgres
    - redis
  env:
    DATABASE_URL: "postgresql://myapp:secret@postgres:5432/myapp"
    REDIS_URL: "redis://redis:6379"
  # Worker doesn't expose HTTP, use exec health check
  health_check:
    check_type:
      Exec:
        command: ["./health-check.sh"]
    initial_delay: 5
    interval: 30
    timeout: 5

# Frontend - depends on backend
frontend:
  build_type: "TrunkWasm"
  location: "frontend"
  dockerfile: "frontend/Dockerfile"
  port: 3000
  target_port: 3000
  depends_on:
    - backend
  env:
    API_URL: "http://backend:8080"
  health_check:
    check_type:
      Http:
        path: "/"
        port: 3000
        expected_status: 200
    initial_delay: 3
    interval: 10
    timeout: 5

# Ingress - depends on both frontend and backend
ingress:
  build_type: "Ingress"
  location: "ingress"
  port: 80
  target_port: 80
  depends_on:
    - frontend
    - backend
  # DNS health check ensures backends are resolvable
  startup_probe:
    check_type:
      Dns:
        hostname: "backend"
    initial_delay: 1
    interval: 2
    timeout: 2
    success_threshold: 1
    failure_threshold: 10
  health_check:
    check_type:
      Http:
        path: "/health"
        port: 80
        expected_status: 200
    interval: 30
    timeout: 5
```

### 2. Nginx Configuration

Your `ingress/nginx.conf` can now safely reference backend services:

```nginx
upstream backend {
    server backend:8080;  # This will always resolve!
}

upstream frontend {
    server frontend:3000;
}

server {
    listen 80;

    location /api/ {
        proxy_pass http://backend/;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }

    location / {
        proxy_pass http://frontend/;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }

    location /health {
        return 200 "OK";
        add_header Content-Type text/plain;
    }
}
```

## Startup Sequence

With this configuration, Rush will:

### Wave 0: Independent Services
1. Start `postgres` and `redis` in parallel
2. Wait for TCP connectivity on ports 5432 and 6379

### Wave 1: Services with Database Dependencies
1. Start `backend` and `worker` in parallel
2. Backend: Wait for HTTP 200 on `/health`
3. Worker: Wait for health check script to succeed

### Wave 2: Frontend
1. Start `frontend`
2. Wait for HTTP 200 on `/`

### Wave 3: Ingress
1. Start `ingress`
2. First run startup probe: Verify DNS resolution of "backend"
3. Then run health check: Verify HTTP 200 on `/health`

## Health Check Types

### TCP Health Check
Best for databases and services that don't have HTTP endpoints:
```yaml
health_check:
  check_type:
    Tcp:
      port: 5432
```

### HTTP Health Check
Best for web services:
```yaml
health_check:
  check_type:
    Http:
      path: "/health"
      port: 8080
      expected_status: 200
```

### DNS Health Check
Best for ingress startup probes to ensure backends are resolvable:
```yaml
startup_probe:
  check_type:
    Dns:
      hostname: "backend"
```

### Exec Health Check
Best for custom health verification:
```yaml
health_check:
  check_type:
    Exec:
      command: ["./bin/health-check", "--timeout", "5"]
```

## Debugging

Rush provides detailed logging during startup:

```
[INFO] Starting 5 services with dependency-aware ordering
[INFO] Starting wave 1 with 2 components: ["postgres", "redis"]
[INFO] Component postgres started with container abc123
[INFO] Component redis started with container def456
[INFO] Performing health checks for wave 1
[INFO] Waiting for postgres to become healthy
[INFO] Component postgres is healthy
[INFO] Waiting for redis to become healthy
[INFO] Component redis is healthy
[INFO] Wave 1 completed successfully
[INFO] Starting wave 2 with 2 components: ["backend", "worker"]
...
```

## Benefits

1. **Reliability**: No more race conditions or DNS resolution failures
2. **Speed**: Components in the same wave start in parallel
3. **Visibility**: Clear logging shows startup progress and issues
4. **Flexibility**: Multiple health check types for different scenarios
5. **Failure Detection**: Early detection and reporting of startup failures

## Migration from Old System

If you have an existing Rush setup without dependencies:

1. Add `depends_on` fields to components that require other services
2. Add `health_check` configurations to verify readiness
3. For ingress components, add a `startup_probe` with DNS check
4. Test with `rush <product> dev` to verify startup order

The system is backward compatible - components without dependencies or health checks will continue to work as before.
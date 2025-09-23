# Rush Configuration Best Practices

This guide provides best practices for configuring Rush with dependency-aware container orchestration, health checks, and production-ready settings.

## Table of Contents
1. [Dependency Management](#dependency-management)
2. [Health Check Configuration](#health-check-configuration)
3. [Performance Optimization](#performance-optimization)
4. [Error Recovery](#error-recovery)
5. [Monitoring and Observability](#monitoring-and-observability)
6. [Development vs Production](#development-vs-production)

## Dependency Management

### 1. Define Clear Dependency Chains

Always explicitly declare dependencies to ensure proper startup order:

```yaml
# ✅ GOOD: Clear dependencies
user-service:
  depends_on:
    - postgres     # Database must be ready
    - redis        # Cache must be ready
    - auth-service # Auth must be ready for token validation

# ❌ BAD: Missing critical dependencies
user-service:
  depends_on:
    - postgres  # Missing auth-service dependency!
```

### 2. Use Priority Levels

Combine `depends_on` with `priority` for fine-grained control:

```yaml
postgres:
  priority: 0  # Highest priority - starts first

auth-service:
  priority: 1  # Second wave
  depends_on:
    - postgres

api-gateway:
  priority: 4  # Lower priority
  depends_on:
    - auth-service
    - user-service
```

### 3. Avoid Circular Dependencies

Rush detects cycles, but design to avoid them:

```yaml
# ❌ BAD: Circular dependency
service-a:
  depends_on: [service-b]

service-b:
  depends_on: [service-a]  # Creates a cycle!

# ✅ GOOD: Use event bus or shared cache
service-a:
  depends_on: [redis]  # Both depend on shared infrastructure

service-b:
  depends_on: [redis]
```

## Health Check Configuration

### 1. Choose the Right Health Check Type

#### TCP Health Check
Best for databases and services without HTTP:

```yaml
postgres:
  health_check:
    type: tcp
    port: 5432
    initial_delay: 2      # Quick for TCP
    interval: 5
    timeout: 3
    success_threshold: 1  # One success is enough
    failure_threshold: 3
```

#### HTTP Health Check
Best for REST APIs and web services:

```yaml
api-service:
  health_check:
    type: http
    path: "/health"
    expected_status: 200
    initial_delay: 5      # Allow time for initialization
    interval: 10
    timeout: 5
    success_threshold: 2  # Require 2 consecutive successes
    failure_threshold: 3
```

#### DNS Health Check
Essential for ingress/proxy services:

```yaml
ingress:
  startup_probe:  # Use as startup probe!
    type: dns
    hosts:
      - "backend"
      - "frontend"
    initial_delay: 1
    interval: 2
    timeout: 2
    failure_threshold: 10  # Allow time for DNS propagation
```

#### Exec Health Check
For custom health verification:

```yaml
worker-service:
  health_check:
    type: exec
    command: ["./bin/health-check", "--timeout", "5"]
    initial_delay: 10     # Workers may take time to start
    interval: 30         # Less frequent for workers
    timeout: 5
```

### 2. Use Startup Probes for Heavy Initialization

Separate startup from liveness checks:

```yaml
ml-service:
  startup_probe:
    type: http
    path: "/ready"
    expected_status: 200
    initial_delay: 5
    interval: 5
    timeout: 10
    failure_threshold: 60  # Up to 5 minutes for model loading
  health_check:
    type: http
    path: "/health"
    expected_status: 200
    interval: 30
    timeout: 5
    failure_threshold: 3
```

### 3. Tune Timing Parameters

```yaml
# Fast services (simple APIs)
fast-service:
  health_check:
    initial_delay: 2
    interval: 5
    timeout: 2
    success_threshold: 1
    failure_threshold: 3

# Slow services (databases, complex apps)
slow-service:
  health_check:
    initial_delay: 10
    interval: 15
    timeout: 10
    success_threshold: 2
    failure_threshold: 5
```

## Performance Optimization

### 1. Enable Metrics Collection

```yaml
# rushd.yaml
lifecycle:
  collect_metrics: true
  exponential_backoff: true
  max_backoff_delay: 60
```

Monitor startup performance:
- Track slowest components
- Identify bottlenecks
- Optimize wave parallelization

### 2. Optimize Startup Waves

Structure dependencies to maximize parallelization:

```yaml
# ✅ GOOD: Maximum parallelization
cache:      # Wave 0
database:   # Wave 0 (parallel with cache)

service-a:  # Wave 1
  depends_on: [database]

service-b:  # Wave 1 (parallel with service-a)
  depends_on: [cache]

# ❌ BAD: Unnecessary serialization
service-a:
  depends_on: [database]

service-b:  # Could run in parallel!
  depends_on: [database, service-a]  # Unnecessary dependency on service-a
```

### 3. Resource Limits

Set appropriate resource limits:

```yaml
heavy-service:
  docker_extra_run_args:
    - "--memory=2g"
    - "--cpus=2"
    - "--memory-reservation=1g"
```

## Error Recovery

### 1. Configure Recovery Strategies

```yaml
# rushd.yaml
recovery:
  default_strategy: graceful
  allow_degraded: true
  degraded_threshold: 0.7  # 70% of services must be healthy
  network_retry_attempts: 5
  network_retry_delay: 2s
```

### 2. Component Criticality

Define which components are critical:

```yaml
# In your code or config
criticality:
  postgres: critical      # System can't run without it
  redis: important       # Degraded performance without it
  monitoring: optional   # Nice to have
```

### 3. Implement Fallbacks

```yaml
service:
  env:
    FALLBACK_MODE: "enabled"
    CACHE_FALLBACK: "memory"  # Use memory cache if Redis fails
    DB_FALLBACK: "readonly"   # Fallback to read-only mode
```

## Monitoring and Observability

### 1. Export Metrics

Access metrics via the lifecycle manager:

```rust
// Get JSON metrics
let metrics = lifecycle_manager.get_metrics().await;

// Get Prometheus format
let prometheus = lifecycle_manager.get_metrics_prometheus().await;
```

### 2. Key Metrics to Monitor

- **Startup Duration**: Total time from start to all healthy
- **Success Rate**: Percentage of successful component starts
- **Health Check Attempts**: Number of retries needed
- **Wave Completion Times**: Time for each dependency wave
- **Slowest Component**: Identify bottlenecks

### 3. Integration with Monitoring Stack

```yaml
prometheus:
  build_type: "PureDockerImage"
  image_name_with_tag: "prom/prometheus:latest"
  volumes:
    ./prometheus.yml: /etc/prometheus/prometheus.yml
  depends_on:
    - api-gateway  # Scrape metrics after services are up
```

## Development vs Production

### 1. Development Configuration

```yaml
# stack.spec.dev.yaml
service:
  health_check:
    initial_delay: 1      # Fast feedback
    interval: 5
    timeout: 2
    failure_threshold: 2  # Fail fast in dev
  env:
    LOG_LEVEL: "debug"
    CACHE_DISABLED: "false"
```

### 2. Production Configuration

```yaml
# stack.spec.prod.yaml
service:
  health_check:
    initial_delay: 10     # Allow more time
    interval: 30
    timeout: 10
    failure_threshold: 5  # More tolerant
    max_retries: 30
  env:
    LOG_LEVEL: "warn"
    CACHE_ENABLED: "true"
    METRICS_ENABLED: "true"
```

### 3. Environment-Specific Settings

Use Rush variables for environment-specific values:

```yaml
service:
  env:
    DATABASE_URL: "${DATABASE_URL}"
    API_KEY: "${API_KEY}"
    ENVIRONMENT: "${RUSH_ENV}"
```

## Common Patterns

### 1. Database Migration Pattern

```yaml
migration-job:
  build_type: "Job"
  priority: 1
  depends_on: [postgres]

api-service:
  priority: 2
  depends_on: [migration-job]  # Wait for migrations
```

### 2. Sidecar Pattern

```yaml
app:
  port: 8080
  depends_on: [app-sidecar]

app-sidecar:
  port: 8081
  health_check:
    type: tcp
    port: 8081
```

### 3. Gateway Pattern

```yaml
api-gateway:
  depends_on:
    - auth-service
    - user-service
    - product-service
  startup_probe:
    type: dns
    hosts: ["auth-service", "user-service", "product-service"]
```

## Troubleshooting

### 1. Services Not Starting

Check dependency graph:
```bash
rush describe dependencies
```

### 2. Health Checks Failing

Increase timeouts and thresholds:
```yaml
health_check:
  initial_delay: 30  # More time to start
  timeout: 15        # Longer timeout
  max_retries: 60    # More retries
```

### 3. DNS Resolution Issues

Ensure ingress uses DNS health check:
```yaml
ingress:
  startup_probe:
    type: dns
    hosts: ["backend-service"]
```

### 4. Performance Issues

Enable metrics and analyze:
```bash
rush metrics export --format prometheus
```

## Summary

Key takeaways for production-ready Rush configurations:

1. **Always define dependencies explicitly**
2. **Use appropriate health check types**
3. **Configure startup probes for slow-starting services**
4. **Enable metrics collection in production**
5. **Plan for graceful degradation**
6. **Test your dependency graph before deployment**
7. **Monitor startup performance and optimize waves**

Following these best practices ensures reliable, fast, and observable container orchestration with Rush.
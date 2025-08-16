# Rush MCP (Model Context Protocol) Specification

## Overview

This specification defines the Model Context Protocol (MCP) integration for Rush, enabling AI assistants and other MCP clients to control Rush deployments, monitor builds, read logs, and manage containerized applications.

## Architecture

### Components

1. **MCP Server (`rush-mcp`)**: A standalone MCP server that exposes Rush functionality
2. **MCP Sink**: An output sink that routes logs to MCP clients
3. **Resource Providers**: Expose logs, build status, and container information
4. **Tool Providers**: Enable control operations (build, deploy, restart, etc.)

### Communication Flow

```
MCP Client (e.g., Claude) <--> MCP Server (rush-mcp) <--> Rush Core
                                        |
                                        v
                                   Output Sink
                                        |
                                        v
                                 Log Streaming
```

## MCP Server Implementation

### Server Configuration

The MCP server will be launched as a subprocess or standalone service:

```json
{
  "name": "rush-mcp",
  "version": "0.1.0",
  "description": "MCP server for Rush deployment tool",
  "transport": "stdio",
  "tools": [...],
  "resources": [...]
}
```

### Available Tools

#### 1. Build Management

**`rush_build`**
- Description: Build container images for a product
- Parameters:
  - `product_name` (string, required): Product to build
  - `components` (array[string], optional): Specific components to build
  - `force` (boolean, optional): Force rebuild even if cached
- Returns: Build status and output

**`rush_dev`**
- Description: Start development environment
- Parameters:
  - `product_name` (string, required): Product to run
  - `output_format` (string, optional): "default", "split", or "mcp"
  - `redirect` (array[string], optional): Component redirects
- Returns: Container status and streaming logs

**`rush_deploy`**
- Description: Deploy to an environment
- Parameters:
  - `product_name` (string, required): Product to deploy
  - `environment` (string, required): Target environment
  - `dry_run` (boolean, optional): Preview without deploying
- Returns: Deployment status

#### 2. Container Management

**`rush_status`**
- Description: Get status of running containers
- Parameters:
  - `product_name` (string, optional): Filter by product
- Returns: Container status information

**`rush_stop`**
- Description: Stop running containers
- Parameters:
  - `product_name` (string, required): Product to stop
  - `components` (array[string], optional): Specific components
- Returns: Stop confirmation

**`rush_restart`**
- Description: Restart containers
- Parameters:
  - `product_name` (string, required): Product to restart
  - `components` (array[string], optional): Specific components
- Returns: Restart status

#### 3. Log Management

**`rush_logs`**
- Description: Retrieve container logs
- Parameters:
  - `product_name` (string, required): Product name
  - `component` (string, optional): Specific component
  - `lines` (number, optional): Number of lines (default: 100)
  - `follow` (boolean, optional): Stream logs
  - `since` (string, optional): Time filter
- Returns: Log entries

**`rush_clear_logs`**
- Description: Clear stored logs
- Parameters:
  - `product_name` (string, optional): Product to clear
- Returns: Confirmation

#### 4. Secret Management

**`rush_secrets_init`**
- Description: Initialize secrets for a product
- Parameters:
  - `product_name` (string, required): Product name
  - `vault` (string, optional): Vault type
- Returns: Initialization status

**`rush_secrets_list`**
- Description: List required secrets
- Parameters:
  - `product_name` (string, required): Product name
- Returns: Secret definitions

### Available Resources

#### 1. Log Resources

**`logs://{product_name}/{component}`**
- Description: Access to component logs
- Mime type: `text/plain` or `application/json`
- Contents: Recent log entries

**`logs://{product_name}/build`**
- Description: Build logs
- Mime type: `text/plain`
- Contents: Build output

#### 2. Status Resources

**`status://products`**
- Description: List of available products
- Mime type: `application/json`
- Contents: Product configurations

**`status://containers/{product_name}`**
- Description: Container status
- Mime type: `application/json`
- Contents: Container health and metrics

**`status://builds/{product_name}`**
- Description: Build status and history
- Mime type: `application/json`
- Contents: Build metadata

#### 3. Configuration Resources

**`config://products/{product_name}`**
- Description: Product configuration
- Mime type: `application/yaml`
- Contents: stack.spec.yaml content

**`config://environments`**
- Description: Available environments
- Mime type: `application/json`
- Contents: Environment definitions

## MCP Sink Implementation

### Sink Configuration

The MCP sink routes output to connected MCP clients:

```rust
pub struct McpSink {
    buffer: Arc<Mutex<Vec<LogEntry>>>,
    max_buffer_size: usize,
    subscribers: Arc<Mutex<Vec<McpSubscriber>>>,
}
```

### Features

1. **Buffering**: Store recent logs for retrieval
2. **Streaming**: Real-time log streaming to subscribers
3. **Filtering**: Component and log level filtering
4. **Format Options**: JSON or plain text output

### Log Entry Format

```json
{
  "timestamp": "2024-01-15T10:30:45Z",
  "log_origin": "DOCKER",
  "component": "backend",
  "content": "Server started on port 8080",
  "is_error": false,
  "metadata": {
    "container_id": "abc123",
    "product": "io.wonop.helloworld"
  }
}
```

## Integration Points

### 1. CLI Integration

Add MCP server mode to Rush CLI:

```bash
# Start MCP server
rush mcp serve --port 3333

# Or via stdio for subprocess mode
rush mcp serve --stdio
```

### 2. Output Sink Selection

Enable MCP sink via command line:

```bash
rush --output-format mcp io.wonop.helloworld dev
```

### 3. Configuration

MCP settings in `rushd.yaml`:

```yaml
mcp:
  enabled: true
  port: 3333
  buffer_size: 1000
  auth_token: optional_token
```

## Security Considerations

1. **Authentication**: Optional token-based auth
2. **Authorization**: Restrict tool access by environment
3. **Audit Logging**: Track all MCP operations
4. **Secure Transport**: TLS support for network mode

## Error Handling

### Error Response Format

```json
{
  "error": {
    "code": "BUILD_FAILED",
    "message": "Failed to build frontend component",
    "details": {
      "component": "frontend",
      "exit_code": 1,
      "stderr": "..."
    }
  }
}
```

### Error Codes

- `BUILD_FAILED`: Build process failed
- `CONTAINER_ERROR`: Container operation failed
- `NOT_FOUND`: Resource or product not found
- `PERMISSION_DENIED`: Insufficient permissions
- `INVALID_PARAMS`: Invalid tool parameters
- `TIMEOUT`: Operation timed out

## Implementation Phases

### Phase 1: Core MCP Server
- Basic stdio transport
- Essential tools (build, dev, stop)
- Simple log resources

### Phase 2: Advanced Features
- Network transport
- All tools and resources
- MCP sink with streaming

### Phase 3: Production Features
- Authentication/authorization
- Metrics and monitoring
- Advanced filtering and queries

## Example Usage

### Starting Development via MCP

```python
# MCP client example
client.call_tool("rush_dev", {
    "product_name": "io.wonop.helloworld",
    "output_format": "mcp"
})

# Subscribe to logs
logs = client.read_resource("logs://io.wonop.helloworld/backend")
```

### Monitoring Build Status

```python
# Get build status
status = client.read_resource("status://builds/io.wonop.helloworld")

# Trigger rebuild
result = client.call_tool("rush_build", {
    "product_name": "io.wonop.helloworld",
    "force": true
})
```

## Testing Strategy

1. **Unit Tests**: Test individual MCP handlers
2. **Integration Tests**: Test MCP server with Rush core
3. **E2E Tests**: Test with actual MCP clients
4. **Performance Tests**: Validate streaming and buffering

## Future Enhancements

1. **WebSocket Transport**: For browser-based clients
2. **Batch Operations**: Multiple operations in single request
3. **Event Subscriptions**: Push notifications for status changes
4. **Query Language**: Advanced log filtering and search
5. **Multi-tenant Support**: Isolated environments per client
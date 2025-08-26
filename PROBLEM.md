# Container Startup Problem Analysis

## Issue Summary

Docker containers are failing to start successfully in the Rush container lifecycle manager due to a fundamental race condition between container creation and network availability checking.

## Root Cause Analysis

### The Core Problem

Docker's `docker run` command has a two-phase behavior that causes issues:

1. **Container Creation Phase**: Docker immediately creates the container object and assigns it the requested name
2. **Container Start Phase**: Docker then attempts to start the container, which includes network connection

**Critical Issue**: If the start phase fails (e.g., network not found), the container remains in "Created" state with the name reserved, but the `docker run` command returns an error.

### Failure Sequence

Looking at the logs, here's the exact sequence that causes the infinite loop:

```
1. docker run --name helloworld.wonop.io-frontend --network net-helloworld.wonop.io ...
   → Fails with "network net-helloworld.wonop.io not found"
   → BUT container "helloworld.wonop.io-frontend" is created in "Created" state

2. docker run --name helloworld.wonop.io-frontend --network net-helloworld.wonop.io ...
   → Fails with "container name already in use" (from step 1)
   
3. Cleanup attempts to remove helloworld.wonop.io-frontend
   → Successfully removes the container from step 1
   
4. docker run --name helloworld.wonop.io-frontend --network net-helloworld.wonop.io ...
   → Fails again with "network not found"
   → Creates ANOTHER container in "Created" state
   
5. Loop continues...
```

### Evidence from Logs

```
15:59:51 rush_container | Successfully removed existing container: helloworld.wonop.io-frontend
15:59:51 rush_container | Failed to run Docker container: network net-helloworld.wonop.io not found
15:59:52 rush_container | Failed to run Docker container: container name already in use by container "15e610e37657..."
15:59:52 rush_container | Successfully cleaned up conflicting container: helloworld.wonop.io-frontend  
15:59:53 rush_container | Failed to run Docker container: network net-helloworld.wonop.io not found
```

Notice the pattern: 
- Network not found → creates container 
- Name conflict → cleanup succeeds
- Network not found again → creates another container

### Why the Network Appears Missing

The network `net-helloworld-wonop-io` exists (verified with `docker network ls`), but Docker intermittently reports it as "not found" during container startup. This typically happens when:

1. **Race conditions** between multiple Docker operations
2. **Docker daemon internal state** being temporarily inconsistent  
3. **Network driver issues** during high-frequency operations
4. **Container cleanup operations** interfering with network state

## Current State

- ✅ Network name configuration is correct (`net-helloworld.wonop.io`)
- ✅ Container cleanup logic works properly
- ✅ Retry logic prevents infinite loops (stops after 3 attempts)
- ❌ **Network availability check is missing before container creation**
- ❌ **Container creation happens even on network errors**

## Required Fixes

### 1. Add Network Existence Check Before Container Creation

Before attempting `docker run`, verify the network exists:

```rust
// In lifecycle/manager.rs, before calling run_container:
if !self.docker_client.network_exists(&docker_config.network).await? {
    return Err(Error::Docker(format!("Network {} not found", docker_config.network)));
}
```

### 2. Implement Retry Logic for Network Availability

Add exponential backoff for network availability:

```rust
// Wait for network to be available with retry
let mut network_retries = 0;
while !self.docker_client.network_exists(&docker_config.network).await? {
    if network_retries >= 5 {
        return Err(Error::Docker(format!("Network {} not available after retries", docker_config.network)));
    }
    tokio::time::sleep(Duration::from_millis(100 * (1 << network_retries))).await;
    network_retries += 1;
}
```

### 3. Add Container State Validation

After creation, verify container reaches running state:

```rust
// After run_container succeeds, verify it actually started
let status = self.docker_client.container_status(&container_id).await?;
if status != ContainerStatus::Running {
    // Clean up failed container and retry
    let _ = self.docker_client.remove_container(&container_id).await;
    return Err(Error::Docker("Container created but failed to start".to_string()));
}
```

### 4. Separate Container Creation and Starting

Consider using Docker's separate create + start operations instead of `docker run`:

```rust
// Create container first (reserves name)
let container_id = self.docker_client.create_container(&docker_config).await?;
// Then start it (can retry without name conflicts)
self.docker_client.start_container(&container_id).await?;
```

## Priority

**HIGH** - This blocks the entire container startup process and makes Rush unusable for development workflows.

## Files to Modify

1. `/Users/tfr/Documents/Projects/rush/rush/crates/rush-container/src/lifecycle/manager.rs` - Add network validation
2. `/Users/tfr/Documents/Projects/rush/rush/crates/rush-docker/src/traits.rs` - Add network_exists method if missing
3. `/Users/tfr/Documents/Projects/rush/rush/crates/rush-docker/src/client.rs` - Implement network validation

## Testing Strategy

1. **Unit tests**: Mock network_exists returning false/true
2. **Integration tests**: Test with actual Docker network creation/deletion
3. **Race condition tests**: Parallel container creation attempts
4. **Network recovery tests**: Network becomes available during retry sequence
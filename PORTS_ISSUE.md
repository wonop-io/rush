# Port Assignment Issue Analysis

## Current Problem

The port assignment logic in Rush is not working as intended. Components are being assigned hardcoded ports instead of using a systematic port assignment strategy starting from a base port (8129 by default).

### Current Behavior
```bash
CONTAINER ID   IMAGE                                          PORTS
fd2324435f9d   helloworld.wonop.io/ingress:20250827-203328   0.0.0.0:8080->80/tcp     # Should be 9000->80
0a18edeb9566   helloworld.wonop.io/backend:20250827-203325   0.0.0.0:8000->8000/tcp   # Should be 8130->8000
39e103e2de32   helloworld.wonop.io/frontend:20250827-203326  0.0.0.0:9000->80/tcp     # Should be 8129->80
```

### Expected Behavior
Based on stack.spec.yaml configuration:
- Ingress should be on port 9000 (as specified: `port: 9000`)
- Frontend should be on port 8129 (start_port, auto-assigned)
- Backend should be on port 8130 (start_port + 1, auto-assigned)
- Other components without explicit ports should continue from 8131, 8132, etc.

## Root Causes

### 1. Hardcoded Port Defaults
**Location**: `/Users/tfr/Documents/Projects/rush/rush/crates/rush-container/src/reactor/modular_core.rs` lines 814-817

```rust
let (default_port, default_target) = match spec.component_name.as_str() {
    "frontend" => (9000, 80),      // Hardcoded by name!
    "backend" => (8000, 8000),      // Hardcoded by name!
    "ingress" => (8080, 80),        // Hardcoded by name, ignoring YAML!
    _ => (3000, 3000),
};
```

**Issue**: Ports are hardcoded based on component names which is fundamentally wrong because:
- Component names are user-defined and not under Rush's control
- Port specifications in stack.spec.yaml are completely ignored
- The start_port configuration (8129) is not used
- No auto-incrementing logic for components without explicit ports

### 2. Missing Port Extraction from YAML
**Location**: `/Users/tfr/Documents/Projects/rush/rush/crates/rush-container/src/reactor/modular_core.rs` lines 1741-1742

```rust
port: None,           // Not reading from YAML!
target_port: None,    // Not reading from YAML!
```

**Issue**: When using the fallback ComponentBuildSpec creation (which is always used in dev mode), the code doesn't extract `port` and `target_port` fields from the YAML configuration.

### 3. No Dockerfile Port Scanning
The original implementation was supposed to:
1. Scan Dockerfiles for EXPOSE directives
2. Use those as target_port values
3. Assign host ports starting from start_port (8129)

**Evidence from Dockerfiles**:
- Frontend: `EXPOSE 80`
- Backend: `EXPOSE 8000`
- Ingress: `EXPOSE 80`

These EXPOSE directives are currently ignored.

### 4. start_port Configuration Not Used
The `start_port` configuration (default 8129) is:
- Properly loaded from CLI args (`--port` flag)
- Passed through the Config structure
- Available in the reactor configuration
- **But never actually used for port assignment**

## Port Assignment Logic (How It Should Work)

### Ideal Implementation
```rust
// 1. Extract ports from YAML (if specified)
let yaml_port = component_config.get("port")
    .and_then(|v| v.as_i64())
    .map(|p| p as u16);

let yaml_target_port = component_config.get("target_port")
    .and_then(|v| v.as_i64())
    .map(|p| p as u16);

// 2. Scan Dockerfile for EXPOSE directive (if no target_port in YAML)
let dockerfile_port = scan_dockerfile_for_expose(&dockerfile_path)?;

// 3. Assign ports systematically
let mut next_port = config.start_port(); // 8129
for spec in &mut component_specs {
    if !spec.build_type.requires_docker_build() {
        // Skip LocalService and other non-container types
        continue;
    }
    
    // Use YAML port if specified, otherwise auto-assign
    if spec.port.is_none() {
        spec.port = Some(next_port);
        next_port += 1;
    }
    
    // Use YAML target_port > Dockerfile EXPOSE > default to host port
    if spec.target_port.is_none() {
        spec.target_port = dockerfile_port.or(Some(spec.port.unwrap()));
    }
}
```

### Port Assignment Rules
1. **Components with port in YAML**: Use the specified port (e.g., ingress with port: 9000)
2. **Components without port in YAML**: Auto-assign starting from start_port (8129) and increment
3. **LocalService types**: Skip port assignment (they don't require Docker builds)
4. **Target ports priority**: YAML target_port > Dockerfile EXPOSE > same as host port

## Proposed Solution

### Fix 1: Add Port Extraction in Fallback Logic
```rust
// In modular_core.rs, around line 1741
port: component_config.get("port")
    .and_then(|v| v.as_i64())
    .map(|p| p as u16),
target_port: component_config.get("target_port")
    .and_then(|v| v.as_i64())
    .map(|p| p as u16),
```

### Fix 2: Implement Proper Port Assignment
```rust
// In create_services_from_specs(), replace hardcoded logic with:
let mut port_counter = self.config.start_port;

for spec in &self.component_specs {
    if !spec.build_type.requires_docker_build() {
        continue;
    }
    
    // Determine ports based on spec, not component names
    let host_port = spec.port.unwrap_or_else(|| {
        let p = port_counter;
        port_counter += 1;
        p
    });
    
    let container_port = spec.target_port.unwrap_or_else(|| {
        // Could scan Dockerfile here for EXPOSE
        // For now, default to common ports based on what's detected
        // This should ideally come from Dockerfile scanning
        host_port  // Default: use same as host port
    });
    
    let service = ContainerService {
        port: host_port,
        target_port: container_port,
        // ... rest of fields
    };
}
```

### Fix 3: Add Dockerfile Scanning (Optional Enhancement)
```rust
fn scan_dockerfile_for_expose(dockerfile_path: &Path) -> Option<u16> {
    let content = std::fs::read_to_string(dockerfile_path).ok()?;
    for line in content.lines() {
        if line.trim().starts_with("EXPOSE") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(port) = parts[1].parse::<u16>() {
                    return Some(port);
                }
            }
        }
    }
    None
}
```

## Impact of Current Issue

1. **Port Conflicts**: Frontend on 9000 conflicts with where ingress should be (per YAML spec)
2. **Inconsistent Access**: Services not accessible on expected ports
3. **Configuration Ignored**: start_port setting has no effect
4. **YAML Ignored**: Port specifications in stack.spec.yaml are completely ignored
5. **Name Coupling**: System breaks if users name their components differently

## Testing After Fix

```bash
# Start dev mode
./target/release/rush helloworld.wonop.io dev --port 8129

# Expected result:
docker ps --format "table {{.Names}}\t{{.Ports}}"
# helloworld.wonop.io-frontend    0.0.0.0:8129->80/tcp
# helloworld.wonop.io-backend     0.0.0.0:8130->8000/tcp  
# helloworld.wonop.io-ingress     0.0.0.0:9000->80/tcp

# Test access
curl http://localhost:9000       # Ingress -> Frontend
curl http://localhost:9000/api   # Ingress -> Backend
curl http://localhost:8129       # Direct to Frontend
curl http://localhost:8130       # Direct to Backend
```

## Priority

**HIGH** - This breaks the expected development workflow where:
- Ports specified in stack.spec.yaml must be respected
- Components without explicit ports should auto-increment from start_port (8129)
- System should never rely on hardcoded component names
- Configuration should be respected, not ignored

## Recommended Immediate Fix

At minimum, fix the fallback logic to:
1. Read port/target_port from YAML configuration
2. Use config.start_port() for auto-assignment when ports are not specified
3. Remove ALL hardcoding by component names
4. Respect whatever port is specified in the YAML (ingress has port: 9000 in the spec)

This would restore the expected behavior without requiring Dockerfile scanning.
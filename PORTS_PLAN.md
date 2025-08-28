# Port Assignment Refactoring Plan

## Problem Statement

The current port assignment happens too late in the lifecycle, causing the nginx.conf generation for ingress to use incorrect port mappings. The nginx.conf is rendered during the build phase with hardcoded default ports (8080), but the actual port assignment happens after building when creating services.

### Current Flow (Broken)
```
1. from_product_dir() → Creates ComponentBuildSpec with port/target_port from YAML
2. build_components() → Builds images AND renders nginx.conf (using hardcoded 8080 ports)
3. create_services_from_specs() → Assigns actual ports (8129, 8130, 9000, etc.)
4. start_services() → Launches containers with correct ports
```

### Issue in nginx.conf
```nginx
# Generated with wrong ports:
location / {
    set $upstream_endpoint http://helloworld.wonop.io-frontend:8000;  # Should be :80
}
location /api {
    set $upstream_endpoint http://helloworld.wonop.io-backend:8000;   # Happens to be correct
}
```

## Root Cause Analysis

### 1. Port Assignment Location
- **Current**: Ports are assigned in `create_services_from_specs()` in `modular_core.rs`
- **Problem**: This happens AFTER `build_components()` which needs the ports for artifact rendering

### 2. Artifact Rendering Issue
- **Location**: `render_artifacts_for_component()` in `build/orchestrator.rs`
- **Lines 641-642**: Hardcoded `port: 8080, target_port: 8080` for all ingress components
- **Need**: Actual component ports from ComponentBuildSpec

### 3. Missing Port Information Flow
- ComponentBuildSpec has port/target_port fields
- These are populated from YAML in `from_product_dir()`
- But auto-assignment logic is in `create_services_from_specs()`
- Build orchestrator doesn't have access to the auto-assigned ports

## Proposed Solution

### Phase 1: Port Resolution During Spec Creation
Move port assignment logic earlier in the lifecycle, during ComponentBuildSpec creation in `from_product_dir()`.

#### Changes Required:
1. **In `from_product_dir()` method** (`modular_core.rs` ~line 1540-1850):
   - After creating all ComponentBuildSpec objects
   - Before passing to reactor creation
   - Add a port resolution pass that:
     - Respects explicit ports from YAML
     - Auto-assigns ports from start_port for components without ports
     - Scans Dockerfiles for EXPOSE directives for target_port

2. **New Function**: `resolve_component_ports()`
```rust
fn resolve_component_ports(
    specs: &mut Vec<ComponentBuildSpec>, 
    config: &Arc<Config>
) -> Result<()> {
    let mut next_port = config.start_port();
    
    for spec in specs.iter_mut() {
        // Skip components that don't need Docker containers
        if !spec.build_type.requires_docker_build() {
            continue;
        }
        
        // Assign host port if not specified
        if spec.port.is_none() {
            spec.port = Some(next_port);
            next_port += 1;
        }
        
        // Determine target port: YAML > Dockerfile EXPOSE > host port
        if spec.target_port.is_none() {
            spec.target_port = Some(scan_dockerfile_for_expose(spec)
                .unwrap_or(spec.port.unwrap()));
        }
    }
    Ok(())
}
```

### Phase 2: Update Build Orchestrator
Modify the artifact rendering to use actual ports from ComponentBuildSpec.

#### Changes Required:
1. **In `render_artifacts_for_component()`** (`build/orchestrator.rs` ~line 613-680):
   - Remove hardcoded port values (lines 641-642)
   - For ingress components, look up actual component specs
   - Use their resolved port/target_port values

2. **Updated Service Creation for Ingress**:
```rust
// Around line 634-650
let services = if let BuildType::Ingress { components, .. } = &spec.build_type {
    let mut services_map = HashMap::new();
    
    // Need access to all component specs to get their ports
    for component_name in components {
        // Look up the actual component spec to get its ports
        if let Some(component_spec) = self.find_component_spec(component_name) {
            let service_spec = ServiceSpec {
                name: component_name.clone(),
                host: format!("{}-{}", spec.product_name, component_name),
                port: component_spec.port.unwrap_or(8080),
                target_port: component_spec.target_port.unwrap_or(80),
                mount_point: component_spec.mount_point.clone(),
                domain: spec.domain.clone(),
                docker_host: format!("{}-{}", spec.product_name, component_name),
            };
            services_map.entry(spec.domain.clone())
                .or_insert_with(Vec::new)
                .push(service_spec);
        }
    }
    services_map
} else {
    HashMap::new()
};
```

### Phase 3: Simplify Service Creation
Since ports are now pre-resolved, simplify `create_services_from_specs()`.

#### Changes Required:
1. **In `create_services_from_specs()`** (`modular_core.rs` ~line 794):
   - Remove port assignment logic
   - Simply use spec.port and spec.target_port (now guaranteed to be populated)
   - Remove Dockerfile scanning (moved to Phase 1)

## Implementation Order

1. **Step 1**: Add Dockerfile scanning utility function
2. **Step 2**: Create `resolve_component_ports()` function  
3. **Step 3**: Call `resolve_component_ports()` in `from_product_dir()` after creating specs
4. **Step 4**: Pass all component specs to build orchestrator (for cross-component lookups)
5. **Step 5**: Update `render_artifacts_for_component()` to use actual ports
6. **Step 6**: Simplify `create_services_from_specs()`
7. **Step 7**: Test the complete flow

## Data Flow After Fix

```
1. from_product_dir()
   → Create ComponentBuildSpec with port/target_port from YAML
   → resolve_component_ports() - Assigns all missing ports
   → All specs now have complete port information
   
2. build_components() 
   → render_artifacts_for_component() uses resolved ports
   → nginx.conf generated with correct ports
   
3. create_services_from_specs()
   → Simply uses pre-assigned ports from specs
   
4. start_services()
   → Launches containers with correct ports
```

## Benefits

1. **Correct nginx.conf**: Ingress will route to correct ports
2. **Single source of truth**: Port assignment happens once, early
3. **Predictable**: Ports are known before any building/rendering
4. **Simpler code**: No duplicate port assignment logic

## Testing Strategy

1. **Verify nginx.conf generation**:
   ```bash
   cat products/io.wonop.helloworld/target/rushd/nginx.conf | grep upstream_endpoint
   # Should show:
   # frontend -> :80
   # backend -> :8000
   ```

2. **Test port assignment**:
   ```bash
   ./target/release/rush --port 8129 helloworld.wonop.io dev
   docker ps --format "table {{.Names}}\t{{.Ports}}"
   # frontend: 8129->80
   # backend: 8130->8000
   # ingress: 9000->80
   ```

3. **Test ingress routing**:
   ```bash
   curl http://localhost:9000      # Should reach frontend
   curl http://localhost:9000/api  # Should reach backend
   ```

## Risks and Mitigation

1. **Risk**: Breaking existing port assignment
   - **Mitigation**: Keep same logic, just move it earlier

2. **Risk**: Missing component specs in build orchestrator
   - **Mitigation**: Pass all specs to build orchestrator or store in shared state

3. **Risk**: Dockerfile scanning fails
   - **Mitigation**: Fallback to sensible defaults (80 for web, 8080 for services)

## Alternative Approaches Considered

1. **Pass ports via environment**: Too complex, requires major refactoring
2. **Generate nginx.conf after building**: Would require rebuilding ingress image
3. **Template variables in nginx.conf**: Would need runtime resolution

## Conclusion

Moving port resolution to the ComponentBuildSpec creation phase ensures all components have their ports assigned before any building or artifact rendering occurs. This fixes the nginx.conf generation issue while maintaining the same port assignment logic, just executed earlier in the lifecycle.
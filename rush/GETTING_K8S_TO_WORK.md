# Getting Kubernetes Deployment Working in Rush

## Overview
This document outlines a phased plan to enable full Kubernetes deployment capabilities in Rush, including Docker image push, secrets management, and manifest deployment. The current feature/better-reliability branch has the modular reactor architecture but lacks complete K8s functionality compared to the main branch.

## Current State Analysis

### Main Branch (Working K8s)
- **Location**: `src/cluster/k8s.rs` and `src/cluster/k8_encoder.rs`
- **Features**:
  - Docker push via `docker push` command
  - SealedSecrets encoder for K8s secret management (kubeseal)
  - Manifest generation and deployment via kubectl
  - Full rollout command (build → push → deploy)

### Feature Branch (Modular Architecture)
- **Location**: `crates/rush-k8s/` module and modular reactor
- **Features**:
  - Basic K8s module structure exists
  - Placeholder implementations in reactor (debug messages)
  - No Docker push functionality
  - SealedSecrets encoder exists but not integrated
  - Context management structure exists

## Implementation Phases

### Phase 1: Docker Push Capability ✅ COMPLETED
**Goal**: Add Docker image push functionality to the DockerClient trait and implementations

**Completed Tasks**:
1. ✅ **Updated DockerClient trait** (`crates/rush-docker/src/traits.rs`)
   - Added `push_image(&self, image: &str) -> Result<()>` method

2. ✅ **Implemented push in DockerExecutor** (`crates/rush-docker/src/client.rs`)
   - Full implementation with error handling
   - Added logging for debugging

3. ✅ **Updated reactor's build_and_push** (`crates/rush-container/src/reactor/modular_core.rs`)
   - Replaced placeholder with actual Docker push calls
   - Added registry configuration support (RegistryConfig struct)
   - Implemented docker_login() for authentication
   - Added get_registry_tag() for proper image tagging

4. ✅ **Added registry configuration to ModularReactorConfig**
   - Registry URL, namespace, credentials support
   - Credential helper support flag

5. ✅ **Updated all DockerClient implementations**
   - DockerCliClient, PooledDockerClient, MockDockerClient
   - DockerClientWrapper with retry logic

**Testing**:
- Created test_docker_push.sh script for manual testing
- Build compiles successfully with new functionality

### Phase 2: Registry Configuration ✅ COMPLETED
**Goal**: Support configurable Docker registries

**Completed Tasks**:
1. ✅ **Added registry configuration to Config and ModularReactorConfig**
   - Added `docker_registry_namespace`, `docker_registry_username`, `docker_registry_password` fields to Config
   - Created comprehensive `RegistryConfig` struct with URL, namespace, username, password, and credentials helper support

2. ✅ **Implemented secure Docker login**
   - Added `docker_login()` method with secure password handling via tempfile
   - Supports environment variables: `DOCKER_REGISTRY_USERNAME`, `DOCKER_REGISTRY_PASSWORD`
   - Environment-specific variables: `{ENV}_DOCKER_USERNAME`, `{ENV}_DOCKER_PASSWORD`
   - Better error handling for authentication failures

3. ✅ **Enhanced image tagging**
   - Implemented `get_registry_tag()` for proper registry URL formatting
   - Automatic re-tagging before push to registry
   - Support for format: `registry.url/namespace/image:tag`

4. ✅ **Configuration loading from multiple sources**
   - Environment variables (general and environment-specific)
   - Config struct from rush-config
   - Passed through from CLI context to reactor

**Testing**:
- Created comprehensive unit tests for registry configuration
- Tests for tag formatting with various registry types
- All tests passing

### Phase 3: K8s Manifest Generation ✅
**Goal**: Generate Kubernetes manifests from component specs

**Status**: COMPLETED

**Completed Items**:
- Created ManifestGenerator in rush-k8s/src/generator.rs
- Implemented conversion from ComponentBuildSpec to K8s resources
- Generate Deployment manifests with proper image names, ports, and env vars
- Generate Service manifests for components with ports
- Generate Ingress manifests for components with mount points
- Generate ConfigMaps and Secrets with base64 encoding
- Added registry support for image naming (supports Docker Hub, GCR, ECR, etc.)
- Wired generator into reactor's build_manifests() method
- Added kubectl integration in apply() and unapply() methods
- Created comprehensive tests for manifest generation
- All tests passing

**Tasks**:
1. **Create manifest generator** (`crates/rush-k8s/src/generator.rs`)
   - Convert ComponentBuildSpec to K8s resources
   - Generate Deployment, Service, Ingress manifests
   - Support ConfigMaps and Secrets

2. **Implement template rendering**
   - Use existing Tera templates
   - Support variable substitution
   - Handle environment-specific values

3. **Wire into reactor's build_manifests()**
   - Generate manifests to output directory
   - Organize by component and resource type

**Testing**:
- Generate manifests for test components
- Validate YAML structure
- Apply to test cluster

### Phase 4: Secrets Management Integration
**Goal**: Integrate SealedSecrets for secure K8s deployments

**Tasks**:
1. **Connect rush-security with rush-k8s**
   - Load secrets from vault (1Password, JSON, etc.)
   - Convert to K8s Secret manifests

2. **Integrate SealedSecretsEncoder**
   - Apply encoder after manifest generation
   - Support both sealed and unsealed modes

3. **Add secret injection to manifests**
   - Reference secrets in Deployments
   - Handle environment variables from secrets
   - Support mounted secret volumes

**Testing**:
- Create test secrets
- Encode with kubeseal
- Deploy and verify in cluster

### Phase 5: Kubectl Integration
**Goal**: Apply manifests to Kubernetes clusters

**Tasks**:
1. **Implement kubectl wrapper** (`crates/rush-k8s/src/kubectl.rs`)
   - Execute kubectl commands
   - Handle output and errors
   - Support dry-run mode

2. **Update reactor's apply() method**
   ```rust
   pub async fn apply(&mut self) -> Result<()> {
       let manifests = self.generate_manifests().await?;
       for manifest in manifests {
           kubectl_apply(&manifest).await?;
       }
   }
   ```

3. **Add rollback support**
   - Track deployed versions
   - Implement unapply() properly
   - Support staged rollouts

**Testing**:
- Apply manifests to test cluster
- Verify resources created
- Test rollback functionality

### Phase 6: Full Deployment Pipeline
**Goal**: Complete end-to-end deployment workflow

**Tasks**:
1. **Update deploy command** (`crates/rush-cli/src/commands/deploy.rs`)
   - Wire all components together
   - Add progress reporting
   - Handle errors gracefully

2. **Implement deployment strategies**
   - Rolling update
   - Blue-green deployment
   - Canary releases

3. **Add deployment validation**
   - Wait for pods to be ready
   - Health check verification
   - Rollback on failure

**Testing**:
- Full deployment to staging
- Verify all components working
- Test failure scenarios

### Phase 7: Production Readiness
**Goal**: Add enterprise features for production use

**Tasks**:
1. **Add deployment hooks**
   - Pre-deploy validation
   - Post-deploy verification
   - Notification integration

2. **Implement audit logging**
   - Track all deployments
   - Record who deployed what when
   - Integration with monitoring

3. **Add advanced features**
   - Multi-cluster support
   - Namespace management
   - Resource quotas and limits

**Testing**:
- Production deployment simulation
- Load testing
- Disaster recovery scenarios

## Implementation Order and Priority

### Critical Path (Phases 1-5)
These phases are essential for basic K8s functionality:
1. **Phase 1**: Docker Push (1-2 days)
2. **Phase 3**: Manifest Generation (2-3 days)
3. **Phase 4**: Secrets Management (1-2 days)
4. **Phase 5**: Kubectl Integration (1-2 days)

### Enhancement Path (Phases 2, 6-7)
These add robustness and production features:
- **Phase 2**: Registry Configuration (1 day)
- **Phase 6**: Full Pipeline (2-3 days)
- **Phase 7**: Production Features (3-5 days)

## Code Locations Reference

### Files to Modify
- `crates/rush-docker/src/traits.rs` - Add push_image trait method
- `crates/rush-docker/src/command.rs` - Implement Docker push
- `crates/rush-container/src/reactor/modular_core.rs` - Update K8s methods
- `crates/rush-k8s/src/generator.rs` - Create manifest generator (new)
- `crates/rush-k8s/src/kubectl.rs` - Create kubectl wrapper (new)
- `crates/rush-cli/src/commands/deploy.rs` - Wire everything together

### Existing Code to Reuse
- `crates/rush-k8s/src/encoder.rs` - SealedSecrets encoder (ready)
- `crates/rush-k8s/src/manifests.rs` - Manifest structures (ready)
- `crates/rush-k8s/src/context.rs` - K8s context management (ready)

## Success Criteria

For each phase:
- [ ] Unit tests pass
- [ ] Integration tests pass
- [ ] Manual testing successful
- [ ] Documentation updated
- [ ] Error handling comprehensive

## Rollout Strategy

1. **Development Branch**: Implement each phase in feature branches
2. **Integration Testing**: Merge to feature/better-reliability after each phase
3. **Staging Validation**: Test full pipeline in staging environment
4. **Production Release**: Gradual rollout with fallback plan

## Risk Mitigation

### Risks
1. **Breaking existing functionality**: Mitigate with comprehensive tests
2. **Registry authentication failures**: Support multiple auth methods
3. **Manifest generation errors**: Validate templates thoroughly
4. **Cluster compatibility**: Test on multiple K8s versions

### Rollback Plan
- Keep old implementation available via feature flags
- Support quick reversion via git
- Document manual deployment procedures as backup

## Timeline Estimate

**Total Duration**: 10-15 days for full implementation

- **Week 1**: Phases 1, 3, 4 (Core functionality)
- **Week 2**: Phases 2, 5, 6 (Integration and pipeline)
- **Week 3**: Phase 7 and testing (Production features)

## Next Steps

1. **Immediate Action**: Start with Phase 1 (Docker Push)
2. **Parallel Work**: Begin Phase 3 (Manifest Generation) design
3. **Testing Setup**: Create test K8s cluster for validation
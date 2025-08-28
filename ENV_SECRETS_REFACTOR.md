# Environment Secrets Refactor Plan

## Issue Summary

The `.env.secrets` files are being loaded into the `ComponentBuildSpec` structure but are **never actually used** when:
1. Starting Docker containers
2. Building images  
3. Running build scripts
4. Starting local services

Only the `.env` file contents (stored in `dotenv` field) are passed as environment variables, while `.env.secrets` contents (stored in `dotenv_secrets` field) are completely ignored.

## Current State Analysis

### Where .env.secrets IS Loaded

**File**: `/Users/tfr/Documents/Projects/rush/rush/crates/rush-build/src/spec.rs`
- Lines 395-409: `.env.secrets` file is properly loaded into `dotenv_secrets` field
- The loading happens correctly when creating `ComponentBuildSpec`

### Where .env.secrets SHOULD Be Used (But Isn't)

1. **Lifecycle Manager** (`/Users/tfr/Documents/Projects/rush/rush/crates/rush-container/src/lifecycle/manager.rs`)
   - Line 369-371: Only `spec.dotenv` is added to environment variables
   - Line 381-384: Vault secrets are added (but not `.env.secrets`)
   - **MISSING**: `spec.dotenv_secrets` should also be added

2. **Build Orchestrator** (`/Users/tfr/Documents/Projects/rush/rush/crates/rush-container/src/build/orchestrator.rs`)
   - Line 556: Only `spec.dotenv` is passed to build context
   - **MISSING**: `spec.dotenv_secrets` should be merged with env

3. **Image Builder** (`/Users/tfr/Documents/Projects/rush/rush/crates/rush-container/src/image_builder.rs`)
   - Line 573: Only `spec.dotenv` is passed as env_vars
   - **MISSING**: `spec.dotenv_secrets` should be included

4. **Local Services** (`/Users/tfr/Documents/Projects/rush/rush/crates/rush-local-services/`)
   - No reference to `dotenv_secrets` at all
   - Local services likely need access to secrets too

## Proposed Solution

### Priority 1: Fix Container Runtime Environment

**File**: `/Users/tfr/Documents/Projects/rush/rush/crates/rush-container/src/lifecycle/manager.rs`

In the `start_service` method, after line 371, add:
```rust
// Add dotenv secrets
for (key, value) in &spec.dotenv_secrets {
    env_vars.insert(key.clone(), value.clone());
}
```

### Priority 2: Fix Build Environment

**File**: `/Users/tfr/Documents/Projects/rush/rush/crates/rush-container/src/build/orchestrator.rs`

In `build_component_with_script` method, modify line 556:
```rust
// OLD:
env: spec.dotenv.clone(),

// NEW:
env: {
    let mut env = spec.dotenv.clone();
    env.extend(spec.dotenv_secrets.clone());
    env
},
```

### Priority 3: Fix Image Builder Environment

**File**: `/Users/tfr/Documents/Projects/rush/rush/crates/rush-container/src/image_builder.rs`

Around line 573, modify:
```rust
// OLD:
env_vars: spec_guard.dotenv.clone(),

// NEW:
env_vars: {
    let mut env = spec_guard.dotenv.clone();
    env.extend(spec_guard.dotenv_secrets.clone());
    env
},
```

### Priority 4: Consider Secret Precedence

When merging environment variables, establish clear precedence:
1. Base: `spec.dotenv` (from .env file)
2. Override with: `spec.dotenv_secrets` (from .env.secrets file)
3. Override with: `spec.env` (from YAML)
4. Override with: Vault secrets (from vault storage)
5. Final override: Runtime/command-line environment variables

This ensures secrets can override regular environment variables.

## Implementation Steps

1. **Add helper method** to merge environment variables with proper precedence:
```rust
// In ComponentBuildSpec or as a utility function
pub fn get_merged_env(&self) -> HashMap<String, String> {
    let mut env = self.dotenv.clone();
    env.extend(self.dotenv_secrets.clone());
    if let Some(yaml_env) = &self.env {
        env.extend(yaml_env.clone());
    }
    env
}
```

2. **Update all environment variable usage points** to use the merged environment

3. **Add tests** to verify `.env.secrets` values are properly passed to:
   - Running containers
   - Build processes
   - Local services

4. **Document** the environment variable precedence and loading behavior

## Testing Strategy

### Test 1: Container Runtime
1. Create a component with `.env.secrets` file containing `SECRET_KEY=test123`
2. Start the container
3. Verify the container has `SECRET_KEY` environment variable set

### Test 2: Build Process
1. Create a build script that uses an environment variable from `.env.secrets`
2. Run the build
3. Verify the build script can access the secret value

### Test 3: Precedence
1. Set same variable in `.env`, `.env.secrets`, and YAML
2. Verify correct precedence order is applied

## Security Considerations

1. **Logging**: Ensure secret values are not logged
   - When debug logging environment variables, mask secret values
   - Consider adding a list of known secret keys to mask

2. **File Permissions**: Verify `.env.secrets` files have restricted permissions
   - Should be readable only by owner (600 or 640)

3. **Git Ignore**: Ensure `.env.secrets` is in `.gitignore`
   - Already standard practice but worth verifying

## Backwards Compatibility

This change should be fully backwards compatible:
- Components without `.env.secrets` files will continue to work
- The `dotenv_secrets` field already exists but was unused
- No API changes required

## Estimated Impact

- **Files to modify**: 3-4 files
- **Lines of code**: ~20-30 lines total
- **Risk**: Low - adding missing functionality
- **Testing effort**: Medium - need to verify all injection points

## Alternative Approaches Considered

1. **Remove dotenv_secrets field**: Since it's not used, we could remove it entirely and rely only on vault-managed secrets. However, this would remove the convenience of file-based secrets for development.

2. **Merge at load time**: Merge `.env` and `.env.secrets` when loading the spec. This would be simpler but less flexible for debugging and precedence control.

3. **Rename to single .env file**: Combine both files into one. This would be simpler but less secure as it mixes public config with secrets.

## Recommendation

Implement the proposed solution as it:
- Maintains separation of secrets from configuration
- Provides clear precedence rules
- Is backwards compatible
- Requires minimal code changes
- Fixes a clear bug where loaded data is not being used

The implementation should be straightforward and can be completed in phases, with Priority 1 (container runtime) being the most important for immediate functionality.
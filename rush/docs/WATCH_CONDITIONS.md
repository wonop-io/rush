# Watch Conditions for File Change Detection

Rush CLI now properly respects watch conditions to determine when to trigger rebuilds of components.

## How It Works

When file changes are detected, Rush will only trigger a rebuild if the changed files meet the watch conditions for at least one component.

### Watch Patterns (Explicit Configuration)

If a component defines `watch` patterns in `stack.spec.yaml`, **ONLY** files matching those patterns will trigger rebuilds for that component.

Example:
```yaml
components:
  - component_name: frontend
    build_type: TrunkWasm
    location: src/frontend
    watch:
      - "*.rs"           # Only Rust files
      - "*.toml"         # Only TOML files
      - "assets/**/*"    # All files in assets directory
```

With this configuration:
- ✅ Changes to `src/frontend/main.rs` will trigger a rebuild
- ✅ Changes to `src/frontend/Cargo.toml` will trigger a rebuild
- ✅ Changes to `src/frontend/assets/logo.png` will trigger a rebuild
- ❌ Changes to `src/frontend/README.md` will NOT trigger a rebuild
- ❌ Changes to `src/frontend/style.css` will NOT trigger a rebuild (unless in assets/)

### Context Directory (Default Behavior)

If NO watch patterns are defined, Rush falls back to monitoring the component's context directory (typically the `location` or `context_dir`).

Example:
```yaml
components:
  - component_name: backend
    build_type: RustBinary
    location: src/backend
    # No watch patterns defined
```

With this configuration:
- ✅ ANY file change in `src/backend/` will trigger a rebuild
- ❌ Changes outside `src/backend/` will NOT trigger a rebuild

## Build Type Behaviors

Different build types have different default context directories:

- **TrunkWasm/DixiousWasm/RustBinary/Script/Zola/Book**: Uses `location` as the context directory
- **Ingress**: Uses `context_dir` if specified, otherwise current directory
- **PureDockerImage/PureKubernetes/KubernetesInstallation**: No file watching (never triggers rebuilds)

## Debugging

When running with debug logging (`rush dev --verbose`), you'll see detailed information about watch condition evaluation:

```
INFO Testing 3 changed files for significance
DEBUG   Changed file: src/frontend/main.rs
DEBUG Evaluating component: frontend
DEBUG     Component has watch patterns defined
DEBUG       ✓ File src/frontend/main.rs matches watch pattern
INFO   ✓ Component 'frontend' is affected by file changes
INFO Rebuild triggered for components: ["frontend"]
```

Or when no components are affected:

```
INFO Testing 1 changed files for significance
DEBUG   Changed file: README.md
INFO No components affected by file changes - rebuild skipped
INFO   (Check watch patterns in stack.spec.yaml or component context directories)
```

## Best Practices

1. **Be Specific with Watch Patterns**: Define watch patterns to avoid unnecessary rebuilds
   ```yaml
   watch:
     - "src/**/*.rs"        # Source code
     - "Cargo.toml"         # Dependencies
     - "Cargo.lock"        # Lock file
   ```

2. **Exclude Generated Files**: Don't watch files that are generated during build
   ```yaml
   # Good - excludes target directory
   watch:
     - "src/**/*"
     - "Cargo.toml"
   
   # Bad - includes everything
   watch:
     - "**/*"  # Will rebuild on target/ changes!
   ```

3. **Use Patterns for Related Files**: Group related file types
   ```yaml
   watch:
     - "**/*.{rs,toml}"        # Rust and config files
     - "templates/**/*.html"   # HTML templates
     - "static/**/*"           # Static assets
   ```

## Examples

### Frontend Component with Selective Watching
```yaml
- component_name: web-app
  build_type: TrunkWasm
  location: frontend
  watch:
    - "src/**/*.rs"
    - "src/**/*.html"
    - "style/**/*.scss"
    - "Cargo.toml"
    - "Trunk.toml"
```

### Backend Component with Default Watching
```yaml
- component_name: api-server
  build_type: RustBinary
  location: backend
  # No watch patterns - monitors entire backend/ directory
```

### Shared Library with Cross-Component Dependencies
```yaml
- component_name: shared-lib
  build_type: RustBinary
  location: shared
  watch:
    - "src/**/*.rs"
    - "Cargo.toml"
    # Changes here will only rebuild shared-lib, not dependent components
```

## Migration from Previous Behavior

If you're upgrading from a previous version of Rush where all file changes triggered rebuilds:

1. **Review your stack.spec.yaml** - Check if you have watch patterns defined
2. **Add watch patterns if needed** - Be explicit about what should trigger rebuilds
3. **Test your configuration** - Make changes to different files and verify rebuild behavior
4. **Use debug logging** - Run with `--verbose` to see which files trigger rebuilds

## Troubleshooting

### Component not rebuilding when expected

1. Check if watch patterns are defined - they override default behavior
2. Verify the file path matches the pattern
3. Use debug logging to see the evaluation process
4. Check if the component is redirected (redirected components don't rebuild locally)

### Component rebuilding too often

1. Define explicit watch patterns to limit what triggers rebuilds
2. Exclude generated directories (target/, dist/, node_modules/)
3. Consider using more specific patterns (*.rs instead of **)

### No components rebuilding at all

1. Check if changed files are within any component's context
2. Verify watch patterns aren't too restrictive
3. Ensure components aren't all redirected
4. Check that file watcher is properly initialized
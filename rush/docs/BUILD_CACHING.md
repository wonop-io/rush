# Build Caching with Git Hash-Based Image Tags

Rush CLI now implements intelligent Docker image caching to avoid unnecessary rebuilds, significantly improving development workflow efficiency.

## How It Works

The build caching system uses git commit hashes to tag Docker images, allowing Rush to determine whether an image needs to be rebuilt based on the state of the codebase.

### Image Tagging Strategy

Images are tagged with git-based identifiers:

1. **Clean Commits**: `image-name:abc12345` (first 8 chars of git commit hash)
2. **Uncommitted Changes**: `image-name:abc12345-wip-def67890` (commit hash + WIP suffix with hash of changes)
3. **No Git History**: `image-name:latest` (fallback when git history unavailable)

### Cache Logic

When Rush evaluates whether to build an image:

1. **Check Image Existence**: Uses `docker image inspect` to check if the image exists locally
2. **Evaluate Tag Type**:
   - If image doesn't exist → **Build required**
   - If image exists with clean git tag → **Skip build** (code hasn't changed)
   - If image exists with `-wip-` tag → **Build required** (uncommitted changes present)

## Implementation Details

### Key Components

#### ImageBuilder (`src/container/image_builder.rs`)

The `ImageBuilder` struct now includes caching functionality:

```rust
pub struct ImageBuilder {
    // ... other fields ...
    git_tag: Option<String>,           // Git-based tag for the image
    image_exists_in_cache: bool,       // Whether image exists locally
}
```

Key methods:
- `compute_git_tag()`: Generates the git-based tag for the image
- `check_image_exists()`: Verifies if the image exists in Docker cache
- `evaluate_rebuild_needed()`: Determines if rebuild is necessary

#### ContainerReactor (`src/container/reactor.rs`)

The reactor's `build_image` method now:
1. Creates an `ImageBuilder` with toolchain context
2. Evaluates cache status before building
3. Skips build if image exists with clean git tag

## Benefits

1. **Faster Development Cycles**: Skip rebuilds for unchanged components
2. **Bandwidth Savings**: Avoid pulling/pushing unchanged images
3. **Deterministic Builds**: Git hash ensures consistency across environments
4. **WIP Detection**: Automatically rebuilds when uncommitted changes exist

## Configuration

The caching system works automatically without configuration. However, you can influence behavior through:

- **Git State**: Commit your changes for stable caching
- **Docker Registry**: Configure in `rush.yaml` for remote caching
- **Manual Override**: Use `--force-rebuild` flag to bypass cache (if implemented)

## Example Workflow

```bash
# First build - image doesn't exist
$ rush dev
Building image example-app:5abfcfca...

# Second run - no changes, uses cache
$ rush dev
Image example-app:5abfcfca already exists in cache, skipping build

# Make changes without committing
$ echo "// TODO" >> src/main.rs
$ rush dev
Building image example-app:5abfcfca-wip-12345678...

# Commit changes
$ git add . && git commit -m "Add TODO"
$ rush dev
Building image example-app:9bc23def...

# Run again - uses new cache
$ rush dev
Image example-app:9bc23def already exists in cache, skipping build
```

## Troubleshooting

### Image Not Being Cached

1. Check git status: `git status`
2. Verify Docker is running: `docker ps`
3. Check image exists: `docker images | grep your-image`

### Unexpected Rebuilds

1. Check for uncommitted changes: `git diff`
2. Verify git hash consistency: `git log -n 1 --format=%H`
3. Check Docker image tags: `docker images --format "table {{.Repository}}:{{.Tag}}"`

### Cache Invalidation

To force a rebuild despite cache:
1. Delete the Docker image: `docker rmi image-name:tag`
2. Make a small change and commit
3. Use force rebuild flag (when available)

## Future Enhancements

Potential improvements to the caching system:

1. **Remote Cache Support**: Pull images from registry if not available locally
2. **Layer Caching**: Optimize Dockerfile for better layer caching
3. **Cache Metrics**: Track cache hit/miss rates
4. **Selective Invalidation**: Invalidate specific components
5. **Build Cache Sharing**: Share cache across team members via registry

## Technical Notes

- Uses `docker image inspect` for existence checking (fast, no network)
- Git hash computed per component's context directory
- WIP detection uses SHA256 hash of uncommitted changes
- Compatible with multi-stage Docker builds
- Thread-safe implementation using Arc/Mutex
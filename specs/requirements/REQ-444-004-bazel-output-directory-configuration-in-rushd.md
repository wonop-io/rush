---
id: REQ-444-004
type: requirement
progress: backlog
parents:
- UC-444-002
priority: high
created: 2026-02-27T10:51:40.473562Z
updated: 2026-02-27T10:51:40.473562Z
author: agent
---

# Bazel output directory configuration in rushd.yaml

## Description
Extend the `rushd.yaml` configuration file to support a global Bazel output directory setting that provides a persistent location for Bazel build artifacts.

## Specification

### Configuration Schema
Add a new `bazel` section to `RushdConfig` in `rush-config/src/loader.rs`:

```yaml
# rushd.yaml
env:
  # ... existing env vars

bazel:
  output_dir: "/absolute/path/to/bazel/outputs"
  # or
  output_dir: ".bazel-out"  # relative to project root
  
  # Optional: default base image for OCI generation
  default_base_image: "gcr.io/distroless/static"
  
  # Optional: additional global Bazel arguments
  global_args:
    - "--experimental_remote_cache=..."
```

### Rust Implementation
```rust
// In rush-config/src/loader.rs

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BazelConfig {
    /// Output directory for Bazel build artifacts
    /// Can be absolute or relative to project root
    #[serde(default = "default_bazel_output_dir")]
    pub output_dir: String,
    
    /// Default base image for OCI image generation
    #[serde(default)]
    pub default_base_image: Option<String>,
    
    /// Global Bazel arguments applied to all builds
    #[serde(default)]
    pub global_args: Option<Vec<String>>,
}

fn default_bazel_output_dir() -> String {
    "target/bazel-out".to_string()
}

// Add to RushdConfig
#[derive(Debug, Deserialize, Serialize)]
pub struct RushdConfig {
    pub env: std::collections::HashMap<String, String>,
    #[serde(default = "default_cross_compile")]
    pub cross_compile: String,
    #[serde(default)]
    pub dev_output: DevOutputConfig,
    #[serde(default)]
    pub bazel: Option<BazelConfig>,
}
```

### Path Resolution Logic
```rust
impl BazelConfig {
    /// Resolves the output directory path
    /// - Absolute paths are used as-is
    /// - Relative paths are resolved against the project root
    pub fn resolve_output_dir(&self, project_root: &Path) -> PathBuf {
        let path = Path::new(&self.output_dir);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            project_root.join(path)
        }
    }
}
```

## Acceptance Criteria
- [ ] `BazelConfig` struct is defined with all fields
- [ ] Default values are provided for optional fields
- [ ] Serde deserialization works for both absolute and relative paths
- [ ] Path resolution correctly handles both path types
- [ ] Existing `rushd.yaml` files without `bazel` section continue to work
- [ ] Unit tests verify configuration loading and path resolution

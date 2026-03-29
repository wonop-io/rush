# Setting Up Bazel Components with Rush

This guide explains how to configure and build Bazel-based components using Rush.

## Overview

Rush supports Bazel as a build system for components in your stack. The Bazel integration:

1. Executes `bazel build` in your workspace directory
2. Copies build outputs to a staging directory
3. Generates a Dockerfile that packages the build artifacts
4. Builds an OCI image and runs it as part of your stack

## Prerequisites

- Bazel installed and available in your PATH (or configured via `rushd.yaml`)
- A valid Bazel workspace with `WORKSPACE` or `MODULE.bazel` file

## Quick Start

### 1. Create the Bazel Workspace Structure

```
products/your.product/my-bazel-component/
├── WORKSPACE           # or MODULE.bazel for Bzlmod
├── BUILD              # optional root BUILD file
└── src/
    ├── BUILD
    └── main.py        # or your source files
```

### 2. Configure the Component in stack.spec.yaml

```yaml
components:
  my-bazel-component:
    build_type: "Bazel"
    location: "my-bazel-component"
    output_dir: "target/bazel-out"
    port: 8080
    target_port: 8080
    mount_point: "/my-service"
    targets:
      - "//src:app"
    base_image: "python:3.11-slim"
```

## Configuration Reference

### Component Options (stack.spec.yaml)

| Option | Required | Default | Description |
|--------|----------|---------|-------------|
| `build_type` | Yes | - | Must be `"Bazel"` |
| `location` | Yes | - | Path to Bazel workspace (relative to product directory) |
| `output_dir` | No | `"target/bazel-out"` | Directory where build outputs are staged |
| `targets` | No | `["//..."]` | List of Bazel targets to build |
| `additional_args` | No | `[]` | Additional arguments passed to `bazel build` |
| `base_image` | No | `"python:3.11-slim"` | Base Docker image for the container |
| `port` | Yes | - | External port for the service |
| `target_port` | Yes | - | Internal container port |
| `mount_point` | Yes | - | URL path prefix for routing |

### Global Bazel Configuration (rushd.yaml)

You can set global defaults for all Bazel builds in your `rushd.yaml`:

```yaml
bazel:
  output_dir: "target/bazel-out"    # Default output directory
  binary: "bazel"                    # Path to bazel binary
  additional_args:                   # Default build arguments
    - "--jobs=4"
```

## Build Process

When Rush builds a Bazel component, it performs these steps:

### 1. Workspace Validation
Rush checks for either `WORKSPACE` or `MODULE.bazel` in the component's location directory.

### 2. Build Execution
```bash
bazel build <targets> <additional_args>
```
- Targets default to `//...` (all targets) if not specified
- Additional args from both component and global config are merged

### 3. Output Collection
Build outputs from `bazel-bin/` are copied to the configured `output_dir`.

### 4. Dockerfile Generation
Rush generates a Dockerfile that:
- Uses the configured `base_image`
- Copies all files from `output_dir` to `/app`
- Sets `/app` as the working directory

### 5. Image Build
The generated Dockerfile is used to build an OCI image via Rush's container build system.

## Complete Example

### Directory Structure

```
products/io.wonop.helloworld/
├── stack.spec.yaml
└── demo-bazel/
    ├── WORKSPACE
    ├── MODULE.bazel
    └── src/
        ├── BUILD
        └── main.py
```

### src/BUILD

```python
py_binary(
    name = "app",
    srcs = ["main.py"],
    visibility = ["//visibility:public"],
)
```

### src/main.py

```python
from http.server import HTTPServer, SimpleHTTPRequestHandler
import os

class Handler(SimpleHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-type", "text/plain")
        self.end_headers()
        self.wfile.write(b"Hello from Bazel-built service!")

if __name__ == "__main__":
    port = int(os.environ.get("PORT", 8080))
    server = HTTPServer(("", port), Handler)
    print(f"Serving on port {port}")
    server.serve_forever()
```

### stack.spec.yaml

```yaml
components:
  demo-bazel:
    build_type: "Bazel"
    location: "demo-bazel"
    output_dir: "target/bazel-out"
    port: 8086
    target_port: 8086
    mount_point: "/demo-bazel"
    targets:
      - "//src:app"
    base_image: "python:3.11-slim"
```

### nginx.conf routing

```nginx
location /demo-bazel/ {
    proxy_pass http://demo-bazel:8086/;
}
```

## Advanced Usage

### Multiple Targets

Build multiple targets in a single component:

```yaml
my-component:
  build_type: "Bazel"
  targets:
    - "//src:server"
    - "//src:worker"
    - "//lib:utils"
```

### Custom Build Arguments

Pass additional arguments to Bazel:

```yaml
my-component:
  build_type: "Bazel"
  additional_args:
    - "--config=release"
    - "--define=version=1.0.0"
```

### Using Bzlmod (MODULE.bazel)

Rush supports both traditional WORKSPACE and the newer Bzlmod system. Simply include a `MODULE.bazel` file in your workspace:

```python
module(
    name = "my_component",
    version = "1.0.0",
)

bazel_dep(name = "rules_python", version = "0.27.0")
```

## Troubleshooting

### "No WORKSPACE or MODULE.bazel found"

Ensure your component's `location` directory contains either a `WORKSPACE` or `MODULE.bazel` file.

### Build outputs not found

1. Check that your targets are correct
2. Verify the `output_dir` matches where you expect outputs
3. Run `bazel build` manually to see where outputs are placed

### Permission denied on bazel-bin

Bazel creates symlinks with specific permissions. Ensure your user has access to the Bazel cache directory.

### Container fails to start

1. Verify the `base_image` has all required dependencies
2. Check that the built binary/script is executable
3. Review container logs with `rush logs <component>`

# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Rush is a Rust-based deployment tool that bridges development and production environments, enabling cross-compilation of x86 Docker images on ARM64 platforms and simplifying multi-container workflows. It manages builds, deployments, and secrets across multiple environments (local, dev, staging, production).

## Commands

### Building and Testing
```bash
# Build the Rush CLI
cargo build --release

# Run tests (use serial execution for integration tests)
cargo test
cargo test -- --test-threads=1  # For integration tests that interact with Docker

# Build a specific product
./target/release/rush io.wonop.helloworld build

# Start development environment
./target/release/rush io.wonop.helloworld dev

# Deploy to environment
./target/release/rush --env staging io.wonop.helloworld deploy
```

### Development Commands
```bash
# Check dependencies
./target/release/rush check-deps

# Initialize secrets for a product
./target/release/rush io.wonop.helloworld secrets init

# Validate Kubernetes manifests
./target/release/rush validate manifests

# Describe configurations
./target/release/rush describe toolchain
./target/release/rush describe images
```

## Architecture

### Workspace Structure
The project uses a Rust workspace with 14 specialized crates:
- `rush-cli`: Main CLI entry point and command orchestration
- `rush-core`: Core types, constants, and error definitions shared across all crates
- `rush-config`: Configuration loading from YAML files and environment resolution
- `rush-container`: Docker container lifecycle management via the ContainerReactor pattern
- `rush-build`: Build orchestration for different build types (RustBinary, TrunkWasm, Image)
- `rush-k8s`: Kubernetes manifest generation and deployment operations
- `rush-security`: Secret management with vault adapters (1Password, JSON, environment)
- `rush-toolchain`: Platform detection and cross-compilation toolchain management

### Key Patterns
- **ContainerReactor**: Central orchestrator in `rush-container` manages container lifecycle events asynchronously
- **BuildType enum**: Polymorphic build handling for RustBinary, TrunkWasm, Image, Ingress, LocalService
- **Arc<T> sharing**: Configuration and context shared across async tasks using Arc
- **Template-based configs**: Tera templates render dynamic configurations with environment variables

### Product Configuration
Products are defined in `products/<product-name>/stack.spec.yaml`:
```yaml
frontend:
  build_type: "TrunkWasm"
  location: "frontend/webui"
  dockerfile: "frontend/Dockerfile"
  mount_point: "/"

backend:
  build_type: "RustBinary"
  location: "backend/server"
  dockerfile: "backend/Dockerfile"
  mount_point: "/api"
```

### Cross-Compilation
Rush automatically handles cross-compilation on Apple Silicon:
- Detects platform via `rush-toolchain`
- Sets up Docker buildx for multi-platform builds
- Targets linux/amd64 for deployment compatibility

## Testing Approach

Integration tests in `rush/crates/rush-cli/tests/` focus on:
- Docker container operations
- Configuration loading and validation
- Build system functionality
- Container reactor behavior

Use `serial_test` crate to prevent test conflicts when interacting with Docker.

## Secret Management

Rush integrates with multiple secret stores:
- **Local**: JSON files in `.rush/vault/`
- **1Password**: Via op CLI integration
- **Kubernetes**: Kubeseal for encrypted secrets

Initialize secrets with: `rush <product> secrets init`

## Important Files
- `rushd.yaml`: Global Rush configuration
- `products/*/stack.spec.yaml`: Product-specific stack definitions
- `rush/crates/rush-cli/src/commands/`: CLI command implementations
- `rush/crates/rush-container/src/reactor.rs`: Container orchestration logic
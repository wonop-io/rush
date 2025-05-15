#!/bin/bash

# Create directory structure
mkdir -p src/utils
mkdir -p src/toolchain
mkdir -p src/core/config
mkdir -p src/core/environment
mkdir -p src/core/product
mkdir -p src/security/vault
mkdir -p src/security/secrets
mkdir -p src/build/templates/build
mkdir -p src/container/lifecycle
mkdir -p src/container/build
mkdir -p src/container/watcher
mkdir -p src/k8s
mkdir -p src/cli/commands

# Touch all files

# Phase 1: Core Utilities and Base Types
touch src/error.rs
touch src/utils/fs.rs
touch src/utils/path.rs
touch src/utils/template.rs
touch src/utils/git.rs
touch src/utils/process.rs
touch src/utils/directory.rs
touch src/utils/docker_cross.rs
touch src/utils/path_matcher.rs
touch src/utils/mod.rs
touch src/toolchain/platform.rs
touch src/toolchain/context.rs
touch src/toolchain/mod.rs

# Phase 2: Foundation Components
touch src/core/config/loader.rs
touch src/core/config/types.rs
touch src/core/config/validator.rs
touch src/core/config/mod.rs
touch src/core/environment/setup.rs
touch src/core/environment/variables.rs
touch src/core/environment/mod.rs
touch src/core/product/types.rs
touch src/core/product/loader.rs
touch src/core/product/mod.rs
touch src/core/dotenv.rs
touch src/core/types.rs
touch src/core/mod.rs

# Phase 3: Security Components
touch src/security/vault/dotenv.rs
touch src/security/vault/file.rs
touch src/security/vault/onepassword.rs
touch src/security/vault/adapter.rs
touch src/security/vault/trait.rs
touch src/security/vault/mod.rs
touch src/security/secrets/definitions.rs
touch src/security/secrets/provider.rs
touch src/security/secrets/encoder.rs
touch src/security/secrets/adapter.rs
touch src/security/secrets/mod.rs
touch src/security/env_defs.rs
touch src/security/mod.rs

# Phase 4: Build System
touch src/build/context.rs
touch src/build/script.rs
touch src/build/artefact.rs
touch src/build/variables.rs
touch src/build/types.rs
touch src/build/spec.rs
touch src/build/build_type.rs
touch src/build/templates/build/mdbook.sh
touch src/build/templates/build/rust_binary.sh
touch src/build/templates/build/wasm_dixious.sh
touch src/build/templates/build/wasm_trunk.sh
touch src/build/templates/build/zola.sh
touch src/build/templates/mod.rs
touch src/build/mod.rs

# Phase 5: Container Management
touch src/container/docker.rs
touch src/container/network.rs
touch src/container/service.rs
touch src/container/status.rs
touch src/container/lifecycle/launch.rs
touch src/container/lifecycle/monitor.rs
touch src/container/lifecycle/shutdown.rs
touch src/container/lifecycle/mod.rs
touch src/container/build/processor.rs
touch src/container/build/error.rs
touch src/container/build/mod.rs
touch src/container/watcher/setup.rs
touch src/container/watcher/processor.rs
touch src/container/watcher/mod.rs
touch src/container/reactor.rs
touch src/container/mod.rs

# Phase 6: Kubernetes Support
touch src/k8s/context.rs
touch src/k8s/manifests.rs
touch src/k8s/deployment.rs
touch src/k8s/validation.rs
touch src/k8s/encoder.rs
touch src/k8s/infrastructure.rs
touch src/k8s/minikube.rs
touch src/k8s/mod.rs

# Phase 7: CLI Layer
touch src/cli/args.rs
touch src/cli/commands/describe.rs
touch src/cli/commands/dev.rs
touch src/cli/commands/build.rs
touch src/cli/commands/deploy.rs
touch src/cli/commands/vault.rs
touch src/cli/commands/rollout.rs
touch src/cli/commands/apply.rs
touch src/cli/commands/unapply.rs
touch src/cli/commands/validate.rs
touch src/cli/commands/minikube.rs
touch src/cli/commands/mod.rs
touch src/cli/mod.rs

# Phase 8: Integration
touch src/lib.rs
touch src/main.rs

echo "All files have been created successfully!"

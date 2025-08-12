#!/bin/bash
set -e

echo "Starting Rush workspace migration..."

# Backup current Cargo.toml
cp Cargo.toml Cargo.toml.backup

# Create rush-utils lib.rs
cat > crates/rush-utils/src/lib.rs << 'EOF'
//! Rush Utils - General utilities and helpers

pub mod command;
pub mod directory;
pub mod docker_cross;
pub mod path;
pub mod path_matcher;
pub mod version;

pub use directory::Directory;
pub use docker_cross::DockerCrossCompileGuard;
pub use path::expand_path;
pub use path_matcher::{PathMatcher, Pattern};

/// Run a command with proper error handling
pub async fn run_command(
    name: &str,
    command: &str,
    args: Vec<&str>,
) -> Result<String, anyhow::Error> {
    command::run_command(name, command, args).await
        .map_err(|e| anyhow::anyhow!("Command failed: {}", e))
}
EOF

# Create rush-toolchain
echo "Creating rush-toolchain..."
cat > crates/rush-toolchain/Cargo.toml << 'EOF'
[package]
name = "rush-toolchain"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
rush-core = { workspace = true }
rush-utils = { workspace = true }
anyhow = { workspace = true }
log = { workspace = true }
which = { workspace = true }
serde = { workspace = true }
sha2 = { workspace = true }
hex = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
EOF

cp -r src/toolchain/* crates/rush-toolchain/src/ 2>/dev/null || true
echo "pub mod context;" > crates/rush-toolchain/src/lib.rs
echo "pub use context::ToolchainContext;" >> crates/rush-toolchain/src/lib.rs

# Create rush-config
echo "Creating rush-config..."
cat > crates/rush-config/Cargo.toml << 'EOF'
[package]
name = "rush-config"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
rush-core = { workspace = true }
rush-utils = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
serde = { workspace = true }
serde_yaml = { workspace = true }
serde_json = { workspace = true }
toml = { workspace = true }
log = { workspace = true }
directories = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
EOF

cp -r src/core/* crates/rush-config/src/ 2>/dev/null || true

# Create rush-security
echo "Creating rush-security..."
cat > crates/rush-security/Cargo.toml << 'EOF'
[package]
name = "rush-security"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
rush-core = { workspace = true }
rush-config = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
log = { workspace = true }
base64 = { workspace = true }
sha2 = { workspace = true }
hex = { workspace = true }
async-trait = { workspace = true }
tokio = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
tokio-test = "0.4"
EOF

cp -r src/security/* crates/rush-security/src/ 2>/dev/null || true

# Create rush-build
echo "Creating rush-build..."
cat > crates/rush-build/Cargo.toml << 'EOF'
[package]
name = "rush-build"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
rush-core = { workspace = true }
rush-config = { workspace = true }
rush-toolchain = { workspace = true }
rush-utils = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
serde = { workspace = true }
serde_yaml = { workspace = true }
log = { workspace = true }
handlebars = { workspace = true }
tera = { workspace = true }
regex = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
EOF

cp -r src/build/* crates/rush-build/src/ 2>/dev/null || true

# Create rush-output
echo "Creating rush-output..."
cat > crates/rush-output/Cargo.toml << 'EOF'
[package]
name = "rush-output"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
rush-core = { workspace = true }
anyhow = { workspace = true }
log = { workspace = true }
console = { workspace = true }
indicatif = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
chrono = { workspace = true }

[dev-dependencies]
tokio-test = "0.4"
EOF

cp -r src/output/* crates/rush-output/src/ 2>/dev/null || true

# Create rush-container
echo "Creating rush-container..."
cat > crates/rush-container/Cargo.toml << 'EOF'
[package]
name = "rush-container"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
rush-core = { workspace = true }
rush-config = { workspace = true }
rush-build = { workspace = true }
rush-toolchain = { workspace = true }
rush-security = { workspace = true }
rush-output = { workspace = true }
rush-utils = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
serde = { workspace = true }
log = { workspace = true }
tokio = { workspace = true }
async-trait = { workspace = true }
bollard = { workspace = true }
notify = { workspace = true }
futures = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
tokio-test = "0.4"
EOF

cp -r src/container/* crates/rush-container/src/ 2>/dev/null || true

# Create rush-k8s
echo "Creating rush-k8s..."
cat > crates/rush-k8s/Cargo.toml << 'EOF'
[package]
name = "rush-k8s"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
rush-core = { workspace = true }
rush-config = { workspace = true }
rush-build = { workspace = true }
rush-container = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
serde = { workspace = true }
serde_yaml = { workspace = true }
serde_json = { workspace = true }
log = { workspace = true }
tokio = { workspace = true }
k8s-openapi = { workspace = true }
kube = { workspace = true }
handlebars = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
EOF

cp -r src/k8s/* crates/rush-k8s/src/ 2>/dev/null || true

# Create rush-cli
echo "Creating rush-cli..."
cat > crates/rush-cli/Cargo.toml << 'EOF'
[package]
name = "rush-cli"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "rush"
path = "src/main.rs"

[dependencies]
rush-core = { workspace = true }
rush-utils = { workspace = true }
rush-config = { workspace = true }
rush-toolchain = { workspace = true }
rush-security = { workspace = true }
rush-build = { workspace = true }
rush-output = { workspace = true }
rush-container = { workspace = true }
rush-k8s = { workspace = true }

anyhow = { workspace = true }
thiserror = { workspace = true }
clap = { workspace = true }
tokio = { workspace = true }
log = { workspace = true }
env_logger = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
console = { workspace = true }
dialoguer = { workspace = true }
directories = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
EOF

# Move CLI files
cp -r src/cli/* crates/rush-cli/src/ 2>/dev/null || true
cp src/main.rs crates/rush-cli/src/main.rs 2>/dev/null || true
cp src/lib.rs crates/rush-cli/src/lib.rs.old 2>/dev/null || true

# Update workspace Cargo.toml
mv Cargo.toml.new Cargo.toml

echo "Migration complete! Now you need to:"
echo "1. Update all import statements in the moved files"
echo "2. Fix any compilation errors"
echo "3. Move tests to appropriate crates"
echo "4. Run 'cargo build' to verify everything works"
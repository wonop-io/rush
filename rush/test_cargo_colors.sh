#!/bin/bash

# Test if cargo outputs colors with our environment variables

echo "Testing cargo color output with environment variables..."
echo

# Set the environment variables we use in Rush
export FORCE_COLOR=1
export CARGO_TERM_COLOR=always
export RUST_LOG_STYLE=always
export CLICOLOR_FORCE=1
export COLORTERM=truecolor
export TERM=${TERM:-xterm-256color}
unset NO_COLOR

# Create a simple Rust project with an intentional error
TEMP_DIR=$(mktemp -d)
cd "$TEMP_DIR"

cat > Cargo.toml << 'EOF'
[package]
name = "test"
version = "0.1.0"
edition = "2021"
EOF

mkdir -p src
cat > src/main.rs << 'EOF'
fn main() {
    lets break shit  // Intentional error
}
EOF

echo "Running cargo build with color environment variables..."
cargo build 2>&1 | head -20

cd - > /dev/null
rm -rf "$TEMP_DIR"

echo
echo "Test complete!"
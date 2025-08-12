#\!/bin/bash
# Test that rush can access templates for a wasm_trunk build

# Create a minimal rush.yaml for testing
cat > rush_test.yaml << 'YAML'
name: test-product
version: 1.0.0
domain: test.local
components:
  test-wasm:
    type: wasm_trunk
    location: test-wasm
    domain: test.local
YAML

# Try to run rush in dry-run mode to see if templates load
RUST_LOG=debug ./target/release/rush test-product build --dry-run 2>&1 | grep -E "template|Template" | head -5

rm -f rush_test.yaml

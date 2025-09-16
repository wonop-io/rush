#!/bin/bash
set -e

echo "Testing hash fix for watch patterns..."

# Build the fixed version
echo "Building Rush with the fix..."
cd rush
cargo build --release --quiet
cd ..

# Create a test product structure
TEST_DIR="/tmp/rush-test-$$"
mkdir -p "$TEST_DIR/test-product/frontend/src"
cd "$TEST_DIR"

# Create a basic stack.spec.yaml with restrictive watch patterns
cat > test-product/stack.spec.yaml <<EOF
frontend:
  build_type: TrunkWasm
  location: frontend
  dockerfile: frontend/Dockerfile
  watch:
    - "**/*_screens"
    - "**/*_api"
EOF

# Create a Dockerfile
cat > test-product/frontend/Dockerfile <<EOF
FROM scratch
COPY . /app
EOF

# Create a source file that doesn't match watch patterns
echo "fn main() {}" > test-product/frontend/src/lib.rs

# Initialize git repo (required for hash computation)
cd test-product
git init
git add .
git commit -m "Initial commit" --quiet

# Modify the file to mark it dirty
echo "fn main() { println!(\"test\"); }" > frontend/src/lib.rs

# Run rush to compute the tag (with increased logging)
echo "Computing tag with modified source file..."
RUST_LOG=debug,rush_container::tagging=trace "$OLDPWD/rush/target/release/rush" describe images 2>&1 | grep -E "(Computing hash|Found|Total files|Generated|frontend.*tag)" || true

# Check if the hash is NOT the empty hash
OUTPUT=$(RUST_LOG=warn "$OLDPWD/rush/target/release/rush" describe images 2>&1 | grep frontend || echo "")
if echo "$OUTPUT" | grep -q "e3b0c442"; then
    echo "❌ FAILED: Still generating empty hash e3b0c442"
    echo "Output: $OUTPUT"
    exit 1
else
    echo "✅ SUCCESS: Not generating empty hash"
    echo "Output: $OUTPUT"
fi

# Cleanup
cd /
rm -rf "$TEST_DIR"

echo "Test completed successfully!"
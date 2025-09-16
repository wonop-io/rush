#!/bin/bash
set -e

echo "Testing that hash changes when files are modified..."

# Create a test product structure
TEST_DIR="/tmp/rush-hash-test-$$"
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

# Create source files
echo "fn main() { println!(\"version 1\"); }" > test-product/frontend/src/lib.rs
echo "fn api() {}" > test-product/frontend/src/test_api.rs

# Initialize git repo
cd test-product
git init --quiet
git add .
git commit -m "Initial" --quiet

# Get initial hash
echo "Initial state - lib.rs has 'version 1'"
HASH1=$(RUST_LOG=error /Users/tfr/Documents/Projects/rush/rush/target/release/rush describe images 2>&1 | grep frontend | awk '{print $2}' | cut -d: -f2)
echo "Hash 1: $HASH1"

# Modify lib.rs (doesn't match watch patterns)
echo "fn main() { println!(\"version 2\"); }" > frontend/src/lib.rs
echo "Modified lib.rs to 'version 2'"

# Get hash after modifying lib.rs
HASH2=$(RUST_LOG=error /Users/tfr/Documents/Projects/rush/rush/target/release/rush describe images 2>&1 | grep frontend | awk '{print $2}' | cut -d: -f2)
echo "Hash 2: $HASH2"

# Modify test_api.rs (matches watch pattern)
echo "fn api() { println!(\"api v2\"); }" > frontend/src/test_api.rs
echo "Modified test_api.rs (matches watch pattern)"

# Get hash after modifying test_api.rs
HASH3=$(RUST_LOG=error /Users/tfr/Documents/Projects/rush/rush/target/release/rush describe images 2>&1 | grep frontend | awk '{print $2}' | cut -d: -f2)
echo "Hash 3: $HASH3"

# Verify results
echo ""
echo "Results:"
echo "========"

# Check that hash is not empty
if [[ "$HASH1" == *"e3b0c442"* ]]; then
    echo "❌ FAILED: Initial hash is empty string hash"
    exit 1
fi

# Check that hash changes when lib.rs changes (main fix)
if [[ "$HASH1" == "$HASH2" ]]; then
    echo "❌ FAILED: Hash didn't change when lib.rs was modified"
    echo "   This means component files are not being included!"
    exit 1
else
    echo "✅ SUCCESS: Hash changed when lib.rs was modified"
    echo "   Component files are being included despite watch patterns"
fi

# Check that hash changes when watched file changes
if [[ "$HASH2" == "$HASH3" ]]; then
    echo "❌ FAILED: Hash didn't change when test_api.rs was modified"
    exit 1
else
    echo "✅ SUCCESS: Hash changed when watched file was modified"
    echo "   Watch patterns are still working for additional files"
fi

# Show all hashes
echo ""
echo "Hash progression:"
echo "  Initial:           $HASH1"
echo "  After lib.rs:      $HASH2"
echo "  After test_api.rs: $HASH3"

# Cleanup
cd /
rm -rf "$TEST_DIR"

echo ""
echo "All tests passed! The fix is working correctly."
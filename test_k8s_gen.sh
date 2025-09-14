#!/bin/bash

# Test K8s manifest generation

echo "Testing K8s manifest generation..."

# Clean up previous manifests
rm -rf .rush/k8s

# Set environment variables for testing
export RUSHD_ROOT=$(pwd)
export RUSH_ENV=local
export K8S_NAMESPACE=test-namespace
export INFRASTRUCTURE_REPOSITORY="git@github.com:test/infra.git"

echo "Running build to generate manifests..."
# Since rollout needs git repo, let's directly test manifest generation
# by running a simplified test

# Create a test program to call build_manifests
cat > test_manifests.rs << 'EOF'
// This would be a test to verify manifest generation
// For now, we'll just check if the structure is created
EOF

# Try to trigger manifest generation through the build command
./rush/target/release/rush io.wonop.helloworld build 2>&1 | grep -E "manifest|k8s" || true

# Check if manifests were generated
echo -e "\nChecking .rush/k8s directory structure:"
if [ -d ".rush/k8s" ]; then
    echo "✓ .rush/k8s directory exists"
    ls -la .rush/k8s/ 2>/dev/null || echo "  Directory is empty"

    # Check for component directories with priority prefixes
    for dir in .rush/k8s/*/; do
        if [ -d "$dir" ]; then
            echo "  Found component directory: $(basename $dir)"
            ls -la "$dir" | head -5
        fi
    done
else
    echo "✗ .rush/k8s directory not found"
    echo "  Note: build command may not trigger manifest generation"
fi

echo -e "\nDone!"
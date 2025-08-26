#!/bin/bash

# Test script for Docker push functionality
set -e

echo "Testing Docker push functionality..."

# Build a simple test image
echo "Creating test Dockerfile..."
cat > /tmp/test_dockerfile <<EOF
FROM alpine:latest
RUN echo "Test image for Rush Docker push"
EOF

echo "Building test image..."
docker build -t rush-test-push:latest -f /tmp/test_dockerfile /tmp/

echo "Tagging image for local registry..."
docker tag rush-test-push:latest localhost:5000/rush-test-push:latest

echo "Image built and tagged successfully!"
echo ""
echo "To test push functionality:"
echo "1. Start a local registry: docker run -d -p 5000:5000 registry:2"
echo "2. Run: docker push localhost:5000/rush-test-push:latest"
echo ""
echo "Or use the Rush reactor's build_and_push method with registry config:"
echo "  - registry.url: localhost:5000"
echo "  - registry.namespace: test"

# Clean up
rm -f /tmp/test_dockerfile

echo "Test preparation complete!"
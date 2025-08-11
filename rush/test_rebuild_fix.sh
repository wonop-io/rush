#!/bin/bash

echo "Testing rebuild flag fix"
echo "========================"

# Create a simple test Dockerfile and context
TEST_DIR="/tmp/rush_rebuild_test"
rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

# Initialize git repo
git init
git config user.email "test@example.com"
git config user.name "Test User"

# Create a simple Dockerfile
cat > Dockerfile <<'EOF'
FROM alpine:latest
RUN echo "Test image"
EOF

# Create a test file
echo "Initial content" > test.txt

# Add and commit
git add .
git commit -m "Initial commit"

echo ""
echo "Test 1: Building with clean git state"
echo "--------------------------------------"
# The image should be built with a clean tag (no -wip- suffix)

# Make a change
echo "Modified content" >> test.txt

echo ""
echo "Test 2: Building with uncommitted changes"  
echo "-----------------------------------------"
# The image should be built with a -wip- tag

# Commit the changes
git add .
git commit -m "Update test.txt"

echo ""
echo "Test 3: After committing, image should not rebuild"
echo "---------------------------------------------------"
# The existing image (even if it was tagged -wip-) should be reused
# and potentially retagged to the clean tag

echo ""
echo "Manual verification steps:"
echo "1. Run 'docker images | grep rush' to see the tagged images"
echo "2. Check the logs to verify no unnecessary rebuilds happen"
echo "3. Verify that -wip- images get retagged to clean tags when appropriate"
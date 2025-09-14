#!/bin/bash

# Test script for the rollout command implementation
# This script verifies that the rollout command can be called and handles errors properly

echo "Testing Rush CLI rollout command..."
echo "=================================="

# Set up test environment variables
export RUSHD_ROOT=$(pwd)
export INFRASTRUCTURE_REPOSITORY="git@github.com:test/infra.git"
export DOCKER_REGISTRY="test-registry.io"
export RUSH_ENV="staging"

# Test 1: Check if rollout command is recognized
echo -e "\n1. Testing if rollout command is available..."
./rush/target/release/rush --help | grep -q "rollout" && echo "✓ Rollout command found" || echo "✗ Rollout command not found"

# Test 2: Try to run rollout without a product name (should fail)
echo -e "\n2. Testing rollout without product name..."
./rush/target/release/rush rollout 2>&1 | head -5

# Test 3: Check rollout help text
echo -e "\n3. Checking rollout command description..."
./rush/target/release/rush --help | grep -A1 "rollout"

echo -e "\n=================================="
echo "Test complete!"

# Note: A full integration test would require:
# - A valid product in the products/ directory
# - Docker daemon running
# - Valid Git repository for infrastructure
# - Proper secrets configured in vault
# - Kubernetes configuration
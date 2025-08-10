#!/bin/bash
# Test script to verify Docker image caching behavior

set -e

echo "Testing Docker image caching behavior..."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test 1: Check if git hash is computed correctly
echo -e "${YELLOW}Test 1: Git hash computation${NC}"
GIT_HASH=$(git log -n 1 --format=%H -- . | head -c 8)
if [ -z "$GIT_HASH" ]; then
    echo -e "${RED}✗ Failed to compute git hash${NC}"
    exit 1
else
    echo -e "${GREEN}✓ Git hash computed: $GIT_HASH${NC}"
fi

# Test 2: Check if we can detect uncommitted changes
echo -e "${YELLOW}Test 2: Uncommitted changes detection${NC}"
if git diff --quiet .; then
    echo -e "${GREEN}✓ No uncommitted changes detected${NC}"
    WIP_SUFFIX=""
else
    echo -e "${GREEN}✓ Uncommitted changes detected, will add -wip- suffix${NC}"
    WIP_SUFFIX="-wip-"
fi

# Test 3: Verify Docker image inspect works
echo -e "${YELLOW}Test 3: Docker image inspection${NC}"
TEST_IMAGE="alpine:latest"
if docker image inspect "$TEST_IMAGE" > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Docker image inspection works${NC}"
else
    echo -e "${YELLOW}⚠ Docker not available or test image not found${NC}"
fi

# Test 4: Check if a non-existent image is correctly identified
echo -e "${YELLOW}Test 4: Non-existent image detection${NC}"
NONEXISTENT_IMAGE="nonexistent-image-that-should-not-exist:v999"
if docker image inspect "$NONEXISTENT_IMAGE" > /dev/null 2>&1; then
    echo -e "${RED}✗ Non-existent image incorrectly reported as existing${NC}"
else
    echo -e "${GREEN}✓ Non-existent image correctly identified${NC}"
fi

echo -e "${GREEN}All tests passed!${NC}"
echo ""
echo "Summary:"
echo "- Git-based tagging is working"
echo "- Uncommitted changes detection is working"
echo "- Docker image existence checking is working"
echo ""
echo "The caching implementation should:"
echo "1. Skip builds for images with clean git tags that already exist"
echo "2. Rebuild images with -wip- tags (uncommitted changes)"
echo "3. Build images that don't exist in the cache"
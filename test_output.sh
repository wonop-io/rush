#!/bin/bash

echo "Testing Rush output formats..."

RUSH_BIN="/Users/tfr/Documents/Projects/rush/rush/target/release/rush"

echo ""
echo "=== Testing --output-format split ==="
echo "You should see [BUILD] and [RUNTIME] prefixes"
echo "Command: $RUSH_BIN io.wonop.helloworld dev --output-format split"
echo ""
echo "Press Ctrl+C after seeing a few lines of output"
echo ""

# Note: This will actually start the dev environment
# You'll need to manually verify the output format

echo "=== Testing colors ==="
echo "Docker containers should now preserve their original colors"
echo "because we added -t flag to docker run command"
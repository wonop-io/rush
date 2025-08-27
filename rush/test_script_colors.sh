#!/bin/bash

# Test script to verify that script output preserves original formatting
# and doesn't show everything in red

echo "Testing script output formatting..."
echo

# Test normal stdout output
echo "This is normal stdout output - should not be red"

# Test stderr output (should also not be red for scripts)
echo "This is stderr output - should not be red for scripts" >&2

# Test colored output with ANSI codes
echo -e "\033[32mThis is green text\033[0m"
echo -e "\033[33mThis is yellow text\033[0m"
echo -e "\033[36mThis is cyan text\033[0m"

# Test progress indicator with carriage return
echo -n "Progress: "
for i in {1..5}; do
    echo -ne "\rProgress: $i/5"
    sleep 0.2
done
echo " Done!"

# Test mixed output
echo "Build starting..."
echo "Warning: This is a warning message" >&2
echo -e "\033[32m✓ Build completed successfully\033[0m"

echo
echo "Script test completed!"
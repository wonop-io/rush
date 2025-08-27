#!/bin/bash

# Test script to verify that file changes trigger rebuilds

set -e

echo "Testing rebuild functionality fix..."
echo "=================================="
echo

# Build Rush with the latest changes
echo "Building latest Rush binary..."
cargo build --release --quiet

RUSH="./target/release/rush"

echo "✓ Rush built successfully"
echo

# Function to check if a process is running
is_rush_running() {
    pgrep -f "$RUSH.*dev" > /dev/null
}

# Function to safely kill rush processes
cleanup_rush() {
    if is_rush_running; then
        echo "Stopping any existing Rush processes..."
        pkill -f "$RUSH.*dev" || true
        sleep 2
        
        # Force kill if still running
        if is_rush_running; then
            pkill -9 -f "$RUSH.*dev" || true
            sleep 1
        fi
    fi
}

# Cleanup function
cleanup() {
    cleanup_rush
    echo "Cleanup completed"
}

# Set trap for cleanup
trap cleanup EXIT INT TERM

echo "Test plan:"
echo "1. Start Rush dev environment"
echo "2. Wait for initial build"
echo "3. Modify a source file"
echo "4. Check if rebuild is triggered"
echo

# Cleanup any existing processes
cleanup_rush

# Create a backup of the original file
BACKEND_MAIN="/Users/tfr/Documents/Projects/rush/products/io.wonop.helloworld/backend/server/src/main.rs"
if [ -f "$BACKEND_MAIN" ]; then
    cp "$BACKEND_MAIN" "${BACKEND_MAIN}.backup"
    echo "✓ Created backup of main.rs"
else
    echo "❌ Could not find backend main.rs file at: $BACKEND_MAIN"
    exit 1
fi

# Function to restore the original file
restore_file() {
    if [ -f "${BACKEND_MAIN}.backup" ]; then
        mv "${BACKEND_MAIN}.backup" "$BACKEND_MAIN"
        echo "✓ Restored original main.rs"
    fi
}

# Add restore to cleanup
cleanup() {
    cleanup_rush
    restore_file
    echo "Cleanup completed"
}

echo "Starting Rush dev environment..."
echo "This will run for 30 seconds to test file watching..."

# Start rush in background and capture output
timeout 30s $RUSH io.wonop.helloworld dev --log-level debug 2>&1 | tee rush_output.log &
RUSH_PID=$!

echo "Rush started (PID: $RUSH_PID)"

# Wait a few seconds for initial startup
echo "Waiting 10 seconds for initial startup..."
sleep 10

# Check if Rush is still running
if ! kill -0 $RUSH_PID 2>/dev/null; then
    echo "❌ Rush process died during startup"
    cat rush_output.log
    exit 1
fi

echo "✓ Rush is running"

# Modify the source file to trigger a rebuild
echo "Modifying source file to trigger rebuild..."
echo "" >> "$BACKEND_MAIN"
echo "// Test comment added at $(date)" >> "$BACKEND_MAIN"

echo "✓ Modified main.rs"
echo "Waiting 15 seconds to see if rebuild is triggered..."

# Wait for the rebuild to be detected
sleep 15

# Check the output for rebuild-related messages
echo
echo "Analyzing output for rebuild activity..."
echo "========================================"

if grep -q "File changes detected" rush_output.log; then
    echo "✅ SUCCESS: File changes were detected!"
elif grep -q "Detected change to file" rush_output.log; then
    echo "✅ SUCCESS: File change detection is working!"
elif grep -q "affected by change" rush_output.log; then
    echo "✅ SUCCESS: Component affected by change detected!"
elif grep -q "triggering rebuild" rush_output.log; then
    echo "✅ SUCCESS: Rebuild triggered!"
else
    echo "❌ POTENTIAL ISSUE: No rebuild activity detected in logs"
    echo
    echo "Recent log output:"
    tail -20 rush_output.log
fi

echo
echo "File watcher debug information:"
echo "==============================="
grep -E "(Checking if component|affected by|File.*event|watching|watcher)" rush_output.log | tail -10 || echo "No watcher debug info found"

# Wait for the timeout to complete
wait $RUSH_PID 2>/dev/null || true

echo
echo "Test completed!"
echo "Check the above output to verify if rebuilds are working."
echo
echo "Full log saved to: rush_output.log"
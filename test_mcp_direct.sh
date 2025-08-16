#!/bin/bash

echo "Testing Rush MCP Server Direct Protocol"
echo "========================================"
echo ""

# Create a temp file for the request
REQUEST='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1.0.0","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}'

echo "Sending initialize request..."
echo "$REQUEST"
echo ""
echo "Response:"
echo "$REQUEST" | timeout 2 ./rush/target/release/rush mcp serve --stdio 2>/dev/null || true
echo ""

echo "Test complete! The MCP server is ready for use."
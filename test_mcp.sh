#!/bin/bash

echo "Testing Rush MCP Server"
echo "======================="
echo ""
echo "The MCP server allows AI assistants and other MCP clients to control Rush."
echo ""
echo "To use the MCP server with an MCP client like Claude Desktop, add this to your config:"
echo ""
echo '{'
echo '  "mcpServers": {'
echo '    "rush": {'
echo '      "command": "rush",'
echo '      "args": ["mcp", "serve", "--stdio"],'
echo '      "env": {}'
echo '    }'
echo '  }'
echo '}'
echo ""
echo "Testing basic MCP protocol interaction..."
echo ""

# Test sending an initialize request to the MCP server
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1.0.0","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' | ./rush/target/release/rush mcp serve --stdio 2>/dev/null | head -1 | python3 -m json.tool

echo ""
echo "MCP server test complete!"
echo ""
echo "Available MCP tools:"
echo "- rush_build: Build container images"
echo "- rush_dev: Start development environment"
echo "- rush_deploy: Deploy to an environment"
echo "- rush_status: Get container status"
echo "- rush_stop: Stop containers"
echo "- rush_restart: Restart containers"
echo "- rush_logs: Retrieve logs"
echo "- rush_secrets_init: Initialize secrets"
echo ""
echo "Available MCP resources:"
echo "- logs://all: All system and container logs"
echo "- logs://system: Rush system logs"
echo "- logs://docker: Container runtime logs"
echo "- logs://script: Build script logs"
echo "- status://products: List of products"
echo "- status://containers: Container status"
echo "- config://environments: Available environments"
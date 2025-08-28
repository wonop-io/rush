#!/bin/bash
# Test script to verify .env.secrets files are loaded

echo "Testing .env.secrets loading..."

# Create a test product directory
TEST_DIR="/tmp/test_env_secrets_product"
rm -rf $TEST_DIR
mkdir -p $TEST_DIR/test_component

# Create a test .env file
cat > $TEST_DIR/test_component/.env << EOF
REGULAR_VAR=from_env_file
OVERRIDE_VAR=from_env_file
EOF

# Create a test .env.secrets file
cat > $TEST_DIR/test_component/.env.secrets << EOF
SECRET_VAR=from_env_secrets_file
OVERRIDE_VAR=from_env_secrets_file
EOF

# Create a simple stack.spec.yaml
cat > $TEST_DIR/stack.spec.yaml << EOF
test_component:
  build_type: "PureDockerImage"
  image_name_with_tag: "alpine:latest"
  location: "test_component"
  port: 8080
  target_port: 8080
EOF

echo "Test files created in $TEST_DIR"
echo ""
echo "Contents of .env:"
cat $TEST_DIR/test_component/.env
echo ""
echo "Contents of .env.secrets:"
cat $TEST_DIR/test_component/.env.secrets
echo ""

echo "Now you can test with:"
echo "rush test_env_secrets_product dev"
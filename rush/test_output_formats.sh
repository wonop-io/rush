#!/bin/bash

# Test script for the new output formats
# This script demonstrates the different output formats available in rush dev

echo "=========================================="
echo "Rush Output Format Test"
echo "=========================================="
echo ""
echo "This script will show a sample of each output format."
echo "Note: You'll need a rush project set up to see real output."
echo ""

# Function to show command and explanation
show_command() {
    local format=$1
    local description=$2
    local extra_args=$3
    
    echo "=========================================="
    echo "Format: $format"
    echo "Description: $description"
    echo "Command: rush myapp dev --output-format $format $extra_args"
    echo "=========================================="
    echo ""
    
    # Uncomment the line below to actually run the command
    # timeout 5 rush myapp dev --output-format $format $extra_args 2>&1 | head -20
    
    echo "(Command would run here - uncomment the line in the script to test)"
    echo ""
}

# Test each format
show_command "auto" "Automatically selects best format based on terminal"
show_command "simple" "Traditional line-by-line output"
show_command "split" "Split view with BUILD and RUNTIME sections" 
show_command "dashboard" "Dashboard with widgets and metrics"
show_command "web" "Web browser view (would open http://localhost:8080)"

echo "=========================================="
echo "Testing with filters"
echo "=========================================="
echo ""

echo "Command: rush myapp dev --output-format split --filter-components backend --log-level debug"
echo "This would show only backend logs in split view with debug level"
echo ""

echo "=========================================="
echo "Configuration via rushd.yaml"
echo "=========================================="
echo ""
echo "You can also configure defaults in rushd.yaml:"
echo ""
cat << 'EOF'
dev_output:
  mode: split
  log_level: debug
  components:
    include: ["backend", "frontend"]
  phases:
    show_build: true
    show_runtime: true
  colors:
    enabled: auto
    theme: default
EOF

echo ""
echo "=========================================="
echo "Test complete!"
echo "=========================================="
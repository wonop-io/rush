#!/bin/bash

# Test that colors are preserved when running through Rush

echo "Testing color preservation in scripts..."
echo

# Test basic ANSI colors
echo -e "\033[31mThis should be RED\033[0m"
echo -e "\033[32mThis should be GREEN\033[0m"
echo -e "\033[33mThis should be YELLOW\033[0m"
echo -e "\033[34mThis should be BLUE\033[0m"
echo -e "\033[35mThis should be MAGENTA\033[0m"
echo -e "\033[36mThis should be CYAN\033[0m"

# Test bold and other styles
echo -e "\033[1mThis should be BOLD\033[0m"
echo -e "\033[4mThis should be UNDERLINED\033[0m"

# Test error-like output to stderr (should NOT be red if from script)
echo -e "\033[33mWarning:\033[0m This is a warning on stderr" >&2
echo -e "\033[31mError:\033[0m This is an error on stderr" >&2

# Test emojis
echo "🚀 Starting build..."
echo "✓ Build completed"
echo "❌ Build failed"

echo
echo "Color test complete!"
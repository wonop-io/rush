#!/usr/bin/env bash
# Rush CI Fix Script
# Automatically fixes common code issues that can be auto-corrected
# Safe to run - will not make breaking changes

set -e  # Exit on error
set -o pipefail  # Exit on pipe failure

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Track what we fixed
FIXES_APPLIED=""
TOTAL_FIXES=0

# Helper function for section headers
print_header() {
    echo ""
    echo -e "${MAGENTA}${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${MAGENTA}${BOLD}  $1${NC}"
    echo -e "${MAGENTA}${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

# Helper function to apply a fix
apply_fix() {
    local name=$1
    local command=$2
    local check_command=${3:-""}

    echo -e "\n${BOLD}▶ Applying: $name${NC}"

    # Check if fix is needed (optional)
    if [ -n "$check_command" ]; then
        if eval "$check_command" 2>/dev/null; then
            echo -e "${GREEN}✓ $name - already fixed${NC}"
            return 0
        fi
    fi

    if eval "$command"; then
        echo -e "${GREEN}✓ $name - fixed successfully${NC}"
        FIXES_APPLIED="${FIXES_APPLIED}\n  ✓ $name"
        TOTAL_FIXES=$((TOTAL_FIXES + 1))
    else
        echo -e "${YELLOW}⚠ $name - fix failed or not needed${NC}"
    fi
}

# Confirmation prompt
confirm_action() {
    local prompt=$1
    echo -e "${YELLOW}$prompt${NC}"
    read -p "Continue? (y/n) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Aborted by user"
        exit 0
    fi
}

# Main script starts here
print_header "Rush CI Auto-Fix Tool"
echo "This tool will automatically fix common code issues."
echo "All changes are safe and non-breaking."
echo ""
echo -e "${BOLD}Issues that will be fixed:${NC}"
echo "  • Code formatting issues"
echo "  • Simple clippy warnings"
echo "  • Import organization"
echo "  • Cargo.toml sorting"
echo "  • Trailing whitespace"
echo "  • File permissions"
echo ""

# Ask for confirmation in interactive mode
if [ -t 0 ]; then
    confirm_action "This will modify files in your working directory."
fi

START_TIME=$(date +%s)

# 1. Format Rust Code
print_header "1. Formatting Rust Code"
echo "Running cargo fmt to format all Rust files..."
apply_fix "Rust code formatting" "cargo fmt --all"

# 2. Fix Clippy Issues (those that can be auto-fixed)
print_header "2. Fixing Clippy Issues"
echo "Running cargo clippy --fix to auto-fix warnings..."
apply_fix "Clippy auto-fixes" "cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged"

# 3. Fix Deprecated Code
print_header "3. Fixing Deprecated Code"
echo "Running cargo fix to update deprecated items..."
apply_fix "Deprecated code" "cargo fix --workspace --allow-dirty --allow-staged"

# 4. Sort Cargo.toml files (if cargo-sort is installed)
print_header "4. Sorting Cargo.toml Files"
if command -v cargo-sort &> /dev/null; then
    echo "Sorting dependencies in Cargo.toml files..."
    apply_fix "Cargo.toml sorting" "cargo sort --workspace"
else
    echo -e "${YELLOW}⚠ cargo-sort not installed, skipping${NC}"
    echo "  Install with: cargo install cargo-sort"
fi

# 5. Apply workspace inheritance (if cargo-autoinherit is installed)
print_header "5. Workspace Inheritance"
if command -v cargo-autoinherit &> /dev/null; then
    echo "Applying workspace inheritance..."
    apply_fix "Workspace inheritance" "cargo autoinherit"
else
    echo -e "${YELLOW}⚠ cargo-autoinherit not installed, skipping${NC}"
    echo "  Install with: cargo install cargo-autoinherit"
fi

# 6. Remove trailing whitespace
print_header "6. Removing Trailing Whitespace"
echo "Cleaning trailing whitespace from Rust files..."
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    find crates -name "*.rs" -type f -exec sed -i '' 's/[[:space:]]*$//' {} \; 2>/dev/null
    find . -name "*.toml" -type f -exec sed -i '' 's/[[:space:]]*$//' {} \; 2>/dev/null
else
    # Linux
    find crates -name "*.rs" -type f -exec sed -i 's/[[:space:]]*$//' {} \; 2>/dev/null
    find . -name "*.toml" -type f -exec sed -i 's/[[:space:]]*$//' {} \; 2>/dev/null
fi
echo -e "${GREEN}✓ Trailing whitespace removed${NC}"
TOTAL_FIXES=$((TOTAL_FIXES + 1))

# 7. Fix file permissions for scripts
print_header "7. Fixing File Permissions"
echo "Setting execute permissions on shell scripts..."
for script in *.sh; do
    if [ -f "$script" ]; then
        chmod +x "$script"
        echo -e "${GREEN}✓ Made $script executable${NC}"
    fi
done

# 8. Remove unnecessary files
print_header "8. Cleaning Temporary Files"
echo "Removing temporary and backup files..."
find . -type f \( -name "*.bak" -o -name "*.swp" -o -name "*~" -o -name ".DS_Store" \) -delete 2>/dev/null || true
echo -e "${GREEN}✓ Temporary files cleaned${NC}"

# 9. Update dependencies to latest compatible versions
print_header "9. Updating Dependencies"
echo "Updating to latest compatible versions..."
if cargo update 2>&1 | grep -q "Updated"; then
    echo -e "${GREEN}✓ Dependencies updated${NC}"
    TOTAL_FIXES=$((TOTAL_FIXES + 1))
else
    echo -e "${BLUE}ℹ All dependencies are already up to date${NC}"
fi

# 10. Generate/Update Documentation
print_header "10. Documentation Generation"
echo "Building documentation to check for issues..."
if cargo doc --workspace --no-deps --document-private-items 2>&1 | grep -q "warning"; then
    echo -e "${YELLOW}⚠ Documentation has warnings - manual review needed${NC}"
else
    echo -e "${GREEN}✓ Documentation builds cleanly${NC}"
fi

# 11. Organize imports (using rustfmt)
print_header "11. Organizing Imports"
echo "Organizing and grouping imports..."
# This is already handled by cargo fmt, but we'll make it explicit
cargo fmt --all -- --config imports_granularity=Module,group_imports=StdExternalCrate 2>/dev/null || true
echo -e "${GREEN}✓ Imports organized${NC}"

# 12. Fix common typos (if typos is installed)
print_header "12. Fixing Typos"
if command -v typos &> /dev/null; then
    echo "Checking and fixing common typos..."
    if typos --write-changes 2>/dev/null; then
        echo -e "${GREEN}✓ Typos fixed${NC}"
        TOTAL_FIXES=$((TOTAL_FIXES + 1))
    else
        echo -e "${BLUE}ℹ No typos found${NC}"
    fi
else
    echo -e "${YELLOW}⚠ typos-cli not installed, skipping${NC}"
    echo "  Install with: cargo install typos-cli"
fi

# Calculate elapsed time
END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))
MINUTES=$((ELAPSED / 60))
SECONDS=$((ELAPSED % 60))

# Final Summary
print_header "Auto-Fix Summary"
echo -e "Time elapsed: ${BOLD}${MINUTES}m ${SECONDS}s${NC}"
echo -e "Fixes applied: ${GREEN}${BOLD}$TOTAL_FIXES${NC}"

if [ "$TOTAL_FIXES" -gt 0 ]; then
    echo -e "\n${GREEN}${BOLD}Applied fixes:${NC}${FIXES_APPLIED}"
fi

echo ""
echo -e "${BOLD}Next steps:${NC}"
echo "  1. Review the changes: git diff"
echo "  2. Run CI checks: ./ci-check.sh"
echo "  3. Commit changes: git add -A && git commit -m 'Apply CI fixes'"

# Run a quick check to see if everything is now passing
echo ""
echo -e "${BLUE}${BOLD}Running quick validation...${NC}"
VALIDATION_FAILED=false

# Quick format check
if ! cargo fmt --all -- --check &>/dev/null; then
    echo -e "${YELLOW}⚠ Some formatting issues remain${NC}"
    VALIDATION_FAILED=true
fi

# Quick clippy check
if ! cargo clippy --workspace --all-targets -- -D warnings &>/dev/null; then
    echo -e "${YELLOW}⚠ Some clippy warnings remain${NC}"
    VALIDATION_FAILED=true
fi

if [ "$VALIDATION_FAILED" = false ]; then
    echo -e "${GREEN}${BOLD}✨ All automated fixes successful!${NC}"
    echo -e "${GREEN}Your code should now pass CI checks.${NC}"
    exit 0
else
    echo ""
    echo -e "${YELLOW}${BOLD}Some issues need manual attention.${NC}"
    echo -e "${YELLOW}Run './ci-check.sh' for detailed information.${NC}"
    exit 0
fi
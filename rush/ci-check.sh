#!/usr/bin/env bash
# Rush CI Check Script
# Runs all code quality checks without modifying any files
# Exit on first failure for CI environments

set -e  # Exit on error
set -o pipefail  # Exit on pipe failure

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Track overall status
FAILED_CHECKS=""
TOTAL_CHECKS=0
PASSED_CHECKS=0

# Helper function for section headers
print_header() {
    echo ""
    echo -e "${BLUE}${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}${BOLD}  $1${NC}"
    echo -e "${BLUE}${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

# Helper function to run a check
run_check() {
    local name=$1
    local command=$2

    TOTAL_CHECKS=$((TOTAL_CHECKS + 1))
    echo -e "\n${BOLD}▶ Running: $name${NC}"

    if eval "$command"; then
        echo -e "${GREEN}✓ $name passed${NC}"
        PASSED_CHECKS=$((PASSED_CHECKS + 1))
    else
        echo -e "${RED}✗ $name failed${NC}"
        FAILED_CHECKS="${FAILED_CHECKS}\n  - $name"
        return 1
    fi
}

# Main script starts here
print_header "Rush CI Quality Checks"
echo "Starting comprehensive code quality checks..."
START_TIME=$(date +%s)

# 1. Check Rust toolchain
print_header "1. Rust Toolchain Check"
echo "Checking Rust installation..."
rustc --version
cargo --version
echo -e "${GREEN}✓ Rust toolchain verified${NC}"

# 2. Format Check
print_header "2. Code Formatting Check"
if ! run_check "Rust formatting" "cargo fmt --all -- --check"; then
    echo -e "${YELLOW}  → Run 'cargo fmt --all' or './ci-fix.sh' to fix formatting${NC}"
fi

# 3. Compilation Check
print_header "3. Compilation Check"
run_check "Debug build" "cargo build --workspace --all-targets"

# 4. Clippy Linting
print_header "4. Clippy Linting"
if ! run_check "Clippy warnings" "cargo clippy --workspace --all-targets --all-features -- -D warnings"; then
    echo -e "${YELLOW}  → Run 'cargo clippy --fix' or './ci-fix.sh' to fix some issues${NC}"
fi

# 5. Documentation Check
print_header "5. Documentation Check"
run_check "Documentation build" "cargo doc --workspace --no-deps --document-private-items"

# 6. Test Compilation
print_header "6. Test Compilation"
run_check "Test build" "cargo test --workspace --no-run"

# 7. Dependency Audit (if cargo-audit is installed)
print_header "7. Security Audit"
if command -v cargo-audit &> /dev/null; then
    run_check "Security vulnerabilities" "cargo audit"
else
    echo -e "${YELLOW}⚠ cargo-audit not installed, skipping security check${NC}"
    echo "  Install with: cargo install cargo-audit"
fi

# 8. Check for unused dependencies (if cargo-udeps is installed)
print_header "8. Dependency Check"
if command -v cargo-udeps &> /dev/null; then
    if rustup show | grep -q nightly; then
        run_check "Unused dependencies" "cargo +nightly udeps --all-targets"
    else
        echo -e "${YELLOW}⚠ Nightly toolchain required for cargo-udeps${NC}"
    fi
else
    echo -e "${YELLOW}⚠ cargo-udeps not installed, skipping dependency check${NC}"
    echo "  Install with: cargo install cargo-udeps"
fi

# 9. Check Cargo.toml sorting (if cargo-sort is installed)
print_header "9. Cargo.toml Organization"
if command -v cargo-sort &> /dev/null; then
    run_check "Cargo.toml sorting" "cargo sort --workspace --check"
else
    echo -e "${YELLOW}⚠ cargo-sort not installed, skipping Cargo.toml check${NC}"
    echo "  Install with: cargo install cargo-sort"
fi

# 10. Verify examples compile
print_header "10. Examples Check"
if [ -d "examples" ]; then
    run_check "Examples compilation" "cargo build --examples"
else
    echo "No examples directory found, skipping"
fi

# 11. Check for FIXME/TODO/HACK comments
print_header "11. Code Quality Markers"
echo "Checking for FIXME/TODO/HACK comments..."
MARKERS=$(grep -rn "FIXME\|TODO\|HACK" --include="*.rs" crates/ 2>/dev/null | wc -l || echo "0")
if [ "$MARKERS" -gt 0 ]; then
    echo -e "${YELLOW}⚠ Found $MARKERS FIXME/TODO/HACK comments${NC}"
    echo "Recent markers:"
    grep -rn "FIXME\|TODO\|HACK" --include="*.rs" crates/ 2>/dev/null | head -5 || true
else
    echo -e "${GREEN}✓ No FIXME/TODO/HACK markers found${NC}"
fi

# 12. Check for println! debugging statements
print_header "12. Debug Statement Check"
echo "Checking for println! debug statements..."
PRINTS=$(grep -rn "println!" --include="*.rs" crates/ 2>/dev/null | grep -v "^[[:space:]]*\/\/" | wc -l || echo "0")
if [ "$PRINTS" -gt 0 ]; then
    echo -e "${YELLOW}⚠ Found $PRINTS println! statements (may be debugging code)${NC}"
    echo "Recent occurrences:"
    grep -rn "println!" --include="*.rs" crates/ 2>/dev/null | grep -v "^[[:space:]]*\/\/" | head -3 || true
else
    echo -e "${GREEN}✓ No println! debug statements found${NC}"
fi

# 13. License headers check (optional)
print_header "13. License Headers"
echo "Checking for license headers..."
FILES_WITHOUT_LICENSE=$(find crates -name "*.rs" -type f ! -exec grep -l "Copyright\|License\|SPDX" {} \; 2>/dev/null | wc -l || echo "0")
if [ "$FILES_WITHOUT_LICENSE" -gt 0 ]; then
    echo -e "${YELLOW}ℹ $FILES_WITHOUT_LICENSE files without license headers${NC}"
else
    echo -e "${GREEN}✓ All files have license headers${NC}"
fi

# 14. Run unit tests (quick tests only)
print_header "14. Unit Tests"
if run_check "Unit tests" "cargo test --lib --workspace"; then
    echo -e "${GREEN}✓ All unit tests passed${NC}"
fi

# Calculate elapsed time
END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))
MINUTES=$((ELAPSED / 60))
SECONDS=$((ELAPSED % 60))

# Final Summary
print_header "CI Check Summary"
echo -e "Total checks run: ${BOLD}$TOTAL_CHECKS${NC}"
echo -e "Passed: ${GREEN}${BOLD}$PASSED_CHECKS${NC}"
echo -e "Failed: ${RED}${BOLD}$((TOTAL_CHECKS - PASSED_CHECKS))${NC}"
echo -e "Time elapsed: ${BOLD}${MINUTES}m ${SECONDS}s${NC}"

if [ -n "$FAILED_CHECKS" ]; then
    echo ""
    echo -e "${RED}${BOLD}Failed checks:${NC}${FAILED_CHECKS}"
    echo ""
    echo -e "${YELLOW}${BOLD}To fix issues, run: ./ci-fix.sh${NC}"
    exit 1
else
    echo ""
    echo -e "${GREEN}${BOLD}🎉 All CI checks passed!${NC}"
    echo -e "${GREEN}Code is ready for commit/push.${NC}"
    exit 0
fi
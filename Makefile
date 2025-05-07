# Rush CLI Makefile

# Variables
RUSH_DIR = rush
SIMPLE_TEST = --test simple_test

.PHONY: all clean build test test-simple run fix-warnings help

all: build

# Build the project 
build:
	cd $(RUSH_DIR) && cargo build

# Run simple tests that are known to work
test-simple:
	cd $(RUSH_DIR) && cargo test -p rush-cli $(SIMPLE_TEST)

# Run all tests
test:
	cd $(RUSH_DIR) && cargo test

# Clean build artifacts
clean:
	cd $(RUSH_DIR) && cargo clean

# Run the Rush CLI
run:
	cd $(RUSH_DIR) && cargo run -- --help

# Fix code warnings automatically
fix-warnings:
	cd $(RUSH_DIR) && cargo fix --lib -p rush-cli
	cd $(RUSH_DIR) && cargo fix --bin rush

# Rebuild tests
rebuild-tests:
	cd $(RUSH_DIR) && cargo clean
	cd $(RUSH_DIR) && cargo test -p rush-cli $(SIMPLE_TEST)

help:
	@echo "Rush CLI Makefile"
	@echo "Available targets:"
	@echo "  all           - Default target, builds the project"
	@echo "  build         - Build the Rush CLI project"
	@echo "  test-simple   - Run simple tests only"
	@echo "  test          - Run all tests"
	@echo "  clean         - Clean build artifacts"
	@echo "  run           - Run the Rush CLI with --help"
	@echo "  fix-warnings  - Fix code warnings automatically"
	@echo "  rebuild-tests - Clean and rebuild only the simple tests"
	@echo "  help          - Display this help message"
# Rush Development Makefile

.PHONY: run-ci
run-ci:
	@./scripts/run-ci.sh

.PHONY: ci
ci: run-ci

.PHONY: fmt
fmt:
	cargo fmt --all

.PHONY: fmt-check
fmt-check:
	cargo fmt --all -- --check

.PHONY: sort
sort:
	cargo sort --workspace

.PHONY: sort-check
sort-check:
	cargo sort --workspace --check

.PHONY: autoinherit
autoinherit:
	cargo autoinherit

.PHONY: autoinherit-check
autoinherit-check:
	cargo autoinherit --check

.PHONY: clippy
clippy:
	cargo clippy --workspace --all-targets -- -D warnings

.PHONY: test
test:
	cargo test --workspace

.PHONY: build
build:
	cargo build --release

.PHONY: help
help:
	@echo "Available targets:"
	@echo "  run-ci           - Run all CI checks (formatting, sorting, autoinherit, clippy)"
	@echo "  ci               - Alias for run-ci"
	@echo "  fmt              - Format code"
	@echo "  fmt-check        - Check code formatting"
	@echo "  sort             - Sort Cargo.toml files"
	@echo "  sort-check       - Check Cargo.toml sorting"
	@echo "  autoinherit      - Apply workspace inheritance"
	@echo "  autoinherit-check - Check workspace inheritance"
	@echo "  clippy           - Run clippy linter"
	@echo "  test             - Run all tests"
	@echo "  build            - Build release binary"
	@echo "  help             - Show this help message"
#!/bin/bash
set -e

echo "🔍 Running CI checks..."

echo "📝 Checking formatting..."
cargo fmt --all -- --check

echo "🔤 Checking Cargo.toml sorting..."
cargo sort --workspace --check

echo "🔗 Checking workspace inheritance..."
cargo autoinherit --check

echo "📋 Running clippy..."
cargo clippy --workspace --all-targets -- -D warnings

echo "✅ All CI checks passed!"
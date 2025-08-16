#!/bin/bash
set -e

echo "🔍 Running CI checks..."

echo "📝 Checking formatting..."
cargo fmt --all

echo "🔤 Checking Cargo.toml sorting..."
cargo sort --workspace

echo "🔗 Checking workspace inheritance..."
cargo autoinherit

echo "💾 Committing changes if any..."
if git diff --quiet; then
  echo "No changes to commit."
else
  git add .
  git commit -m "Auto-format and fix code style issues"
  echo "Changes committed."
fi

echo "📋 Running clippy..."
cargo clippy --workspace --all-targets --fix -- -D warnings

echo "✅ All CI checks passed!"

echo "💾 Committing clippy fixes if any..."
if git diff --quiet; then
  echo "No clippy changes to commit."
else
  git add .
  git commit -m "Auto-fix clippy issues"
  echo "Clippy changes committed."
fi

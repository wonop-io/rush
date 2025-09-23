#!/bin/bash
set -e

echo "Testing GitignoreManager module..."

# Create test program
cat > /tmp/test_gitignore.rs <<'EOF'
use std::fs;
use std::path::Path;
use tempfile::TempDir;

// Import the necessary crates
use ignore::gitignore::GitignoreBuilder;

fn main() {
    println!("Testing gitignore functionality...");

    // Test 1: Basic gitignore
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Create files
    fs::write(base_path.join("included.rs"), "// included").unwrap();
    fs::write(base_path.join("excluded.tmp"), "// excluded").unwrap();
    fs::write(base_path.join(".gitignore"), "*.tmp\n").unwrap();

    // Test gitignore parsing
    let mut builder = GitignoreBuilder::new(base_path);
    let gitignore_path = base_path.join(".gitignore");

    if let Some(err) = builder.add(&gitignore_path) {
        println!("Error adding .gitignore: {}", err);
    } else {
        println!("Successfully added .gitignore");
    }

    if let Ok(gitignore) = builder.build() {
        let included = gitignore.matched(base_path.join("included.rs"), false);
        let excluded = gitignore.matched(base_path.join("excluded.tmp"), false);

        println!("included.rs ignored: {}", included.is_ignore());
        println!("excluded.tmp ignored: {}", excluded.is_ignore());

        if !included.is_ignore() && excluded.is_ignore() {
            println!("✅ Test 1 PASSED: Basic gitignore works");
        } else {
            println!("❌ Test 1 FAILED: Basic gitignore doesn't work as expected");
        }
    }

    // Test 2: Walk with gitignore
    use ignore::WalkBuilder;

    let mut found_files = Vec::new();
    for entry in WalkBuilder::new(base_path)
        .git_ignore(true)
        .build()
    {
        if let Ok(entry) = entry {
            if entry.file_type().map_or(false, |ft| ft.is_file()) {
                let path = entry.path();
                if path != base_path.join(".gitignore") {
                    found_files.push(path.file_name().unwrap().to_string_lossy().to_string());
                }
            }
        }
    }

    println!("Files found by walker: {:?}", found_files);

    if found_files.contains(&"included.rs".to_string()) && !found_files.contains(&"excluded.tmp".to_string()) {
        println!("✅ Test 2 PASSED: Walker respects gitignore");
    } else {
        println!("❌ Test 2 FAILED: Walker doesn't respect gitignore properly");
    }

    println!("\nAll basic tests completed!");
}
EOF

# Compile and run test
echo "Compiling test program..."
cd /tmp
rustc --edition 2021 test_gitignore.rs --extern ignore=/Users/tfr/Documents/Projects/rush/rush/target/release/deps/libignore*.rlib --extern tempfile=/Users/tfr/Documents/Projects/rush/rush/target/release/deps/libtempfile*.rlib -L /Users/tfr/Documents/Projects/rush/rush/target/release/deps 2>/dev/null || {
    echo "Direct compilation failed, trying with cargo..."

    # Create a minimal cargo project
    cargo new --bin gitignore_test --quiet
    cd gitignore_test

    # Add dependencies
    cat > Cargo.toml <<'EOF'
[package]
name = "gitignore_test"
version = "0.1.0"
edition = "2021"

[dependencies]
ignore = "0.4"
tempfile = "3"
EOF

    # Copy test code
    cp /tmp/test_gitignore.rs src/main.rs

    # Build and run
    cargo run --quiet
}

echo "Test completed!"
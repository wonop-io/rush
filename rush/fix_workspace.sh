#!/bin/bash
set -e

echo "Fixing workspace compilation issues..."

# Fix rush-utils path.rs imports
sed -i.bak 's/use crate::constants/use rush_core::constants/g' crates/rush-utils/src/path.rs

# Remove the old mod.rs if it exists
rm -f crates/rush-utils/src/mod.rs

# Fix missing async in rush-utils
sed -i.bak 's/pub fn run_command/pub async fn run_command/g' crates/rush-utils/src/command_runner.rs 2>/dev/null || true

# Move tests directory to workspace root if not there
if [ ! -d "tests" ] && [ -d "../tests" ]; then
    mv ../tests ./
fi

# Fix imports in all files
find crates -name "*.rs" -type f -exec sed -i.bak \
    -e 's/use crate::error::/use rush_core::error::/g' \
    -e 's/use crate::Error/use rush_core::Error/g' \
    -e 's/use crate::Result/use rush_core::Result/g' \
    -e 's/use crate::constants/use rush_core::constants/g' \
    -e 's/use crate::shutdown/use rush_core::shutdown/g' \
    -e 's/crate::error::Error/rush_core::error::Error/g' \
    -e 's/crate::error::Result/rush_core::error::Result/g' \
    {} \;

# Clean up backup files
find crates -name "*.bak" -delete

echo "Workspace fixes applied!"
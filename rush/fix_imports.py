#!/usr/bin/env python3
"""Fix imports after workspace migration"""

import os
import re
from pathlib import Path

# Mapping of old imports to new crate imports
IMPORT_MAPPINGS = {
    r'crate::error': 'rush_core::error',
    r'crate::constants': 'rush_core::constants',
    r'crate::shutdown': 'rush_core::shutdown',
    r'crate::utils::': 'rush_utils::',
    r'crate::toolchain': 'rush_toolchain',
    r'crate::core::': 'rush_config::',
    r'crate::security': 'rush_security',
    r'crate::build': 'rush_build',
    r'crate::output': 'rush_output',
    r'crate::container': 'rush_container',
    r'crate::k8s': 'rush_k8s',
    r'crate::cli': '', # This stays internal to rush-cli
}

def fix_imports_in_file(filepath):
    """Fix imports in a single Rust file"""
    if not filepath.endswith('.rs'):
        return
    
    try:
        with open(filepath, 'r') as f:
            content = f.read()
        
        original = content
        
        # Determine which crate this file belongs to
        crate_name = None
        for crate in ['rush-core', 'rush-utils', 'rush-config', 'rush-toolchain', 
                      'rush-security', 'rush-build', 'rush-output', 'rush-container', 
                      'rush-k8s', 'rush-cli']:
            if f'/crates/{crate}/' in filepath:
                crate_name = crate
                break
        
        if not crate_name:
            return
        
        # Apply import mappings based on which crate we're in
        for old_import, new_import in IMPORT_MAPPINGS.items():
            # Skip self-references
            if new_import and new_import.replace('::', '').replace('_', '-') == crate_name:
                continue
            
            if new_import:
                content = re.sub(f'use {old_import}', f'use {new_import}', content)
            else:
                # For CLI internal imports, keep them as crate::
                if crate_name == 'rush-cli':
                    continue
                else:
                    content = re.sub(f'use {old_import}', f'use crate::', content)
        
        # Fix fully qualified paths
        for old_import, new_import in IMPORT_MAPPINGS.items():
            if new_import and new_import.replace('::', '').replace('_', '-') != crate_name:
                # Remove 'use' from the pattern for inline usage
                old_pattern = old_import.replace(r'crate::', '')
                new_pattern = new_import.replace('::', '::')
                content = re.sub(f'crate::{old_pattern}', new_pattern, content)
        
        if content != original:
            with open(filepath, 'w') as f:
                f.write(content)
            print(f"Fixed imports in {filepath}")
    
    except Exception as e:
        print(f"Error processing {filepath}: {e}")

def main():
    """Fix all imports in the workspace"""
    crates_dir = Path('crates')
    
    for crate_dir in crates_dir.iterdir():
        if crate_dir.is_dir():
            src_dir = crate_dir / 'src'
            if src_dir.exists():
                for rust_file in src_dir.rglob('*.rs'):
                    fix_imports_in_file(str(rust_file))

if __name__ == '__main__':
    main()
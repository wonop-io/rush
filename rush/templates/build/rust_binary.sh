set -e
cd {{ location }} || exit
{% for d,v in domains -%}
export DOMAIN_{{ d | envname }}="{{ v }}"
{% endfor %}
{% for d,v in env -%}
export {{ d | envname }}="{{ v }}"
{% endfor %}

{% if precompile_commands %}
{% for p in precompile_commands -%}
{{ p }}
{% endfor %}
{% endif %}

export SQLX_OFFLINE=true

# Setup Rust-specific cross-compilation environment variables if CC is set
if [ -n "$CC" ]; then
    # Set Rust target-specific CC variable
    export CC_x86_64_unknown_linux_gnu="$CC"
    export TARGET_CC="$CC"
    # Set cargo target directory
    export CARGO_TARGET_x86_64_unknown_linux_gnu_LINKER="$CC"
fi

# Check if we're on Apple Silicon and need to set platform
if [ "$(uname)" = "Darwin" ] && [ "$(uname -m)" = "arm64" ]; then
    export DOCKER_DEFAULT_PLATFORM=linux/amd64
fi

# Check if we have 'cross' for cross-compilation
if command -v cross &> /dev/null && [ "{{ rust_target }}" = "x86_64-unknown-linux-gnu" ] && [ "$(uname)" = "Darwin" ]; then
    echo "Using 'cross' for cross-compilation to Linux"
    CARGO_TARGET_DIR=./target cross build --release --target {{ rust_target }}
elif [ "{{ rust_target }}" = "x86_64-unknown-linux-gnu" ] && [ "$(uname)" = "Darwin" ]; then
    echo "Warning: Cross-compilation to Linux on macOS requires either:"
    echo "  1. The 'cross' tool (cargo install cross)"
    echo "  2. A cross-compilation toolchain (e.g., x86_64-unknown-linux-gnu)"
    echo ""
    echo "Attempting build with cargo and linker configuration..."
    # Try with linker configuration if CC is set
    if [ -n "$CC" ]; then
        CARGO_TARGET_DIR=./target cargo build --release --target {{ rust_target }} --config "target.{{ rust_target }}.linker = '$CC'"
    else
        CARGO_TARGET_DIR=./target cargo build --release --target {{ rust_target }}
    fi
else
    # Native compilation or supported target
    CARGO_TARGET_DIR=./target cargo build --release --target {{ rust_target }}
fi

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

{% if cross_compile == "cross-rs" %}
# Use cross-rs for cross-compilation
echo "Using cross-rs for cross-compilation to {{ rust_target }}"
if ! command -v cross &> /dev/null; then
    echo "Error: cross-rs is not installed. Install it with: cargo install cross --git https://github.com/cross-rs/cross"
    exit 1
fi
CARGO_TARGET_DIR=./target cross build --release --target {{ rust_target }}{% if features %} --features {{ features | join(sep=" ") }}{% endif %}
{% else %}
# Use native cross-compilation
echo "Using native toolchain for cross-compilation to {{ rust_target }}"
{% if rust_target == "x86_64-unknown-linux-gnu" %}
# Cross-compilation with proper linker configuration
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="{{ toolchain.cc }}"
CARGO_TARGET_DIR=./target cargo build --release --target {{ rust_target }}{% if features %} --features {{ features | join(sep=" ") }}{% endif %}
{% else %}
# Other targets
CARGO_TARGET_DIR=./target cargo build --release --target {{ rust_target }}{% if features %} --features {{ features | join(sep=" ") }}{% endif %}
{% endif %}
{% endif %}

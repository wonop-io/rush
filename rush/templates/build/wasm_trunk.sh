set -e
cd {{ location }} || exit
{% for d,v in domains -%}
export DOMAIN_{{ d | envname }}="{{ v }}"
{% endfor %}
{% for d,v in env -%}
export {{ d | envname }}="{{ v }}"
{% endfor %}

{% if precompile_commands -%}
{% for p in precompile_commands -%}
{{ p }}
{% endfor %}
{% endif %}

{% if ssr %}
CARGO_TARGET_DIR=./target wasm-trunk build  --features hydration --release
{% else %}
CARGO_TARGET_DIR=./target wasm-trunk build  --release
{% endif %}

{% if ssr %}
export SQLX_OFFLINE=true
{% if cross_compile == "cross-rs" %}
# Use cross-rs for SSR binary cross-compilation
echo "Using cross-rs for SSR binary cross-compilation to {{ rust_target }}"
if ! command -v cross &> /dev/null; then
    echo "Error: cross-rs is not installed. Install it with: cargo install cross --git https://github.com/cross-rs/cross"
    exit 1
fi
CARGO_TARGET_DIR=./target cross build --features ssr --release --target {{ rust_target }}
{% else %}
# Use native cross-compilation for SSR binary
CARGO_TARGET_DIR=./target cargo build --features ssr --release --target {{ rust_target }} --config "target.{{ rust_target }}.linker = '{{toolchain.cc}}'"
{% endif %}
{% endif %}
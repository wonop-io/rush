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
CARGO_TARGET_DIR=./target wasm-trunk build --features csr --release

{% if ssr %}
export SQLX_OFFLINE=true
CARGO_TARGET_DIR=./target cargo build --features ssr,hydration --release --target {{ rust_target }} --config "target.{{ rust_target }}.linker = '{{toolchain.cc}}'"
{% endif %}

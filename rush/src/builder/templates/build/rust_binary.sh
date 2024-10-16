cd {{ location }} || exit
{% for d,v in domains -%}
export DOMAIN_{{ d | uppercase }}="{{ v }}"
{% endfor %}
{% for d,v in env -%}
export {{ d | uppercase }}="{{ v }}"
{% endfor %}

export SQLX_OFFLINE=true
CARGO_TARGET_DIR=./target cargo build --release --target {{ rust_target }} --config "target.{{ rust_target }}.linker = '{{toolchain.cc}}'"

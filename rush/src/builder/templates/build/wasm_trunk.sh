cd {{ location }} || exit
{% for d,v in domains -%}
export DOMAIN_{{ d | uppercase }}="{{ v }}"
{% endfor %}
{% for d,v in env -%}
export {{ d | uppercase }}="{{ v }}"
{% endfor %}

CARGO_TARGET_DIR=./target wasm-trunk build --release

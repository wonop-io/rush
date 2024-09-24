cd {{ location }} || exit
export SQLX_OFFLINE=true
CARGO_TARGET_DIR=./target cargo build --release --target {{ rust_target }} --config "target.{{ rust_target }}.linker = '{{toolchain.cc}}'"

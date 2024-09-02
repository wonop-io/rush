cd {{ location }} || exit
export SQLX_OFFLINE=true
cargo build --target {{ rust_target }} --release --config "target.{{ rust_target }}.linker = '{{toolchain.cc}}'"

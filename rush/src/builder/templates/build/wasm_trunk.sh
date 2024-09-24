cd {{ location }} || exit
CARGO_TARGET_DIR=./target wasm-trunk build --release 

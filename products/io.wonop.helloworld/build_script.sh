set -e
cd frontend/webui || exit

export API_URL="http://localhost:8000/api"
export NODE_ENV="local"





CARGO_TARGET_DIR=./target wasm-trunk build  --release



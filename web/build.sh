#!/usr/bin/env bash
# Build the REVENANT wasm core and stage it for the web front-end.
set -e
cd "$(dirname "$0")/.."
cargo build -p revenant-wasm --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/revenant_wasm.wasm web/revenant.wasm
echo "staged web/revenant.wasm ($(wc -c < web/revenant.wasm) bytes)"
echo "serve: cd web && python -m http.server 8080"

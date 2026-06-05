#!/usr/bin/env bash
# Build the REVENANT wasm core + all bundled game ROMs into web/.
set -e
cd "$(dirname "$0")/.."
echo "» building wasm core…"
cargo build -p revenant-wasm --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/revenant_wasm.wasm web/revenant.wasm
echo "» building game ROMs…"
for g in game snake breakout dodge; do
  if [ -f "core/examples/make$g.rs" ]; then
    cargo run -q --release --example "make$g" >/dev/null && echo "   ✓ web/$g.gb"
  fi
done
echo "done.  serve:  cd web && python -m http.server 8080"

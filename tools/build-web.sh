#!/bin/bash
# Build the web version into web/pkg (wasm + JS glue). Serve web/ with any static server.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
cd "$root"
cargo build --release -p cadence-game --lib --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir web/pkg \
    target/wasm32-unknown-unknown/release/cadence_game.wasm
ls -la web/pkg/*.wasm

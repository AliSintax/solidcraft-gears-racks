#!/usr/bin/env bash
# Build the plugin into dist/solidcraft-gears/ (plugin.wasm + plugin.json).
# Needs: rust with the wasm32-unknown-unknown target, and wasm-tools (https://github.com/bytecodealliance/wasm-tools).
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --lib --target wasm32-unknown-unknown
out=dist/solidcraft-gears
rm -rf "$out"; mkdir -p "$out"
wasm-tools component new "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/solidcraft_gears.wasm" -o "$out/plugin.wasm"
cp plugin.json "$out/"
cp README.md LICENSE "$out/"
echo "built $out"

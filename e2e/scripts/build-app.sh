#!/usr/bin/env bash
# Builds the app for the real-app E2E: debug profile, but with the frontend embedded
# (tauri/custom-protocol) instead of loading the Vite devUrl. Uses its own target dir so
# regular `cargo build`/`tauri dev` runs never overwrite it.
# Output: src-tauri/target/e2e/debug/yts-player and .../examples/e2e_seeder
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
npm run build
cd src-tauri
CARGO_TERM_COLOR=never cargo build --features tauri/custom-protocol --target-dir target/e2e --bin yts-player --example e2e_seeder
echo "$root/src-tauri/target/e2e/debug/yts-player"

#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Building WASM..."
wasm-pack build "$ROOT/crates/wasm" --target web --dev

echo "Starting Vite dev server..."
cd "$ROOT/web"
pnpm install
pnpm dev
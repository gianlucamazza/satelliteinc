#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

cd "$ROOT"
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

wasm-pack build crates/wasm --target web

cd web
pnpm install --frozen-lockfile
pnpm run typecheck
pnpm run lint
pnpm run build

echo "CI checks passed."

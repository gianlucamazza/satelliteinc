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
pnpm run test
pnpm run build
# E2E: install Chromium if missing (cached after first local run).
if [[ "${SKIP_E2E:-}" != "1" ]]; then
  pnpm run test:e2e:install
  pnpm run test:e2e
fi

echo "CI checks passed."

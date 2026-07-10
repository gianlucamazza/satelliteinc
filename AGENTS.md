# Satellite Inc — Agent Rules

## Architecture

Three layers — never blur them:

1. **`crates/core`** — deterministic simulation (orbital physics, ops, economy). Zero WASM/DOM deps.
2. **`crates/wasm`** — thin wasm-bindgen bridge only.
3. **`web/`** — presentation (Three.js globe, UI). **No physics here.**

## Hard rules

- Sim math uses `f64` in Rust. Renderer uses `f32` in TypeScript.
- Parent-relative coordinates at the render boundary (km offsets from Earth center).
- Content lives in `content/*.json` — not hardcoded in logic.
- After any `crates/core` change: `cargo test -p satelliteinc-core`.
- After WASM API change: rebuild with `wasm-pack build crates/wasm --target web`.

## Docs

- `docs/gdd.md` — design canon
- `docs/architecture.md` — layer diagram, WASM contract
- `docs/vertical-slice.md` — M1 checklist

## Dev

```bash
bash scripts/dev.sh    # wasm build + vite dev
bash scripts/ci.sh     # full CI locally
```
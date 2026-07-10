# Satellite Inc — Game Design Document

## Vision

Build and operate a space company from your browser. Design satellites, launch them to orbit, schedule ground-station passes, and fulfill contracts — backed by real Keplerian orbital mechanics in a Rust simulation core.

## Pillars

1. **Physics is truth.** Orbital propagation runs in Rust (`f64`). The renderer never computes orbits.
2. **Ops matter.** Pass windows, power budgets, and downlink queues create meaningful waiting and planning.
3. **Business pressure.** Contracts have deadlines and payouts; launch costs bite early cash.
4. **Phased depth.** Ship a tight vertical slice first; add maneuvers, multi-station ops, and tycoon systems later.

## Player promise (M6)

Choose a **campaign** or **sandbox** scenario, design a satellite, accept contracts (some require multiple ground stations), launch to LEO, operate through passes and anomalies, invest in R&D, and grow reputation — with **save/load** between sessions.

### Campaign vs Sandbox

| | Campaign | Sandbox |
|---|----------|---------|
| Buses | Default + R&D unlocks | All unlocked |
| Contracts | Reputation gates, no repeats | All available |
| Failure penalty | −10 rep | −5 rep |
| Starting cash | Scenario-defined (~450k) | Higher (~750k) |

## Core loop

1. Load scenario or configure bus + orbit
2. Accept contract from marketplace
3. Launch to Keplerian orbit
4. Collect imagery in sunlight; downlink during station passes
5. Complete contract → cash + reputation → R&D → harder contracts

## Target platform

Browser — Rust/WASM sim core + TypeScript/Three.js WebGL frontend.

## Future (M7+)

Fleet operations, competitor market, expanded TLE catalog, WebGPU globe, e2e test suite.
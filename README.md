# Satellite Inc

Browser-based satellite company simulator. Rust simulation core (WASM) + Three.js globe.

**Playable through M8:** scenarios, campaign/sandbox modes with win/lose states, save/load (v3 with migration), multi-station and orbit-gated contracts, recurring economy with bankruptcy, pass planner, full ops + tycoon loop — and a data-driven solar system under the hood.

| Milestone | Feature                                                                                                          |
| --------- | ---------------------------------------------------------------------------------------------------------------- |
| M1        | Design → launch → downlink → contract payout                                                                     |
| M2        | Hohmann transfers, impulsive burns, Δv budget                                                                    |
| M3        | 4 ground stations, command queue, anomalies, payload modes                                                       |
| M4        | Reputation, marketplace, R&D tree                                                                                |
| M5        | SGP4 TLE catalog (ISS reference orbit)                                                                           |
| M6        | Campaign rules, save/load, scenarios, pass ETA, multi-station contracts                                          |
| M7        | Economy pressure (ops costs, bankruptcy), orbit requirements, decommission, victory/defeat                       |
| M8        | Celestial bodies as data: `BodyRegistry` (μ/radius/rotation from content), J2000 ephemerides groundwork, save v3 |
| M9+       | Moon/Mars operations, system map, launch windows, interplanetary transfers ([plan](docs/gdd.md))                 |

## Quick start

```bash
# Prerequisites: Rust, wasm-pack, pnpm
bash scripts/dev.sh
```

Open the URL Vite prints (default `http://localhost:5173`).

## Project layout

```
crates/core/   — deterministic sim (orbital physics, ops, economy)
crates/wasm/   — wasm-bindgen bridge
web/           — TypeScript + Three.js frontend
content/       — game data (buses, stations, contracts)
docs/          — GDD, architecture, vertical slice
```

## CI

```bash
bash scripts/ci.sh
```

## Docs

- [Game Design](docs/gdd.md)
- [Architecture](docs/architecture.md)
- [Vertical Slice](docs/vertical-slice.md)
- [Agent Rules](AGENTS.md)

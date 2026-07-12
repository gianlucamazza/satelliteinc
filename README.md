# Satellite Inc

Browser-based satellite company simulator. Rust simulation core (WASM) + Three.js globe.

**Playable through M13 — the full solar system campaign:** run an Earth imaging startup into an interplanetary agency. Programs gate the Moon, Mars, Venus and Jupiter; launch windows come from real mean-element ephemerides; transfers cruise on Hohmann rails with patched-conic burns; the Europa Ocean Survey is the grand victory.

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
| M9 | Moon/Mars operations: per-body stations, DSN links with farside blackout, anomaly flavors |
| M10 | Live ephemerides, per-body seasonal sun, System Map (M) with compressed radii and true longitudes |
| M11 | Interplanetary transfers: launch windows on mean longitudes, cash injections, hibernated cruise, deterministic warp |
| M12 | Campaign Programs (buy-ins, keystones, grants), mobilization advances with clawback, on-station contracts, Mars+Venus campaigns |
| M13 | Jupiter finale: Pioneer RTG bus, Juno-style eccentric capture, radiation upsets, grand victory |

Textures: Earth/Moon from the three.js examples (NASA imagery); Mars from [Solar System Scope](https://www.solarsystemscope.com/textures/) (CC BY 4.0).

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

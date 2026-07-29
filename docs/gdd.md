# Satellite Inc — Game Design Document

## Vision

Build and operate a space company from your browser. Design satellites, launch them to orbit, schedule ground-station passes, and fulfill contracts — backed by real Keplerian orbital mechanics in a Rust simulation core.

## Pillars

1. **Physics is truth.** Orbital propagation runs in Rust (`f64`). The renderer never computes orbits.
2. **Ops matter.** Pass windows, power budgets, and downlink queues create meaningful waiting and planning. **Ops have infrastructure** — ground networks are body-local or DSN, and tech can unlock local relays.
3. **Business pressure.** Contracts have deadlines and payouts; launch costs bite early cash. **Market has adversaries** — rivals contest open offers and claim them if you wait too long.
4. **Phased depth.** Ship complete systems (spec + tests + legible UI), not half-features.

## Player promise (M16)

Run **Satellite Inc** from an Earth imaging startup to an interplanetary agency:

- **One Career** (default): design buses, set orbits, accept contracts, unlock **programs** Earth → Moon → Mars → Venus → Jupiter.
- **Sandbox Lab** is free-play tooling, not a second campaign.
- Operate a **fleet** (mission-control slots): launch, downlink, anomalies, Hohmann maneuvers, decommission.
- Research **Deep Space Operations**, plan **transfer windows**, cruise with warp, capture at destination bodies.
- Complete all **programs** for grand victory (Europa Ocean Survey); **save/load** (JSON export/import).

### Career vs Sandbox

| | Career | Sandbox Lab |
|---|----------|---------|
| Progress | Single program spine Earth→Jupiter | Optional progress board only |
| Buses | Default + R&D unlocks | All unlocked |
| Contracts | Reputation + program gates | All available |
| Rivals / weather | On; pressure scales with programs done | Off |
| Failure penalty | −10 rep | −5 rep |
| Starting cash | ~450k | ~2M |
| Victory / bankruptcy | Yes | No freeze |

## Core loop

1. Start Career (or Sandbox Lab)
2. Accept contract from marketplace
3. Launch to Keplerian orbit
4. Collect imagery in sunlight; downlink during station passes
5. Complete contracts → cash + rep → finish body program → unlock next world

## Target platform

Browser — Rust/WASM sim core + TypeScript/Three.js WebGL frontend.

## Presentation (browser)

- **Body view:** 3D globe of the craft’s current body (stations, orbit trail, satellite mesh).
- **System map (M):** compressed schematic of campaign bodies (Sun through Jupiter); not true-scale, not the full solar system.
- **HUD:** glassmorphism panel — Mission / Ops / Orbit / Company / Transfer tabs; collapsible on tablet.

## Realism (M17)

Player-facing rules that the sim enforces:

| Rule | Effect |
|------|--------|
| **J2 precession** | RAAN and argument of perigee drift; ~98° / 550 km is sun-synchronous (~+1° RAAN/day). |
| **Solar flux 1/r²** | Panel power scales with body distance from the Sun (Mars ~40%, Jupiter ~4% of Earth). RTG buses ignore sunlight. |
| **Plane change at nodes** | Δi burns only near ascending/descending node; Δv = 2·v·sin(\|Δi\|/2). |
| **Light-time commands** | Uplink beyond Earth cannot execute before one-way light time elapses. |
| **Per-bus payload** | Buffer size and imaging rate come from the bus template, not globals. |

## Ground network (M18+)

- Stations have a `body_id`. Craft use **local** dishes on the same body, or Earth **DSN** dishes when away from Earth (bandwidth × body's `dsn_bandwidth_factor`, farside blackout).
- **Earth LEO commercial net** spans polar + equatorial sites (Svalbard, Kiruna, Fairbanks, Santiago, Singapore, Perth, Malindi) so multi-station contracts are feasible.
- **Body relays** (tech-gated in campaign): Moon nearside + farside, Mars dual nodes, Venus aerostat, Jovian system relay. Sandbox unlocks all.
- Launch / capture rebinds downlink to the **preferred reachable** dish (best local bandwidth, else DSN) — never a content-order default.
- Ops UI: body/DSN tags, PASS/LINK, effective Mbps, locked-tech hints; status uses the *selected* dish for signal diagnosis.

## Competitor market (M19)

Campaign only (sandbox is quiet by default; scenario can override with `competitors_enabled`).

| Rule | Player-facing effect |
|------|----------------------|
| **Contest** | A rival marks an open offer for **2 sim-days**. HUD shows who and the countdown. |
| **Accept** | Free during contest — you take the work; contest ends. |
| **Rush bid** | Pay **15% of payout** cash premium to snatch a contested offer immediately (same accept + fee). |
| **Claim** | If the window expires, the rival holds the offer for **10 sim-days**, then it reappears. |
| **Scoreboard** | Rivals track contracts won; Mission tab shows market pressure strip. |

Rivals are content-driven (`competitors[]`): id, name, aggressiveness, focus bodies. Actions are **deterministic** (no RNG) — same content + time → same contests.

## World events (M20)

Campaign only (sandbox quiet by default; scenario can override with `world_events_enabled`).

| Rule | Effect |
|------|--------|
| **Schedule** | Every ~3.5 sim-days (when idle), a weighted event starts (deterministic day-bucket pick). |
| **Duration** | Content `duration_days`; cooldown 1 day after end. |
| **Solar** | `solar_penalty` multiplies panel output (RTG unaffected). |
| **Anomalies** | `anomaly_rate_multiplier` raises/lowers failure rate (see FDIR below). |
| **Bandwidth** | `bandwidth_multiplier` scales links; `dsn_only` hits deep-space only. |
| **Bodies** | Empty `body_ids` = global; else scoped (e.g. Mars dust). |

HUD: priority strip weather chip + banner; toasts on start/end.

## Anomalies & FDIR (ops realism)

Not arcade random damage: deterministic schedule, procedure-matched recovery.

| Archetype | Effect | Recovery |
|-----------|--------|----------|
| **Power / EPS glitch** | −50% solar, +load stress; auto-safe if battery &lt;12% | **Safe mode** (halves remaining) → **EPS clear** at pass |
| **Comms dropout** | Downlink rate 0; long on Mars dust flavor | **Reboot comms** at pass (+ OWLT deep space) |
| **Buffer SEU** | Instant data loss (fraction), **not sticky** | None — one-shot event; cooldown only |

Environment: body rate + flavor labels, weather MTBF, eclipse slightly raises pressure, tech `anomaly_mtbf_multiplier` stretches period. Wrong ground procedure does not clear the fault.

## Agency prestige (M22)

Always-on scoreboard (Company tab + victory endscreen):

| Input | Points |
|-------|--------|
| Contract completion | ×15 |
| Program completion | ×120 |
| Contested offer won | ×25 |
| Reputation | ×2 |
| Cash / fleet revenue | soft bonuses |
| Grand victory | +250 |
| Rival claims | −8 each |

Ranks: Startup → Regional Operator → National Agency → Interplanetary Agency → Grand Agency. Goal checklist doubles as sandbox progress board.

## Systems roadmap (post-M16)

| Ms | Focus | Status |
|----|--------|--------|
| M17 | Realism seal (above) | ✓ |
| M18 | Ground network & station tech gates | ✓ foundation |
| M19 | Competitor market | ✓ |
| M20 | World events (space weather) | ✓ |
| M21 | Content density that exercises systems | ✓ (40 contracts, Career+Sandbox entries, Mars relay, TLE) |
| M22 | Agency prestige / post-victory | ✓ |
| M23 | Production seal (e2e, UI modularization) | ✓ partial (Vitest + Playwright smoke + tabs) |
| M24 | Unified Career (one game logic; progressive pressure) | ✓ |

Presentation-only work (audio, WebGPU, smartphone-first) stays secondary to systems leadership.
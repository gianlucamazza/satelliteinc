# Satellite Inc — Vertical Slice Status

## M1 — Complete

Playable loop: design → launch → pass → downlink → contract payout.

## M2 — Orbital Maneuvers — Complete

PV ↔ Kepler, impulsive burns, Hohmann 2-burn, Δv budget, maneuver HUD.

## M3 — Ground Ops — Complete

4 stations, payload modes, command queue, anomalies, station-specific downlink.

## M4 — Tycoon — Complete

Reputation, marketplace, R&D tree, campaign/sandbox selector.

## M5 — SGP4 / TLE — Complete

ISS reference orbit on globe (purple marker).

## M6 — Campaign, Save & Content — Complete

| Feature | Status | Location |
|---------|--------|----------|
| Campaign rules (rep gates, bus unlock, failure penalty) | ✓ | `crates/core/src/company/mod.rs` |
| Sandbox rules (all buses/contracts, relaxed) | ✓ | `company/rd.rs` |
| Multi-station contracts | ✓ | `content/game.json`, `ActiveContract::stations_used` |
| Pass planner (`next_pass_in_seconds`) | ✓ | `ops/visibility.rs`, snapshot |
| Save / load | ✓ | `crates/core/src/save.rs`, WASM `export_save` / `import_save` |
| Scenarios | ✓ | `content/game.json`, `scenario.rs` |
| 5 contracts, 3 scenarios | ✓ | `content/game.json` |
| UI: tabs, save, scenarios, pass ETA | ✓ | `web/src/ui/panel.ts` |

### M6 tests

| Test | Status |
|------|--------|
| Campaign vs sandbox marketplace | ✓ |
| Multi-station contract completion | ✓ |
| Save round-trip | ✓ |
| Golden action replay | ✓ |
| Scenario load | ✓ |

## Roadmap (post-M6)

- Fleet / multi-satellite (M7)
- Competitor AI & dynamic market (M8)
- WebGPU renderer (M9)
- Playwright e2e, WASM size budget (M10)
# Satellite Inc — Milestone Status

**Current:** M20 world events + M19 market + M17/M18 foundation + UI platform U1/U2. See [README](../README.md) for quick start.

## Sim milestones (M1–M16) — Complete

| Ms | Feature | Key location |
|----|---------|--------------|
| M1 | Design → launch → pass → downlink → payout | `crates/core`, `web/src/ui/panel.ts` |
| M2 | Hohmann, impulsive burns, Δv budget | `orbit/maneuver.rs`, Orbit tab |
| M3 | Stations, commands, anomalies, payload modes | `ops/`, Ops tab |
| M4 | Reputation, marketplace, R&D | `company/` |
| M5 | SGP4 TLE (ISS reference) | `orbit/sgp4_cat.rs`, `body-view.ts` |
| M6 | Campaign/sandbox, save/load, scenarios | `save.rs`, `scenario.rs` |
| M7 | Economy pressure, orbit requirements, decommission | `company/mod.rs`, tests `m7_*` |
| M8 | `BodyRegistry`, ephemerides groundwork, save v3 | `body.rs`, `content/game.json` |
| M9 | Moon/Mars ops, DSN, per-body anomalies | tests `m9_bodies.rs` |
| M10 | Live ephemerides, System Map (M) | `system-map.ts`, `view-manager.ts` |
| M11 | Interplanetary transfers, windows, warp | `orbit/transfer.rs`, Transfer tab |
| M12 | Programs, mobilization, on-station contracts | `content/game.json`, tests `m12_*` |
| M13 | Jupiter finale, RTG bus, grand victory | tests `m13_finale.rs` |
| M14 | Fleet slots, `assign_contract`, fleet strip | `snapshot.rs`, `m14_fleet.rs` |
| M15 | Guided missions, contextual onboarding, warp-to-event | `guided.ts`, `onboarding.ts` |
| M16 | Craft rename, notification tiers, auto-pause | `main.ts`, `panel.ts` |

## Systems leadership track (M17+)

| Ms | Feature | Status | Key location |
|----|---------|--------|--------------|
| M17 | J2, solar flux 1/r², node plane-change, light-time cmds, per-bus payload | ✓ seal | `m17_realism.rs`, Orbit/Ops HUD |
| M18 | Station `body_id` + `requires_tech`, Lunar Nearside Relay | ✓ foundation | `m18_ground.rs`, Ops tab |
| M18+ | Dense LEO net, Moon farside, Mars×2, Venus/Jupiter relays; preferred dish on launch | ✓ | `content/game.json`, `preferred_downlink_station` |
| FDIR | Procedure-matched anomalies (EPS/reboot/safe); SEU one-shot; body weights | ✓ | `ops/anomaly.rs`, `m7_ops_teeth` |
| M19 | Competitor market: contest / claim / rush bid, save v5+ | ✓ | `market.rs`, `m19_market.rs`, Mission tab |
| M20 | World events / space weather, save v6 | ✓ | `weather.rs`, `m20_weather.rs`, priority strip |
| M21 | Content pack: 40 contracts, Career+Sandbox, Mars relay | ✓ | `content/game.json` |
| M24 | Unified Career (programs spine, progressive pressure) | ✓ | `scenario.rs`, `prestige.rs`, market/weather scale |
| M22 | Agency prestige score/rank/goals | ✓ | `prestige.rs`, `m22_prestige.rs`, Company tab |
| M23 | Production: Vitest + Playwright smoke + tab modules | ✓ partial | `web/e2e/`, `pnpm test:e2e` |

### Content scope (`content/game.json`)

- **Bodies:** Sun, Earth, Moon, Venus, Mars, Jupiter (campaign arc Earth → Jupiter).
- **Programs:** five interplanetary programs; Europa Ocean Survey = grand victory.
- **Fleet:** `max_flights` / `active_flights` in snapshot; multi-contract marketplace.

### Core tests (representative)

`cargo test -p satelliteinc-core` — includes `m8_baseline`, `m14_fleet`, `m17_realism`, etc.

## Web / UX milestones — Complete (Phases 1–3)

Phases from the graphics/UX audit (2026-07). **Not** separate sim milestones.

### Phase 1 — HUD ↔ sim alignment

| Item | Status |
|------|--------|
| `max_flights` / `active_flights` in snapshot | ✓ |
| Multi-contract marketplace + craft picker | ✓ |
| `assign_contract` UI | ✓ |
| Mission-control slots in header/status | ✓ |
| Copy: programs (not “contracts”) in onboarding/victory | ✓ |
| Settings: auto-pause arrival/anomaly, persistence | ✓ |
| Save export `.json` + file import | ✓ |

### Phase 2 — Visual polish

| Item | Status |
|------|--------|
| Venus/Jupiter textures (local + fallback) | ✓ |
| Shared palette `colors.ts` ↔ CSS tokens | ✓ |
| Satellite mesh (bus + panels) | ✓ |
| Orbit trail vertex fade | ✓ |
| Victory animation (+ reduced-motion guard) | ✓ |
| Globe legend via CSS classes | ✓ |
| Station pick radius 0.14 (tablet raycast) | ✓ |

### Phase 3 — Tablet + accessibility

| Item | Status |
|------|--------|
| Collapsible HUD + `--hud-clip-right` sync | `hud-layout.ts` |
| Loading stages (wasm → sim → world → ready) | `loading.ts`, `bridge.ts` |
| ARIA tablist, focus trap, keyboard guard | `focus-trap.ts`, `panel.ts` |
| Tab badges 1–5, fleet text labels | `panel.ts` |
| Breakpoint 768px, touch 44px, reduced-motion global | `style.css` |
| `BodyView.preloadBody("earth")` | `body-view.ts` |

### Render gotchas (documented)

- **Textures:** do **not** mirror equirectangular maps (`repeat.x = -1`); sphere UVs already match `stationLatLonToRender`. See `body-view.ts`.
- **Body view:** one central body at a time (craft `body_id`).
- **System map (M):** schematic top-down; radial compression `0.45`; longitudes true; only campaign bodies (not full solar system). Zoom out (scroll) to see Jupiter.

## UI platform track (U1+)

| Item | Status |
|------|--------|
| Split tab modules (`tabs/ops|orbit|transfer|company`) | ✓ U1 |
| Shared `html.ts` helpers + `ToastQueue` stack | ✓ U1 |
| Vitest unit tests (`format.test.ts`) in CI | ✓ U1 |
| Priority strip + marketplace filters + weather banner | ✓ U2 |
| Playwright e2e smoke | ✓ `web/e2e/smoke.spec.ts` |
| Smartphone bottom sheet | planned (optional) |

## Roadmap priority

1. **UI platform** — finish U1 e2e → U2 design system → U3 mobile.
2. **Systems** — M20 weather → M21 content → M22 prestige (see GDD).
3. **Optional later** — system-map auto-fit, WebGPU globe, audio.
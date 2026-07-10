# Satellite Inc — Architecture

## Three layers

```
Player input (HUD; Tweakpane only with `?debug`)
        │
        ▼
┌─────────────────────────────────────────────┐
│  WASM Bridge (crates/wasm)                   │
│  init / step / apply_action / drain_events   │
└─────────────────────┬───────────────────────┘
                      │ JSON Snapshot + Events
                      ▼
┌─────────────────────────────────────────────┐
│  Core (crates/core)                          │
│  SimClock, Kepler orbits, spacecraft, ops,   │
│  contracts — ZERO DOM/WebGL deps             │
└─────────────────────┬───────────────────────┘
                      │ Snapshot
                      ▼
┌─────────────────────────────────────────────┐
│  Presentation (web/)                         │
│  Three.js globe, tabbed HUD, onboarding      │
└─────────────────────────────────────────────┘
```

## Coordinate conventions

- **Sim:** ECI position in km (`f64`), Earth radius 6371 km.
- **Render:** Parent-relative offsets; `km * 0.001` → Three.js units. Y-up maps ECI Z (north pole).
- **View scale** is a render-only knob; physics always uses canonical km.

## WASM API

| Export | Purpose |
|--------|---------|
| `init(content_json)` | Load game content, reset sim |
| `step(real_dt)` | Advance sim, return Snapshot JSON |
| `apply_action(action_json)` | Player action, return Snapshot JSON |
| `drain_events()` | Poll event queue since last drain |
| `reset(content_json)` | Re-init |
| `export_save()` | Serialize sim state JSON |
| `import_save(save_json)` | Restore sim state, return Snapshot |

## Content pipeline

All templates in `content/game.json`: buses, ground stations, contracts, tech tree, TLE catalog, scenarios. Loaded at `init()`.

Player action `load_scenario` applies a scenario template (mode, cash, bus, optional contract).
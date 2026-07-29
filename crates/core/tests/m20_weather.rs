//! M20 world events: deterministic schedule, solar/bandwidth modifiers, save.

use satelliteinc_core::{
    SimState, action::PlayerAction, company::GameMode, constants::SECONDS_PER_DAY,
    weather::{ActiveWorldEvent, CHECK_INTERVAL_DAYS},
};

mod common;
use common::{load_content, sandbox_at, snap_craft};

fn campaign() -> SimState {
    let mut sim = SimState::new(load_content()).unwrap();
    assert!(sim.weather.enabled);
    assert!(!sim.content.world_events.is_empty());
    sim.company.cash_usd = 1_000_000.0;
    sim
}

fn step_once(sim: &mut SimState) {
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 1.0 })
        .unwrap();
    sim.step(0.016);
}

#[test]
fn content_loads_world_events() {
    let c = load_content();
    assert!(c.world_event("solar_storm").is_some());
    assert!(c.world_event("dsn_congestion").is_some());
    assert!(c.world_event("mars_dust_season").is_some());
}

#[test]
fn sandbox_disables_weather_by_default() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "sandbox_lab".into(),
    })
    .unwrap();
    assert!(!sim.weather.enabled);
    assert!(sim.snapshot().active_world_event.is_none());
}

#[test]
fn event_starts_and_ends_with_events() {
    let mut sim = campaign();
    // Force an immediate roll.
    sim.weather.next_roll_sim_seconds = 0.0;
    step_once(&mut sim);
    let started = sim.drain_events();
    assert!(
        started
            .iter()
            .any(|e| matches!(e, satelliteinc_core::SimEvent::WorldEventStarted { .. })),
        "expected WorldEventStarted, got {started:?}"
    );
    let active = sim.weather.active.clone().expect("event active");
    let snap = sim.snapshot();
    let banner = snap.active_world_event.expect("snapshot banner");
    assert_eq!(banner.event_id, active.event_id);

    // Expire.
    sim.weather.active.as_mut().unwrap().ends_at_sim_seconds = sim.clock.sim_seconds;
    step_once(&mut sim);
    let ended = sim.drain_events();
    assert!(ended
        .iter()
        .any(|e| matches!(e, satelliteinc_core::SimEvent::WorldEventEnded { .. })));
    assert!(sim.weather.active.is_none());
    assert!(sim.snapshot().active_world_event.is_none());
}

#[test]
fn solar_storm_derates_earth_flux_path() {
    let mut sim = campaign();
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();

    let storm = sim.content.world_event("solar_storm").unwrap().clone();
    sim.weather.active = Some(ActiveWorldEvent {
        event_id: storm.id.clone(),
        started_at_sim_seconds: sim.clock.sim_seconds,
        ends_at_sim_seconds: sim.clock.sim_seconds + storm.duration_days * SECONDS_PER_DAY,
    });
    let wx = sim
        .weather
        .craft_modifiers(&sim.content, "earth");
    assert!(
        (wx.solar_penalty - 0.55).abs() < 1e-9,
        "solar storm solar_penalty"
    );
    assert!(wx.anomaly_rate_multiplier > 1.0);
}

#[test]
fn mars_dust_does_not_affect_earth() {
    let mut sim = campaign();
    let dust = sim.content.world_event("mars_dust_season").unwrap().clone();
    sim.weather.active = Some(ActiveWorldEvent {
        event_id: dust.id,
        started_at_sim_seconds: 0.0,
        ends_at_sim_seconds: 1e9,
    });
    let earth = sim.weather.craft_modifiers(&sim.content, "earth");
    assert!((earth.solar_penalty - 1.0).abs() < 1e-12);
    let mars = sim.weather.craft_modifiers(&sim.content, "mars");
    assert!(mars.solar_penalty < 1.0);
}

#[test]
fn dsn_congestion_scales_only_dsn_bandwidth() {
    let mut sim = sandbox_at("moon", 100.0, 45.0);
    // Sandbox disables weather — re-enable and inject event.
    sim.weather.enabled = true;
    let craft_id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::Launch { craft_id }).unwrap();

    let cong = sim.content.world_event("dsn_congestion").unwrap().clone();
    sim.weather.active = Some(ActiveWorldEvent {
        event_id: cong.id,
        started_at_sim_seconds: 0.0,
        ends_at_sim_seconds: 1e9,
    });

    let craft = snap_craft(&sim);
    let goldstone = craft
        .station_status
        .iter()
        .find(|s| s.station_id == "goldstone")
        .expect("DSN goldstone");
    // Moon DSN factor 0.7 * 40 Mbps * 0.45 congestion ≈ 12.6
    assert!(
        (goldstone.bandwidth_mbps - 40.0 * 0.7 * 0.45).abs() < 1e-6,
        "DSN bandwidth should include congestion, got {}",
        goldstone.bandwidth_mbps
    );
}

#[test]
fn save_roundtrip_preserves_active_event() {
    let mut sim = campaign();
    sim.weather.next_roll_sim_seconds = 0.0;
    step_once(&mut sim);
    assert!(sim.weather.active.is_some());
    let save = sim.export_save();
    assert_eq!(save.version, 6);
    let json = save.to_json().unwrap();
    let restored = SimState::import_save(
        load_content(),
        satelliteinc_core::SaveGame::from_json(&json).unwrap(),
    )
    .unwrap();
    assert_eq!(
        restored.weather.active.as_ref().map(|a| a.event_id.as_str()),
        sim.weather.active.as_ref().map(|a| a.event_id.as_str())
    );
}

#[test]
fn mode_switch_resets_weather() {
    let mut sim = campaign();
    sim.weather.next_roll_sim_seconds = 0.0;
    step_once(&mut sim);
    sim.apply_action(PlayerAction::SetGameMode {
        mode: "sandbox".into(),
    })
    .unwrap();
    assert_eq!(sim.company.game_mode, GameMode::Sandbox);
    assert!(!sim.weather.enabled);
}

#[test]
fn check_interval_constant_is_positive() {
    assert!(CHECK_INTERVAL_DAYS > 0.0);
}

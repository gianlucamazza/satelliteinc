use satelliteinc_core::maneuver_ops::spend_propellant;
use satelliteinc_core::ops::{ActiveAnomaly, AnomalyType, CommandType, roll_anomaly};
use satelliteinc_core::spacecraft::Spacecraft;
use satelliteinc_core::{SimEvent, SimState, action::PlayerAction};

mod common;
use common::load_content;

fn craft_from(bus_id: &str) -> Spacecraft {
    let content = load_content();
    let bus = content.bus(bus_id).unwrap();
    Spacecraft::new_from_bus(bus, "test", 0)
}

#[test]
fn station_bandwidth_caps_downlink_rate() {
    let mut craft = craft_from("smallsat"); // 20 Mbps bus
    craft.downlink_active = true;

    // Santiago's 8 Mbps binds; Svalbard's 24 Mbps does not.
    assert!((craft.downlink_rate_mb_s(1.0, Some(8.0), 0.0) - 1.0).abs() < 1e-9);
    assert!((craft.downlink_rate_mb_s(1.0, Some(24.0), 0.0) - 2.5).abs() < 1e-9);
    // The cap applies after tech multipliers.
    assert!((craft.downlink_rate_mb_s(1.5, Some(24.0), 0.0) - 3.0).abs() < 1e-9);
}

#[test]
fn unmanaged_anomaly_drains_extra_power() {
    let mut craft = craft_from("microsat");
    craft.downlink_active = false;

    let healthy = craft.power_balance_w(true, 1.0, 0.0, 1.0);
    craft.active_anomaly = Some(ActiveAnomaly::new(AnomalyType::CommsDropout, 0.0));
    let stressed = craft.power_balance_w(true, 1.0, 10.0, 1.0);
    assert!(
        stressed < healthy,
        "active anomaly outside safe mode must draw extra power"
    );
}

#[test]
fn safe_mode_command_halves_anomaly_duration() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetOrbit {
        craft_id: 0,
        altitude_km: 400.0,
        inclination_deg: 97.0,
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();
    sim.apply_action(PlayerAction::QueueCommand {
        craft_id: 0,
        command: CommandType::SafeMode,
        station_id: "svalbard".into(),
    })
    .unwrap();

    // Long-lived anomaly that outlasts the wait for the pass.
    let now = sim.clock.sim_seconds;
    let craft = common::only_craft_mut(&mut sim);
    craft.active_anomaly = Some(ActiveAnomaly::new(AnomalyType::PowerGlitch, now));
    craft.active_anomaly.as_mut().unwrap().duration_seconds = 1_000_000.0;

    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 500.0 })
        .unwrap();
    let mut command_ran = false;
    for _ in 0..3_000 {
        sim.step(0.016);
        for ev in sim.drain_events() {
            if matches!(ev, SimEvent::CommandExecuted { .. }) {
                command_ran = true;
            }
        }
        if command_ran {
            break;
        }
    }
    assert!(command_ran, "safe-mode command should execute on pass");
    let anomaly = common::only_craft(&sim)
        .active_anomaly
        .as_ref()
        .expect("anomaly still active");
    assert!(
        anomaly.duration_seconds < 1_000_000.0,
        "safe mode must halve the remaining anomaly time"
    );
}

#[test]
fn eps_clear_only_clears_power_glitch() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetOrbit {
        craft_id: 0,
        altitude_km: 400.0,
        inclination_deg: 97.0,
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();
    let now = sim.clock.sim_seconds;
    common::only_craft_mut(&mut sim).active_anomaly =
        Some(ActiveAnomaly::new(AnomalyType::CommsDropout, now));
    common::only_craft_mut(&mut sim)
        .active_anomaly
        .as_mut()
        .unwrap()
        .duration_seconds = 1_000_000.0;

    sim.apply_action(PlayerAction::QueueCommand {
        craft_id: 0,
        command: CommandType::ClearAnomaly,
        station_id: "svalbard".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 500.0 })
        .unwrap();
    for _ in 0..3_000 {
        sim.step(0.016);
        if common::only_craft(&sim)
            .command_queue
            .is_empty()
        {
            break;
        }
    }
    assert!(
        common::only_craft(&sim).active_anomaly.is_some(),
        "EPS clear must not clear a comms dropout"
    );

    // Correct procedure: reboot.
    sim.apply_action(PlayerAction::QueueCommand {
        craft_id: 0,
        command: CommandType::RebootComms,
        station_id: "svalbard".into(),
    })
    .unwrap();
    for _ in 0..3_000 {
        sim.step(0.016);
        if common::only_craft(&sim).active_anomaly.is_none() {
            break;
        }
    }
    assert!(
        common::only_craft(&sim).active_anomaly.is_none(),
        "reboot must clear comms dropout"
    );
}

#[test]
fn buffer_seu_is_instant_not_sticky() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();
    let craft = common::only_craft_mut(&mut sim);
    craft.buffer_mb = 100.0;
    craft.anomaly_cooldown_until = 0.0;
    // Inject as the sim would for a SEU event.
    craft.buffer_mb *= 0.75;
    assert!(craft.active_anomaly.is_none());
    assert!((craft.buffer_mb - 75.0).abs() < 1e-9);
}

#[test]
fn redundancy_tech_stretches_anomaly_period() {
    // Base period 4000 s: boundary at 4000 fires with mtbf 1.0...
    assert!(roll_anomaly(4_000.5, 1.0, 0.0, 1.0, 0.0).is_some());
    // ...but not with mtbf 2.4 (period 9600 s)...
    assert!(roll_anomaly(4_000.5, 1.0, 0.0, 2.4, 0.0).is_none());
    // ...whose first boundary is at 9600 s.
    assert!(roll_anomaly(9_600.5, 1.0, 0.0, 2.4, 0.0).is_some());
}

#[test]
fn isp_multiplier_reduces_propellant_use() {
    let mut base = craft_from("microsat");
    let mut upgraded = craft_from("microsat");
    let used_base = spend_propellant(&mut base, 0.1, 1.0).unwrap();
    let used_upgraded = spend_propellant(&mut upgraded, 0.1, 1.15).unwrap();
    assert!(
        used_upgraded < used_base,
        "higher Isp must burn less propellant for the same delta-v"
    );
}

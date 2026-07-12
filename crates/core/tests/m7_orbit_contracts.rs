use satelliteinc_core::{SimState, action::PlayerAction, orbit::circular_orbit};

mod common;
use common::load_content;

fn launch(sim: &mut SimState, altitude_km: f64, inclination_deg: f64) {
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "microsat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetOrbit {
        craft_id: 0,
        altitude_km,
        inclination_deg,
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();
}

/// Downlink a pre-filled buffer with imaging and anomalies disabled, so the
/// buffer can only shrink via downlink. Returns the MB actually transferred.
fn downlink_some(sim: &mut SimState) -> f64 {
    sim.apply_action(PlayerAction::SetPayloadMode {
        craft_id: 0,
        mode: satelliteinc_core::spacecraft::PayloadMode::Safe,
    })
    .unwrap();
    {
        let craft = common::only_craft_mut(sim);
        craft.buffer_mb = 500.0;
        craft.anomaly_cooldown_until = f64::MAX;
    }
    sim.apply_action(PlayerAction::SetTimeAccel {
        multiplier: 1_000.0,
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetDownlink {
        craft_id: 0,
        active: true,
    })
    .unwrap();
    for _ in 0..2_000 {
        sim.step(0.1);
        if common::only_craft(sim).buffer_mb < 400.0 {
            break;
        }
    }
    500.0 - common::only_craft(sim).buffer_mb
}

#[test]
fn out_of_spec_orbit_downlinks_but_accrues_nothing() {
    let mut sim = SimState::new(load_content()).unwrap();
    launch(&mut sim, 400.0, 97.0); // needs >= 700 km
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "comms_relay".into(),
        assigned_craft_id: Some(0),
    })
    .unwrap();

    assert_eq!(sim.snapshot().active_contracts[0].orbit_ok, Some(false));

    let drained = downlink_some(&mut sim);
    assert!(
        drained > 50.0,
        "downlink itself must still flow ({drained} MB)"
    );
    let done = sim.company.active_contracts[0].downlink_mb_done;
    assert_eq!(done, 0.0, "non-compliant orbit must not accrue progress");
}

#[test]
fn compliant_orbit_accrues_progress() {
    let mut sim = SimState::new(load_content()).unwrap();
    launch(&mut sim, 400.0, 97.0);
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "comms_relay".into(),
        assigned_craft_id: Some(0),
    })
    .unwrap();

    // Jump straight to a compliant 720 km orbit.
    let now = sim.clock.sim_seconds;
    let craft = common::only_craft_mut(&mut sim);
    craft.elements = Some(circular_orbit(
        satelliteinc_core::constants::EARTH_RADIUS_KM,
        720.0,
        97.0,
        now,
    ));
    assert_eq!(sim.snapshot().active_contracts[0].orbit_ok, Some(true));

    let drained = downlink_some(&mut sim);
    assert!(drained > 50.0);
    let done = sim.company.active_contracts[0].downlink_mb_done;
    assert!(
        (done - drained).abs() < 1.0,
        "compliant orbit must accrue all downlinked data ({done} vs {drained})"
    );
}

#[test]
fn inclination_requirement_gates_polar_watch() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.company.reputation = 30.0; // clear the rep gate
    launch(&mut sim, 500.0, 51.6); // needs >= 80 deg
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "polar_watch".into(),
        assigned_craft_id: Some(0),
    })
    .unwrap();
    assert_eq!(sim.snapshot().active_contracts[0].orbit_ok, Some(false));

    let now = sim.clock.sim_seconds;
    let craft = common::only_craft_mut(&mut sim);
    craft.elements = Some(circular_orbit(
        satelliteinc_core::constants::EARTH_RADIUS_KM,
        500.0,
        97.0,
        now,
    ));
    assert_eq!(sim.snapshot().active_contracts[0].orbit_ok, Some(true));
}

#[test]
fn unconstrained_contract_reports_orbit_ok() {
    let mut sim = SimState::new(load_content()).unwrap();
    launch(&mut sim, 400.0, 97.0);
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "earth_imaging".into(),
        assigned_craft_id: Some(0),
    })
    .unwrap();
    assert_eq!(sim.snapshot().active_contracts[0].orbit_ok, Some(true));
}

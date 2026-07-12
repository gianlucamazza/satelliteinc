use satelliteinc_core::{SimState, action::PlayerAction, orbit::circular_orbit};

mod common;
use common::load_content;

fn launch(sim: &mut SimState, altitude_km: f64, inclination_deg: f64) {
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "microsat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetOrbit {
        altitude_km,
        inclination_deg,
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();
}

/// Downlink a pre-filled buffer with imaging and anomalies disabled, so the
/// buffer can only shrink via downlink. Returns the MB actually transferred.
fn downlink_some(sim: &mut SimState) -> f64 {
    sim.apply_action(PlayerAction::SetPayloadMode {
        mode: satelliteinc_core::spacecraft::PayloadMode::Safe,
    })
    .unwrap();
    {
        let craft = sim.spacecraft.as_mut().unwrap();
        craft.buffer_mb = 500.0;
        craft.anomaly_cooldown_until = f64::MAX;
    }
    sim.apply_action(PlayerAction::SetTimeAccel {
        multiplier: 1_000.0,
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetDownlink { active: true })
        .unwrap();
    for _ in 0..2_000 {
        sim.step(0.1);
        if sim.spacecraft.as_ref().unwrap().buffer_mb < 400.0 {
            break;
        }
    }
    500.0 - sim.spacecraft.as_ref().unwrap().buffer_mb
}

#[test]
fn out_of_spec_orbit_downlinks_but_accrues_nothing() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "comms_relay".into(),
    })
    .unwrap();
    launch(&mut sim, 400.0, 97.0); // needs >= 700 km

    assert_eq!(sim.snapshot().active_contract_orbit_ok, Some(false));

    let drained = downlink_some(&mut sim);
    assert!(
        drained > 50.0,
        "downlink itself must still flow ({drained} MB)"
    );
    let done = sim
        .company
        .active_contract
        .as_ref()
        .unwrap()
        .downlink_mb_done;
    assert_eq!(done, 0.0, "non-compliant orbit must not accrue progress");
}

#[test]
fn compliant_orbit_accrues_progress() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "comms_relay".into(),
    })
    .unwrap();
    launch(&mut sim, 400.0, 97.0);

    // Jump straight to a compliant 720 km orbit.
    let craft = sim.spacecraft.as_mut().unwrap();
    craft.elements = Some(circular_orbit(
        satelliteinc_core::constants::EARTH_RADIUS_KM,
        720.0,
        97.0,
        sim.clock.sim_seconds,
    ));
    assert_eq!(sim.snapshot().active_contract_orbit_ok, Some(true));

    let drained = downlink_some(&mut sim);
    assert!(drained > 50.0);
    let done = sim
        .company
        .active_contract
        .as_ref()
        .unwrap()
        .downlink_mb_done;
    assert!(
        (done - drained).abs() < 1.0,
        "compliant orbit must accrue all downlinked data ({done} vs {drained})"
    );
}

#[test]
fn inclination_requirement_gates_polar_watch() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.company.reputation = 30.0; // clear the rep gate
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "polar_watch".into(),
    })
    .unwrap();

    launch(&mut sim, 500.0, 51.6); // needs >= 80 deg
    assert_eq!(sim.snapshot().active_contract_orbit_ok, Some(false));

    let craft = sim.spacecraft.as_mut().unwrap();
    craft.elements = Some(circular_orbit(
        satelliteinc_core::constants::EARTH_RADIUS_KM,
        500.0,
        97.0,
        sim.clock.sim_seconds,
    ));
    assert_eq!(sim.snapshot().active_contract_orbit_ok, Some(true));
}

#[test]
fn unconstrained_contract_reports_orbit_ok() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "earth_imaging".into(),
    })
    .unwrap();
    launch(&mut sim, 400.0, 97.0);
    assert_eq!(sim.snapshot().active_contract_orbit_ok, Some(true));
}

//! M11: interplanetary transfers end to end — windows, injection economics,
//! hibernated cruise, automatic capture, warp determinism.

use satelliteinc_core::spacecraft::CraftPhase;
use satelliteinc_core::{SimEvent, SimState, action::PlayerAction};

mod common;
use common::load_content;

/// Tutorial scenario: explorer in LEO, deep-space ops certified,
/// moon_mapper pre-accepted.
fn leo_sim() -> SimState {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "lunar_express".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();
    sim
}

fn set_moon_target(sim: &mut SimState) {
    sim.apply_action(PlayerAction::SetTransferTarget {
        body_id: "moon".into(),
        capture_altitude_km: 100.0,
        capture_inclination_deg: 86.0,
    })
    .unwrap();
}

#[test]
fn full_moon_transfer_arrives_and_captures() {
    let mut sim = leo_sim();
    set_moon_target(&mut sim);

    let cash_before = sim.company.cash_usd;
    sim.apply_action(PlayerAction::BeginTransfer).unwrap();

    // Injection billed at the Moon's 1.5x multiplier on the explorer's 450k.
    assert!((cash_before - sim.company.cash_usd - 675_000.0).abs() < 1.0);
    let craft = sim.spacecraft.as_ref().unwrap();
    assert_eq!(craft.phase, CraftPhase::Transfer);
    assert_eq!(craft.body_id, "earth", "TLI coast is geocentric");
    let arrival = craft.transfer.as_ref().unwrap().arrival_sim_seconds;
    let days = arrival / 86_400.0;
    assert!((4.0..6.0).contains(&days), "TLI coast {days} days");

    // Warp to arrival in chunks.
    let mut guard = 0;
    while sim.clock.sim_seconds < arrival && guard < 10 {
        sim.apply_action(PlayerAction::WarpTo {
            target_sim_seconds: arrival + 1.0,
        })
        .unwrap();
        guard += 1;
    }
    let craft = sim.spacecraft.as_ref().unwrap();
    assert_eq!(craft.phase, CraftPhase::OnOrbit);
    assert_eq!(craft.body_id, "moon");
    let elements = craft.elements.unwrap();
    assert!((elements.semi_major_axis_km - 1_837.4).abs() < 1.0);
    assert!((elements.inclination_deg - 86.0).abs() < 1e-9);

    let events = sim.drain_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SimEvent::TransferArrived { body_id } if body_id == "moon"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SimEvent::CaptureExecuted { .. }))
    );
}

#[test]
fn capture_propellant_is_reserved_up_front() {
    let mut sim = leo_sim();
    set_moon_target(&mut sim);
    let prop_before = sim.spacecraft.as_ref().unwrap().propellant_kg;
    sim.apply_action(PlayerAction::BeginTransfer).unwrap();
    let craft = sim.spacecraft.as_ref().unwrap();
    let reserved = craft
        .transfer
        .as_ref()
        .unwrap()
        .capture_propellant_reserved_kg;
    assert!(reserved > 0.0);
    assert!((prop_before - craft.propellant_kg - reserved).abs() < 1e-9);
}

#[test]
fn hibernation_slashes_cruise_ops_cost() {
    let mut sim = leo_sim();
    let on_orbit_cost = sim.current_ops_cost_usd_per_day();
    set_moon_target(&mut sim);
    sim.apply_action(PlayerAction::BeginTransfer).unwrap();
    let cruise_cost = sim.current_ops_cost_usd_per_day();
    // Explorer: 1000 + 20000 on orbit vs 1000 + 2000 hibernated.
    assert!((on_orbit_cost - 21_000.0).abs() < 1.0);
    assert!((cruise_cost - 3_000.0).abs() < 1.0);
}

#[test]
fn warp_equals_manual_stepping() {
    let run_warp = || {
        let mut sim = leo_sim();
        sim.apply_action(PlayerAction::WarpTo {
            target_sim_seconds: 40_000.0,
        })
        .unwrap();
        (
            sim.clock.sim_seconds.to_bits(),
            sim.company.cash_usd.to_bits(),
            sim.snapshot().spacecraft.unwrap().position_eci_km[0].to_bits(),
        )
    };
    let run_manual = || {
        let mut sim = leo_sim();
        sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 1.0 })
            .unwrap();
        for _ in 0..40_000 {
            sim.step(1.0);
        }
        (
            sim.clock.sim_seconds.to_bits(),
            sim.company.cash_usd.to_bits(),
            sim.snapshot().spacecraft.unwrap().position_eci_km[0].to_bits(),
        )
    };
    assert_eq!(run_warp(), run_manual(), "warp must be bit-identical");
}

#[test]
fn transfer_needs_open_window_cash_and_certification() {
    // Mars window: usually closed — expect a clear rejection with ETA.
    let mut sim = leo_sim();
    sim.apply_action(PlayerAction::SetTransferTarget {
        body_id: "mars".into(),
        capture_altitude_km: 400.0,
        capture_inclination_deg: 30.0,
    })
    .unwrap();
    let planner = sim.snapshot().spacecraft.unwrap().transfer_planner.unwrap();
    if !planner.window_open {
        let err = sim.apply_action(PlayerAction::BeginTransfer);
        assert!(err.is_err(), "closed window must reject the injection");
        assert!(planner.next_window_sim_seconds > sim.clock.sim_seconds);
    }

    // No route from the Moon to Mars directly.
    let mut sim2 = leo_sim();
    set_moon_target(&mut sim2);
    sim2.apply_action(PlayerAction::BeginTransfer).unwrap();
    let arrival = sim2
        .spacecraft
        .as_ref()
        .unwrap()
        .transfer
        .as_ref()
        .unwrap()
        .arrival_sim_seconds;
    sim2.apply_action(PlayerAction::WarpTo {
        target_sim_seconds: arrival + 1.0,
    })
    .unwrap();
    sim2.apply_action(PlayerAction::SetTransferTarget {
        body_id: "mars".into(),
        capture_altitude_km: 400.0,
        capture_inclination_deg: 30.0,
    })
    .unwrap();
    assert!(sim2.apply_action(PlayerAction::BeginTransfer).is_err());
}

#[test]
fn mid_transfer_save_round_trips() {
    let mut sim = leo_sim();
    set_moon_target(&mut sim);
    sim.apply_action(PlayerAction::BeginTransfer).unwrap();
    sim.apply_action(PlayerAction::WarpTo {
        target_sim_seconds: 100_000.0,
    })
    .unwrap();

    let json = sim.export_save().to_json().unwrap();
    let mut restored = SimState::import_save(
        sim.content.clone(),
        satelliteinc_core::SaveGame::from_json(&json).unwrap(),
    )
    .unwrap();
    let arrival = {
        let craft = restored.spacecraft.as_ref().unwrap();
        assert_eq!(craft.phase, CraftPhase::Transfer);
        let t = craft.transfer.as_ref().unwrap();
        assert_eq!(t.target_body_id, "moon");
        t.arrival_sim_seconds
    };

    // The restored cruise still arrives and captures.
    restored
        .apply_action(PlayerAction::WarpTo {
            target_sim_seconds: arrival + 1.0,
        })
        .unwrap();
    assert_eq!(restored.spacecraft.as_ref().unwrap().body_id, "moon");
}

#[test]
fn transfers_gated_by_certification_in_campaign() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "campaign_start".into(),
    })
    .unwrap();
    assert!(
        !sim.snapshot().transfers_unlocked,
        "campaign starts without deep-space certification"
    );
    let mut sandbox = SimState::new(load_content()).unwrap();
    sandbox
        .apply_action(PlayerAction::LoadScenario {
            scenario_id: "sandbox_lab".into(),
        })
        .unwrap();
    assert!(sandbox.snapshot().transfers_unlocked);
}

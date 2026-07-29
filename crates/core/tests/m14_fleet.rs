//! M14: the fleet — concurrent spacecraft and contracts, explicit 1:1
//! assignment, per-craft pass edges, save v4 migration, determinism.

use satelliteinc_core::{SaveGame, SimEvent, SimState, action::PlayerAction};

mod common;
use common::load_content;

/// Sandbox with two cubesats on different Earth orbits.
fn two_craft_sim() -> SimState {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "sandbox_lab".into(),
    })
    .unwrap();
    // Craft 0 comes from the scenario; polar orbit.
    sim.apply_action(PlayerAction::SetOrbit {
        craft_id: 0,
        altitude_km: 500.0,
        inclination_deg: 97.0,
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();
    // Craft 1: equatorial-ish low orbit.
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetOrbit {
        craft_id: 1,
        altitude_km: 400.0,
        inclination_deg: 30.0,
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 1 })
        .unwrap();
    sim
}

#[test]
fn snapshot_exposes_flight_slots() {
    let sim = two_craft_sim();
    let snap = sim.snapshot();
    assert_eq!(snap.max_flights, 6, "sandbox mission control cap");
    assert_eq!(snap.active_flights, 2);
}

#[test]
fn two_craft_fly_independent_orbits() {
    let mut sim = two_craft_sim();
    sim.apply_action(PlayerAction::SetTimeAccel {
        multiplier: 1_000.0,
    })
    .unwrap();
    for _ in 0..500 {
        sim.step(0.016);
    }
    let snap = sim.snapshot();
    assert_eq!(snap.spacecraft.len(), 2);
    let a = &snap.spacecraft[0];
    let b = &snap.spacecraft[1];
    assert_eq!(a.id, 0);
    assert_eq!(b.id, 1);
    assert!((a.altitude_km - 500.0).abs() < 5.0);
    assert!((b.altitude_km - 400.0).abs() < 5.0);
    // Different inclinations → genuinely different planes.
    assert!((a.elements.unwrap().inclination_deg - 97.0).abs() < 1e-9);
    assert!((b.elements.unwrap().inclination_deg - 30.0).abs() < 1e-9);
}

#[test]
fn mission_control_slots_gate_campaign_launches() {
    // Campaign base: 2 slots. Completed programs and the annex add more.
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "campaign_start".into(),
    })
    .unwrap();
    sim.company.cash_usd = 2_000_000.0;
    assert_eq!(sim.max_flights(), 2);
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    let id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::Launch { craft_id: id })
        .unwrap();
    // Two flights up: the third hits the base cap.
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    let id = sim.fleet.last().unwrap().id;
    assert!(
        sim.apply_action(PlayerAction::Launch { craft_id: id })
            .is_err(),
        "base mission control cap must hold"
    );
    // A completed program grows the room — and bills its staffing.
    let ops_before = sim.current_ops_cost_usd_per_day();
    sim.company.completed_programs.push("earth_program".into());
    assert_eq!(sim.max_flights(), 3);
    assert!(
        sim.current_ops_cost_usd_per_day() > ops_before,
        "extra slots bill staffing"
    );
    sim.apply_action(PlayerAction::Launch { craft_id: id })
        .unwrap();
}

#[test]
fn contracts_accrue_only_from_the_assigned_craft() {
    let mut sim = two_craft_sim();
    // Two concurrent contracts, one per craft.
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "earth_imaging".into(),
        assigned_craft_id: Some(0),
    })
    .unwrap();
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "rapid_eye".into(),
        assigned_craft_id: Some(1),
    })
    .unwrap();
    assert_eq!(sim.company.active_contracts.len(), 2);

    // Only craft 0 downlinks.
    sim.apply_action(PlayerAction::SetDownlink {
        craft_id: 0,
        active: true,
    })
    .unwrap();
    sim.apply_action(PlayerAction::SelectDownlinkStation {
        craft_id: 0,
        station_id: "svalbard".into(),
    })
    .unwrap();
    {
        let craft = &mut sim.fleet[0];
        craft.buffer_mb = 500.0;
        craft.anomaly_cooldown_until = f64::MAX;
        sim.fleet[1].anomaly_cooldown_until = f64::MAX;
    }
    sim.apply_action(PlayerAction::SetTimeAccel {
        multiplier: 1_000.0,
    })
    .unwrap();
    for _ in 0..2_000 {
        sim.step(0.1);
    }
    let c0 = sim
        .company
        .active_contracts
        .iter()
        .find(|c| c.template_id == "earth_imaging");
    let c1 = sim
        .company
        .active_contracts
        .iter()
        .find(|c| c.template_id == "rapid_eye");
    // earth_imaging may have completed (removed) — then it accrued plenty.
    let accrued0 = c0.map_or(f64::MAX, |c| c.downlink_mb_done);
    assert!(accrued0 > 0.0, "assigned craft must accrue");
    assert_eq!(
        c1.unwrap().downlink_mb_done,
        0.0,
        "craft 1 never downlinked: its contract must stay at zero"
    );
}

#[test]
fn one_craft_cannot_serve_two_contracts() {
    let mut sim = two_craft_sim();
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "earth_imaging".into(),
        assigned_craft_id: Some(0),
    })
    .unwrap();
    let err = sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "rapid_eye".into(),
        assigned_craft_id: Some(0),
    });
    assert!(err.is_err(), "1:1 binding must reject a second assignment");
    // Unassigned acceptance is fine; assigning later to the busy craft isn't.
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "rapid_eye".into(),
        assigned_craft_id: None,
    })
    .unwrap();
    let err = sim.apply_action(PlayerAction::AssignContract {
        contract_id: "rapid_eye".into(),
        craft_id: Some(0),
    });
    assert!(err.is_err());
    sim.apply_action(PlayerAction::AssignContract {
        contract_id: "rapid_eye".into(),
        craft_id: Some(1),
    })
    .unwrap();
}

#[test]
fn reassignment_resets_the_on_station_clock() {
    let mut sim = two_craft_sim();
    sim.company.reputation = 200.0;
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "earth_imaging".into(),
        assigned_craft_id: Some(0),
    })
    .unwrap();
    sim.company.active_contracts[0].on_station_seconds = 12_345.0;
    sim.apply_action(PlayerAction::AssignContract {
        contract_id: "earth_imaging".into(),
        craft_id: Some(1),
    })
    .unwrap();
    assert_eq!(
        sim.company.active_contracts[0].on_station_seconds, 0.0,
        "no tag-teaming presence contracts"
    );
    assert_eq!(sim.company.active_contracts[0].assigned_craft_id, Some(1));
}

#[test]
fn decommission_unassigns_but_keeps_the_contract() {
    let mut sim = two_craft_sim();
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "earth_imaging".into(),
        assigned_craft_id: Some(0),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Decommission { craft_id: 0 })
        .unwrap();
    assert_eq!(sim.fleet.len(), 1);
    assert_eq!(sim.company.active_contracts.len(), 1);
    assert_eq!(sim.company.active_contracts[0].assigned_craft_id, None);
    assert!(sim.drain_events().iter().any(|e| matches!(
        e,
        SimEvent::ContractAssignmentChanged { craft_id: None, .. }
    )));
}

#[test]
fn removing_the_middle_craft_preserves_pass_edges() {
    let mut sim = two_craft_sim();
    // Third craft so there's a "middle".
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    let id2 = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::Launch { craft_id: id2 })
        .unwrap();
    // Warm the pass matrix.
    sim.apply_action(PlayerAction::SetTimeAccel {
        multiplier: 1_000.0,
    })
    .unwrap();
    for _ in 0..600 {
        sim.step(0.1);
    }
    let edges_craft2_before = sim.pass_active[2].clone();
    // Remove the middle craft (fleet idx 1).
    sim.apply_action(PlayerAction::Decommission { craft_id: 1 })
        .unwrap();
    assert_eq!(sim.fleet.len(), 2);
    assert_eq!(sim.pass_active.len(), 2);
    assert_eq!(
        sim.pass_active[1], edges_craft2_before,
        "craft 2's edges must follow it to index 1"
    );
    // Ticks keep working with no PassStarted storm from misalignment.
    let _ = sim.drain_events();
    sim.step(0.016);
}

#[test]
fn save_v4_round_trips_two_craft_and_contracts() {
    let mut sim = two_craft_sim();
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "earth_imaging".into(),
        assigned_craft_id: Some(1),
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetTimeAccel {
        multiplier: 1_000.0,
    })
    .unwrap();
    for _ in 0..200 {
        sim.step(0.1);
    }

    let json = sim.export_save().to_json().unwrap();
    let restored =
        SimState::import_save(sim.content.clone(), SaveGame::from_json(&json).unwrap()).unwrap();
    assert_eq!(restored.fleet.len(), 2);
    assert_eq!(restored.next_craft_id, sim.next_craft_id);
    assert_eq!(restored.pass_active, sim.pass_active);
    assert_eq!(
        restored.company.active_contracts[0].assigned_craft_id,
        Some(1)
    );
    // Both keep propagating identically after the round trip.
    let mut a = sim;
    let mut b = restored;
    for _ in 0..100 {
        a.step(0.1);
        b.step(0.1);
    }
    let sa = a.snapshot();
    let sb = b.snapshot();
    for (ca, cb) in sa.spacecraft.iter().zip(&sb.spacecraft) {
        assert_eq!(
            ca.position_eci_km[0].to_bits(),
            cb.position_eci_km[0].to_bits()
        );
    }
}

#[test]
fn two_craft_run_is_self_deterministic() {
    let run = || {
        let mut sim = two_craft_sim();
        sim.apply_action(PlayerAction::SetDownlink {
            craft_id: 0,
            active: true,
        })
        .unwrap();
        sim.apply_action(PlayerAction::SetTimeAccel {
            multiplier: 1_000.0,
        })
        .unwrap();
        for _ in 0..1_000 {
            sim.step(0.016);
        }
        let snap = sim.snapshot();
        (
            snap.cash_usd.to_bits(),
            snap.spacecraft[0].position_eci_km[0].to_bits(),
            snap.spacecraft[1].position_eci_km[0].to_bits(),
            snap.spacecraft[0].power_wh.to_bits(),
            snap.spacecraft[1].power_wh.to_bits(),
        )
    };
    assert_eq!(run(), run());
}

#[test]
fn ops_cost_sums_the_fleet() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "campaign_start".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();
    // cubesat on orbit: 1000 + 4000.
    assert!((sim.current_ops_cost_usd_per_day() - 5_000.0).abs() < 1e-9);
    sim.company.cash_usd = 1_000_000.0;
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    let id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::Launch { craft_id: id })
        .unwrap();
    // Two cubesats: 1000 + 4000 + 4000.
    assert!((sim.current_ops_cost_usd_per_day() - 9_000.0).abs() < 1e-9);
}

#[test]
fn accepting_an_already_active_template_is_rejected() {
    let mut sim = two_craft_sim();
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "earth_imaging".into(),
        assigned_craft_id: Some(0),
    })
    .unwrap();
    let err = sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "earth_imaging".into(),
        assigned_craft_id: Some(1),
    });
    assert!(err.is_err());
}

#[test]
fn anomaly_schedules_decorrelate_between_craft() {
    use satelliteinc_core::ops::{ANOMALY_PHASE_STRIDE_SECONDS, roll_anomaly};
    // Craft 0 keeps the historical schedule (offset 0); craft 1 must not
    // fire at the same tick with the same kind across the first buckets.
    let mut same = 0;
    let mut fired = 0;
    for tick in 1..40_000_u32 {
        let t = f64::from(tick);
        let a = roll_anomaly(t, 1.0, 0.0, 1.0, 0.0);
        let b = roll_anomaly(t, 1.0, 0.0, 1.0, ANOMALY_PHASE_STRIDE_SECONDS);
        if a.is_some() {
            fired += 1;
            if a == b {
                same += 1;
            }
        }
    }
    assert!(fired > 0);
    assert_eq!(same, 0, "offset craft must never mirror craft 0's schedule");
}

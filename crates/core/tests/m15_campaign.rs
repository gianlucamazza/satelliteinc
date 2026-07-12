//! M15: Mission Control slots and the fleet's next-event predictor.

use satelliteinc_core::{SimState, action::PlayerAction};

mod common;
use common::load_content;

#[test]
fn annex_tech_adds_a_flight_slot() {
    let mut sim = common::interplanetary_campaign(&[], 2_000_000.0, 100.0);
    assert_eq!(sim.max_flights(), 2);
    sim.apply_action(PlayerAction::PurchaseTech {
        tech_id: "mission_control_annex".into(),
    })
    .unwrap();
    assert_eq!(sim.max_flights(), 3);
    // The annex bills its staffing even before it's flown.
    assert!(
        sim.current_ops_cost_usd_per_day() >= satelliteinc_core::sim::SLOT_OVERHEAD_USD_PER_DAY
    );
}

#[test]
fn next_event_tracks_the_earliest_deadline_or_pass() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "campaign_start".into(),
    })
    .unwrap();
    // Pre-launch: only the seeded contract's deadline is on the horizon.
    let deadline = sim.company.active_contracts[0].deadline_sim_seconds;
    assert_eq!(sim.next_fleet_event_sim_seconds(), Some(deadline));

    // Once flying, the next pass (minutes away) beats the deadline (days).
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();
    let next = sim
        .next_fleet_event_sim_seconds()
        .expect("a pass or the deadline");
    assert!(next <= deadline);
    assert!(next > sim.clock.sim_seconds);

    // Warping to it is a legal, deterministic move.
    sim.apply_action(PlayerAction::WarpTo {
        target_sim_seconds: next,
    })
    .unwrap();
    assert!((sim.clock.sim_seconds - next).abs() < 1.5);
}

#[test]
fn next_event_covers_transfer_arrivals() {
    let mut sim = common::interplanetary_campaign(&["moon_program"], 1_400_000.0, 70.0);
    let craft_id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::Launch { craft_id }).unwrap();
    sim.apply_action(PlayerAction::SetTransferTarget {
        craft_id,
        body_id: "moon".into(),
        capture_altitude_km: 100.0,
        capture_inclination_deg: 85.0,
    })
    .unwrap();
    sim.apply_action(PlayerAction::BeginTransfer { craft_id })
        .unwrap();
    let arrival = sim.fleet[0].transfer.as_ref().unwrap().arrival_sim_seconds;
    // With no contracts and the craft hibernated, the arrival IS the event.
    assert_eq!(sim.next_fleet_event_sim_seconds(), Some(arrival));
}

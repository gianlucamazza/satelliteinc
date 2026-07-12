use satelliteinc_core::{SimEvent, SimState, action::PlayerAction};

mod common;
use common::load_content;

#[test]
fn decommission_salvages_and_returns_to_design() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "earth_imaging".into(),
        assigned_craft_id: Some(0),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();
    // Register some pass state to prove it gets cleared.
    sim.apply_action(PlayerAction::SetTimeAccel {
        multiplier: 1_000.0,
    })
    .unwrap();
    for _ in 0..200 {
        sim.step(0.1);
    }

    let cash_before = sim.company.cash_usd;
    sim.apply_action(PlayerAction::Decommission { craft_id: 0 })
        .unwrap();

    // 15% of the cubesat's 120k launch cost.
    assert!((sim.company.cash_usd - cash_before - 18_000.0).abs() < 1.0);
    assert!(sim.fleet.is_empty());
    assert!(
        sim.pass_active.iter().flatten().all(|&active| !active),
        "stale pass edges must be cleared for the next mission"
    );
    assert!(
        !sim.company.active_contracts.is_empty(),
        "the contract keeps running through a re-launch"
    );
    assert!(
        sim.drain_events()
            .iter()
            .any(|e| matches!(e, SimEvent::Decommissioned { .. }))
    );

    // Re-launch path works: pick a new bus and fly again (fresh id).
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    let new_id = sim.fleet[0].id;
    assert_eq!(new_id, 1, "craft ids are never reused after decommission");
    sim.apply_action(PlayerAction::Launch { craft_id: new_id })
        .unwrap();
}

#[test]
fn decommission_requires_an_on_orbit_craft() {
    let mut sim = SimState::new(load_content()).unwrap();
    assert!(
        sim.apply_action(PlayerAction::Decommission { craft_id: 0 })
            .is_err()
    );

    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    assert!(
        sim.apply_action(PlayerAction::Decommission { craft_id: 0 })
            .is_err(),
        "design-phase craft cannot be decommissioned"
    );
}

#[test]
fn select_bus_adds_a_second_craft_while_on_orbit() {
    // Fleet era: designing a new craft never touches the one flying.
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();

    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "microsat".into(),
    })
    .unwrap();
    assert_eq!(sim.fleet.len(), 2);
    assert_eq!(sim.fleet[0].id, 0);
    assert_eq!(sim.fleet[1].id, 1, "new craft gets the next monotonic id");
    assert_eq!(
        sim.fleet[0].phase,
        satelliteinc_core::spacecraft::CraftPhase::OnOrbit
    );
    // Re-selecting replaces only the Design craft, with a fresh id.
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    assert_eq!(sim.fleet.len(), 2);
    assert_eq!(sim.fleet[1].id, 2, "ids are never reused");
}

use satelliteinc_core::{SimEvent, SimState, action::PlayerAction};

mod common;
use common::load_content;

#[test]
fn decommission_salvages_and_returns_to_design() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "earth_imaging".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();
    // Register some pass state to prove it gets cleared.
    sim.apply_action(PlayerAction::SetTimeAccel {
        multiplier: 1_000.0,
    })
    .unwrap();
    for _ in 0..200 {
        sim.step(0.1);
    }

    let cash_before = sim.company.cash_usd;
    sim.apply_action(PlayerAction::Decommission).unwrap();

    // 15% of the cubesat's 120k launch cost.
    assert!((sim.company.cash_usd - cash_before - 18_000.0).abs() < 1.0);
    assert!(sim.spacecraft.is_none());
    assert!(
        sim.pass_active.iter().all(|&active| !active),
        "stale pass edges must be cleared for the next mission"
    );
    assert!(
        sim.company.active_contract.is_some(),
        "the contract keeps running through a re-launch"
    );
    assert!(
        sim.drain_events()
            .iter()
            .any(|e| matches!(e, SimEvent::Decommissioned { .. }))
    );

    // Re-launch path works: pick a new bus and fly again.
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();
}

#[test]
fn decommission_requires_an_on_orbit_craft() {
    let mut sim = SimState::new(load_content()).unwrap();
    assert!(sim.apply_action(PlayerAction::Decommission).is_err());

    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    assert!(
        sim.apply_action(PlayerAction::Decommission).is_err(),
        "design-phase craft cannot be decommissioned"
    );
}

#[test]
fn select_bus_rejected_while_on_orbit() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();

    assert!(
        sim.apply_action(PlayerAction::SelectBus {
            bus_id: "microsat".into(),
        })
        .is_err(),
        "swapping hardware mid-flight must require decommission first"
    );
}

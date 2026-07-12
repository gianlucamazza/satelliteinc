use satelliteinc_core::{SimState, action::PlayerAction, company::GameMode};

mod common;
use common::load_content;

#[test]
fn rd_unlocks_smallsat_and_marketplace_gates_contracts() {
    let content = load_content();
    let mut sim = SimState::new(content).unwrap();

    sim.apply_action(PlayerAction::SetGameMode {
        mode: "campaign".into(),
    })
    .unwrap();
    assert_eq!(sim.company.game_mode, GameMode::Campaign);

    let market = sim.company.marketplace(&sim.content);
    let science = market
        .iter()
        .find(|c| c.template_id == "science_multi")
        .unwrap();
    assert!(!science.available, "science contract locked at low rep");

    sim.apply_action(PlayerAction::PurchaseTech {
        tech_id: "comms_mk2".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::PurchaseTech {
        tech_id: "smallsat_bus".into(),
    })
    .unwrap();

    assert!(sim.company.is_bus_unlocked("smallsat"));
    assert!(
        sim.company
            .unlocked_tech
            .contains(&"smallsat_bus".to_string())
    );
}

#[test]
fn tech_purchase_requires_cash_and_prerequisites() {
    let content = load_content();
    let mut sim = SimState::new(content).unwrap();

    // Missing prerequisite (smallsat_bus requires comms_mk2).
    assert!(
        sim.apply_action(PlayerAction::PurchaseTech {
            tech_id: "smallsat_bus".into(),
        })
        .is_err()
    );

    // Insufficient cash.
    sim.company.cash_usd = 10_000.0;
    assert!(
        sim.apply_action(PlayerAction::PurchaseTech {
            tech_id: "comms_mk2".into(),
        })
        .is_err()
    );
    assert!(sim.company.unlocked_tech.is_empty());
    assert!(
        (sim.company.cash_usd - 10_000.0).abs() < 1e-9,
        "no charge on failure"
    );
}

#[test]
fn contract_deadline_expiry_fails_contract_and_costs_reputation() {
    let content = load_content();
    let mut sim = SimState::new(content).unwrap();

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
    let rep_before = sim.company.reputation;

    // Let the 7-day deadline lapse without downlinking anything.
    sim.apply_action(PlayerAction::SetTimeAccel {
        multiplier: 10_000.0,
    })
    .unwrap();
    let mut failed = false;
    for _ in 0..70 {
        sim.step(1.0);
        for ev in sim.drain_events() {
            if matches!(ev, satelliteinc_core::SimEvent::ContractFailed { .. }) {
                failed = true;
            }
        }
        if failed {
            break;
        }
    }

    assert!(failed, "deadline expiry must emit ContractFailed");
    assert!(sim.company.active_contracts.is_empty());
    assert!(
        sim.company.reputation < rep_before,
        "failure must cost reputation"
    );
}

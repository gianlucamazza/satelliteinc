use satelliteinc_core::{SimState, action::PlayerAction, company::GameMode};

mod common;
use common::load_content;

#[test]
fn sandbox_unlocks_all_buses_and_contracts() {
    let content = load_content();
    let mut sim = SimState::new(content).unwrap();

    sim.apply_action(PlayerAction::SetGameMode {
        mode: "sandbox".into(),
    })
    .unwrap();

    assert_eq!(sim.company.game_mode, GameMode::Sandbox);
    assert_eq!(sim.company.unlocked_buses.len(), 4);
    let market = sim.company.marketplace(&sim.content);
    assert!(market.iter().all(|o| o.available));
}

#[test]
fn campaign_gates_contracts_by_reputation() {
    let content = load_content();
    let sim = SimState::new(content).unwrap();
    let market = sim.company.marketplace(&sim.content);
    let science = market
        .iter()
        .find(|c| c.template_id == "science_multi")
        .unwrap();
    assert!(!science.available);
}

#[test]
fn multi_station_contract_requires_distinct_stations() {
    let content = load_content();
    let mut sim = SimState::new(content).unwrap();

    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "sandbox_lab".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "science_multi".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();

    // Artificially fill downlink quota but only one station
    if let Some(c) = sim.company.active_contract.as_mut() {
        c.downlink_mb_done = c.downlink_mb_required;
        c.stations_used = vec!["svalbard".into()];
    }
    sim.check_contract();
    assert!(
        sim.company.active_contract.is_some(),
        "should not complete with only 1 station"
    );

    if let Some(c) = sim.company.active_contract.as_mut() {
        c.stations_used = vec!["svalbard".into(), "fairbanks".into(), "kiruna".into()];
    }
    sim.check_contract();
    assert!(
        sim.company
            .completed_contracts
            .contains(&"science_multi".to_string()),
        "should complete with 3 stations"
    );
}

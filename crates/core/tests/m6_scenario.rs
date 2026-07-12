use satelliteinc_core::{SimState, action::PlayerAction, company::GameMode};

mod common;
use common::load_content;

#[test]
fn load_scenario_sets_mode_and_contract() {
    let content = load_content();
    let mut sim = SimState::new(content).unwrap();

    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "ops_challenge".into(),
    })
    .unwrap();

    assert_eq!(sim.company.game_mode, GameMode::Campaign);
    assert_eq!(sim.company.reputation, 20.0);
    assert_eq!(
        sim.company
            .active_contract
            .as_ref()
            .map(|c| c.template_id.as_str()),
        Some("comms_relay")
    );
    assert_eq!(sim.spacecraft.as_ref().unwrap().bus_id, "microsat");
}

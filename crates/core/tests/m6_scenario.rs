use satelliteinc_core::{SimState, action::PlayerAction, company::GameMode};

mod common;
use common::load_content;

#[test]
fn load_scenario_sets_mode_and_contract() {
    let content = load_content();
    let mut sim = SimState::new(content).unwrap();

    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "campaign_start".into(),
    })
    .unwrap();

    assert_eq!(sim.company.game_mode, GameMode::Campaign);
    assert_eq!(sim.company.reputation, 20.0);
    assert_eq!(
        sim.company
            .active_contracts
            .first()
            .as_ref()
            .map(|c| c.template_id.as_str()),
        Some("earth_imaging")
    );
    assert_eq!(
        sim.company.active_contracts[0].assigned_craft_id,
        Some(0),
        "the seeded contract services the scenario craft"
    );
    assert_eq!(common::only_craft(&sim).bus_id, "cubesat");
}

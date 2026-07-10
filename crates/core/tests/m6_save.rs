use satelliteinc_core::{action::PlayerAction, SaveGame, SimState};

mod common;
use common::load_content;

#[test]
fn save_round_trip_preserves_state() {
    let content = load_content();
    let mut sim = SimState::new(content);

    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "campaign_start".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 250.0 })
        .unwrap();
    sim.step(1.0);

    let save = sim.export_save();
    let json = save.to_json().unwrap();
    let restored =
        SimState::import_save(sim.content.clone(), SaveGame::from_json(&json).unwrap()).unwrap();

    assert!(
        (restored.clock.sim_seconds - sim.clock.sim_seconds).abs() < 1e-9,
        "sim_seconds must survive the JSON round trip"
    );
    assert!(
        (restored.company.cash_usd - sim.company.cash_usd).abs() < 1e-6,
        "cash must survive the JSON round trip"
    );
    assert!(restored.spacecraft.is_some());
}

#[test]
fn golden_action_replay_deterministic() {
    let content = load_content();
    let mut sim = SimState::new(content.clone());

    let actions = [
        PlayerAction::LoadScenario {
            scenario_id: "campaign_start".into(),
        },
        PlayerAction::Launch,
        PlayerAction::SetTimeAccel { multiplier: 1000.0 },
    ];
    for action in actions {
        sim.apply_action(action).unwrap();
    }
    for _ in 0..500 {
        sim.step(0.016);
    }
    let cash_a = sim.company.cash_usd;
    let days_a = sim.clock.sim_days();

    let mut sim2 = SimState::new(content);
    for action in [
        PlayerAction::LoadScenario {
            scenario_id: "campaign_start".into(),
        },
        PlayerAction::Launch,
        PlayerAction::SetTimeAccel { multiplier: 1000.0 },
    ] {
        sim2.apply_action(action).unwrap();
    }
    for _ in 0..500 {
        sim2.step(0.016);
    }

    assert_eq!(cash_a, sim2.company.cash_usd);
    assert!((days_a - sim2.clock.sim_days()).abs() < 1e-6);
}

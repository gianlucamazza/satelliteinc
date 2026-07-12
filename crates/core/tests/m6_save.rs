use satelliteinc_core::{SaveGame, SimState, action::PlayerAction};

mod common;
use common::load_content;

#[test]
fn save_round_trip_preserves_state() {
    let content = load_content();
    let mut sim = SimState::new(content).unwrap();

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
    assert_eq!(restored.company.outcome, sim.company.outcome);
    assert_eq!(restored.company.isp_multiplier, sim.company.isp_multiplier);
    assert_eq!(
        restored.company.anomaly_mtbf_multiplier,
        sim.company.anomaly_mtbf_multiplier
    );
}

/// A v2 save (pre-solar-system: no `version: 3` fields, no `body_id` on the
/// spacecraft) must import cleanly with `body_id` defaulting to "earth".
#[test]
fn v2_save_migrates_to_v3() {
    let content = load_content();
    let mut sim = SimState::new(content.clone()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "campaign_start".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();
    sim.step(1.0);

    // Reconstruct the exact v2 shape from a v3 export: version stamp back
    // to 2 and the body_id field (new in v3) removed.
    let mut save_json: serde_json::Value =
        serde_json::from_str(&sim.export_save().to_json().unwrap()).unwrap();
    save_json["version"] = serde_json::json!(2);
    let removed = save_json["spacecraft"]
        .as_object_mut()
        .expect("craft present")
        .remove("body_id");
    assert!(removed.is_some(), "v3 exports must carry body_id");

    let restored = SimState::import_save(
        content,
        SaveGame::from_json(&save_json.to_string()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.spacecraft.as_ref().unwrap().body_id, "earth");
    assert!((restored.clock.sim_seconds - sim.clock.sim_seconds).abs() < 1e-9);
}

/// Saves referencing bodies the content no longer defines are rejected
/// instead of being silently relocated.
#[test]
fn save_with_unknown_body_is_rejected() {
    let content = load_content();
    let mut sim = SimState::new(content.clone()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "campaign_start".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();
    let mut save = sim.export_save();
    save.spacecraft.as_mut().unwrap().body_id = "krypton".into();
    assert!(SimState::import_save(content, save).is_err());
}

#[test]
fn golden_action_replay_deterministic() {
    let content = load_content();
    let mut sim = SimState::new(content.clone()).unwrap();

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

    let mut sim2 = SimState::new(content).unwrap();
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

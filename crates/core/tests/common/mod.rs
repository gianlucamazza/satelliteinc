use satelliteinc_core::content::GameContent;

pub fn load_content() -> GameContent {
    let json = include_str!("../../../../content/game.json");
    GameContent::from_json(json).expect("valid content")
}

// ---- Fleet-era helpers: most tests still drive a single craft. ----

use satelliteinc_core::SimState;
use satelliteinc_core::company::ActiveContract;
use satelliteinc_core::snapshot::SpacecraftSnapshot;
use satelliteinc_core::spacecraft::{CraftId, Spacecraft};

/// The scenario/first craft's id.
#[allow(dead_code)]
pub const CRAFT0: CraftId = 0;

#[allow(dead_code)]
pub fn only_craft(sim: &SimState) -> &Spacecraft {
    assert_eq!(sim.fleet.len(), 1, "expected exactly one craft");
    &sim.fleet[0]
}

#[allow(dead_code)]
pub fn only_craft_mut(sim: &mut SimState) -> &mut Spacecraft {
    assert_eq!(sim.fleet.len(), 1, "expected exactly one craft");
    &mut sim.fleet[0]
}

#[allow(dead_code)]
pub fn only_contract(sim: &SimState) -> &ActiveContract {
    assert_eq!(
        sim.company.active_contracts.len(),
        1,
        "expected exactly one active contract"
    );
    &sim.company.active_contracts[0]
}

#[allow(dead_code)]
pub fn only_contract_mut(sim: &mut SimState) -> &mut ActiveContract {
    assert_eq!(
        sim.company.active_contracts.len(),
        1,
        "expected exactly one active contract"
    );
    &mut sim.company.active_contracts[0]
}

/// Owned snapshot of the only craft (convenient for one-liners).
#[allow(dead_code)]
pub fn snap_craft(sim: &SimState) -> SpacecraftSnapshot {
    let snap = sim.snapshot();
    assert_eq!(snap.spacecraft.len(), 1, "expected exactly one craft");
    snap.spacecraft.into_iter().next().unwrap()
}

/// Sandbox staged at any body: the unified replacement for the old
/// per-body sandbox scenarios (sandbox_luna/mars/venus).
#[allow(dead_code)]
pub fn sandbox_at(body_id: &str, altitude_km: f64, inclination_deg: f64) -> SimState {
    use satelliteinc_core::action::PlayerAction;
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "sandbox_lab".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "explorer".into(),
    })
    .unwrap();
    let craft_id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::SetDesignBody {
        craft_id,
        body_id: body_id.into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetOrbit {
        craft_id,
        altitude_km,
        inclination_deg,
    })
    .unwrap();
    sim
}

/// Campaign staged for interplanetary work: explorer in LEO, deep-space
/// certification, the target program bought in — the state the old
/// lunar_express/mars_window tutorial scenarios used to seed (they are
/// in-campaign guided missions now).
#[allow(dead_code)]
pub fn interplanetary_campaign(programs: &[&str], cash: f64, reputation: f64) -> SimState {
    use satelliteinc_core::action::PlayerAction;
    // Fresh campaign company (no scenario): SelectBus mints craft id 0.
    let mut sim = SimState::new(load_content()).unwrap();
    sim.company.cash_usd = cash;
    sim.company.reputation = reputation;
    for tech_id in ["comms_mk2", "smallsat_bus", "deep_space_ops"] {
        let tech = sim.content.tech(tech_id).unwrap().clone();
        sim.company.apply_tech(&tech);
    }
    for program in programs {
        sim.company.unlocked_programs.push((*program).to_string());
    }
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "explorer".into(),
    })
    .unwrap();
    let craft_id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::SetOrbit {
        craft_id,
        altitude_km: 300.0,
        inclination_deg: 28.0,
    })
    .unwrap();
    sim
}

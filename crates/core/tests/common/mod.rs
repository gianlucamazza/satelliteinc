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

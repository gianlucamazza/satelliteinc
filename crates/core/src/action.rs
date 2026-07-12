use serde::{Deserialize, Serialize};

use crate::ops::CommandType;
use crate::spacecraft::{CraftId, PayloadMode};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlayerAction {
    SelectBus {
        bus_id: String,
    },
    SetOrbit {
        craft_id: CraftId,
        altitude_km: f64,
        inclination_deg: f64,
    },
    AcceptContract {
        contract_id: String,
        /// Craft that will serve the contract (assignment is 1:1).
        #[serde(default)]
        assigned_craft_id: Option<CraftId>,
    },
    Launch {
        craft_id: CraftId,
    },
    SetDownlink {
        craft_id: CraftId,
        active: bool,
    },
    SetTimeAccel {
        multiplier: f64,
    },
    ProgradeBurn {
        craft_id: CraftId,
        delta_v_km_s: f64,
    },
    RetrogradeBurn {
        craft_id: CraftId,
        delta_v_km_s: f64,
    },
    SetManeuverTarget {
        craft_id: CraftId,
        target_altitude_km: f64,
    },
    StartHohmann {
        craft_id: CraftId,
    },
    CompleteHohmann {
        craft_id: CraftId,
    },
    /// Retire the on-orbit satellite for a partial salvage refund
    /// (forfeited without salvage during a transfer).
    Decommission {
        craft_id: CraftId,
    },
    /// Choose the interplanetary destination and capture orbit.
    SetTransferTarget {
        craft_id: CraftId,
        body_id: String,
        capture_altitude_km: f64,
        capture_inclination_deg: f64,
    },
    /// Buy the injection and depart (window must be open).
    BeginTransfer {
        craft_id: CraftId,
    },
    /// (Re)assign or unassign an active contract to a craft.
    AssignContract {
        contract_id: String,
        craft_id: Option<CraftId>,
    },
    /// Buy into a body's Program (reputation + keystone gated).
    UnlockProgram {
        program_id: String,
    },
    /// Advance sim time in one deterministic stride (chunked at ~2e6 s per
    /// call; re-issue until the target time is reached).
    WarpTo {
        target_sim_seconds: f64,
    },
    SetPayloadMode {
        craft_id: CraftId,
        mode: PayloadMode,
    },
    SelectDownlinkStation {
        craft_id: CraftId,
        station_id: String,
    },
    QueueCommand {
        craft_id: CraftId,
        command: CommandType,
        station_id: String,
    },
    PurchaseTech {
        tech_id: String,
    },
    SetGameMode {
        mode: String,
    },
    LoadScenario {
        scenario_id: String,
    },
}

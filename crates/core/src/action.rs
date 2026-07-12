use serde::{Deserialize, Serialize};

use crate::ops::CommandType;
use crate::spacecraft::PayloadMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlayerAction {
    SelectBus {
        bus_id: String,
    },
    SetOrbit {
        altitude_km: f64,
        inclination_deg: f64,
    },
    AcceptContract {
        contract_id: String,
    },
    Launch,
    SetDownlink {
        active: bool,
    },
    SetTimeAccel {
        multiplier: f64,
    },
    ProgradeBurn {
        delta_v_km_s: f64,
    },
    RetrogradeBurn {
        delta_v_km_s: f64,
    },
    SetManeuverTarget {
        target_altitude_km: f64,
    },
    StartHohmann,
    CompleteHohmann,
    /// Retire the on-orbit satellite for a partial salvage refund
    /// (forfeited without salvage during a transfer).
    Decommission,
    /// Choose the interplanetary destination and capture orbit.
    SetTransferTarget {
        body_id: String,
        capture_altitude_km: f64,
        capture_inclination_deg: f64,
    },
    /// Buy the injection and depart (window must be open).
    BeginTransfer,
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
        mode: PayloadMode,
    },
    SelectDownlinkStation {
        station_id: String,
    },
    QueueCommand {
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

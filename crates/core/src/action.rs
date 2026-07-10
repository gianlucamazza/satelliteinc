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

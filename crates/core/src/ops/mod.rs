mod anomaly;
mod command;
mod visibility;

pub use anomaly::{roll_anomaly, ActiveAnomaly, AnomalyType, ANOMALY_COOLDOWN_SECONDS};
pub use command::{execute_command, CommandType, QueuedCommand};
pub use visibility::{elevation_deg, geodetic_to_ecef_km, is_in_pass, seconds_until_pass};

use serde::{Deserialize, Serialize};

use crate::constants::EARTH_ROTATION_RAD_S;
use crate::content::GroundStationDef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundStation {
    pub id: String,
    pub name: String,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub min_elevation_deg: f64,
    pub bandwidth_mbps: f64,
}

impl From<&GroundStationDef> for GroundStation {
    fn from(def: &GroundStationDef) -> Self {
        Self {
            id: def.id.clone(),
            name: def.name.clone(),
            latitude_deg: def.latitude_deg,
            longitude_deg: def.longitude_deg,
            min_elevation_deg: def.min_elevation_deg,
            bandwidth_mbps: def.bandwidth_mbps,
        }
    }
}

/// Convert ECI position (km) to ECEF (km) using simplified Earth rotation.
pub fn eci_to_ecef_km(eci: [f64; 3], sim_seconds: f64) -> [f64; 3] {
    let theta = EARTH_ROTATION_RAD_S * sim_seconds;
    let (st, ct) = theta.sin_cos();
    [
        ct * eci[0] + st * eci[1],
        -st * eci[0] + ct * eci[1],
        eci[2],
    ]
}

/// Whether the satellite is in sunlight (simplified: above Earth's shadow cone).
///
/// The sun is intentionally fixed along ECI +X: the shadow cylinder never
/// moves with sim time. This keeps eclipse timing deterministic and cheap;
/// replace with a time-advancing sun vector if seasonal/diurnal variation
/// ever matters for gameplay.
pub fn in_sunlight(eci_km: [f64; 3]) -> bool {
    // Simplified cylindrical shadow along -X (sun direction approximation)
    let r = (eci_km[1].powi(2) + eci_km[2].powi(2)).sqrt();
    !(eci_km[0] < 0.0 && r < crate::constants::EARTH_RADIUS_KM)
}

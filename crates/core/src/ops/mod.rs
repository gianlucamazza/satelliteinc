mod anomaly;
mod command;
mod visibility;

pub use anomaly::{
    ANOMALY_COOLDOWN_SECONDS, ANOMALY_PHASE_STRIDE_SECONDS, ActiveAnomaly, AnomalyType,
    AnomalyWeights, roll_anomaly, roll_anomaly_weighted,
};
pub use command::{CommandType, QueuedCommand, execute_command};
pub use visibility::{elevation_deg, geodetic_to_ecef_km, is_in_pass, seconds_until_pass};

use serde::{Deserialize, Serialize};

use crate::content::GroundStationDef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundStation {
    pub id: String,
    pub name: String,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub min_elevation_deg: f64,
    pub bandwidth_mbps: f64,
    pub body_id: String,
    pub dsn: bool,
    /// Campaign tech gate (`None` = always available).
    pub requires_tech: Option<String>,
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
            body_id: def.body_id.clone(),
            dsn: def.dsn,
            requires_tech: def.requires_tech.clone(),
        }
    }
}

/// Convert body-centered-inertial position (km) to body-fixed (km) for a
/// body rotating at `rotation_rad_s` (rotation angle zero at sim t = 0).
#[must_use]
pub fn eci_to_ecef_km(rotation_rad_s: f64, eci: [f64; 3], sim_seconds: f64) -> [f64; 3] {
    let theta = rotation_rad_s * sim_seconds;
    let (st, ct) = theta.sin_cos();
    [
        ct * eci[0] + st * eci[1],
        -st * eci[0] + ct * eci[1],
        eci[2],
    ]
}

/// Whether the satellite is in sunlight: cylindrical shadow of a body of
/// radius `body_radius_km`, cast opposite the unit sun direction.
#[must_use]
pub fn in_sunlight(eci_km: [f64; 3], sun_dir_unit: [f64; 3], body_radius_km: f64) -> bool {
    let s = eci_km[0] * sun_dir_unit[0] + eci_km[1] * sun_dir_unit[1] + eci_km[2] * sun_dir_unit[2];
    let perp = [
        eci_km[0] - s * sun_dir_unit[0],
        eci_km[1] - s * sun_dir_unit[1],
        eci_km[2] - s * sun_dir_unit[2],
    ];
    let r = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
    !(s < 0.0 && r < body_radius_km)
}

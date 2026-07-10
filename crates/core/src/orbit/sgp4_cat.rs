use serde::{Deserialize, Serialize};

use crate::error::SimError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TleEntry {
    pub norad_id: u32,
    pub name: String,
    pub line1: String,
    pub line2: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceSatellite {
    pub norad_id: u32,
    pub name: String,
    pub position_eci_km: [f64; 3],
    pub velocity_eci_km_s: [f64; 3],
    pub altitude_km: f64,
}

#[cfg(feature = "sgp4")]
mod imp {
    use super::*;
    use sgp4::{parse_3les, Constants, MinutesSinceEpoch};

    use crate::constants::EARTH_RADIUS_KM;

    pub struct TlePropagator {
        pub norad_id: u32,
        pub name: String,
        constants: Constants,
    }

    impl TlePropagator {
        pub fn from_entry(entry: &TleEntry) -> Result<Self, SimError> {
            let tle_text = format!("{}\n{}\n{}", entry.name, entry.line1, entry.line2);
            let elements = parse_3les(&tle_text)
                .map_err(|e| SimError::Content(format!("TLE parse error: {e}")))?
                .into_iter()
                .next()
                .ok_or_else(|| SimError::Content("empty TLE".into()))?;
            let constants = Constants::from_elements(&elements)
                .map_err(|e| SimError::Content(format!("SGP4 init: {e}")))?;
            Ok(Self {
                norad_id: entry.norad_id,
                name: entry.name.clone(),
                constants,
            })
        }

        pub fn propagate(&self, sim_seconds: f64) -> Result<ReferenceSatellite, SimError> {
            // Sim time 0 is anchored at the TLE's own epoch.
            let pred = self
                .constants
                .propagate(MinutesSinceEpoch(sim_seconds / 60.0))
                .map_err(|e| SimError::Content(format!("SGP4 propagate: {e}")))?;
            // SGP4 TEME km / km/s — treat as ECI for visualization
            let pos = pred.position;
            let vel = pred.velocity;
            let r = (pos[0] * pos[0] + pos[1] * pos[1] + pos[2] * pos[2]).sqrt();
            Ok(ReferenceSatellite {
                norad_id: self.norad_id,
                name: self.name.clone(),
                position_eci_km: [pos[0], pos[1], pos[2]],
                velocity_eci_km_s: [vel[0], vel[1], vel[2]],
                altitude_km: r - EARTH_RADIUS_KM,
            })
        }
    }
}

#[cfg(feature = "sgp4")]
pub use imp::TlePropagator;

#[cfg(not(feature = "sgp4"))]
pub struct TlePropagator;

#[cfg(not(feature = "sgp4"))]
impl TlePropagator {
    pub fn from_entry(_entry: &TleEntry) -> Result<Self, SimError> {
        Err(SimError::Content("SGP4 feature disabled".into()))
    }
    pub fn propagate(&self, _sim_seconds: f64) -> Result<ReferenceSatellite, SimError> {
        Err(SimError::Content("SGP4 feature disabled".into()))
    }
}

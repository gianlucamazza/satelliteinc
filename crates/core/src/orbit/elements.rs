use serde::{Deserialize, Serialize};

/// Classical Keplerian orbital elements (f64 throughout).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct KeplerElements {
    pub semi_major_axis_km: f64,
    pub eccentricity: f64,
    pub inclination_deg: f64,
    pub raan_deg: f64,
    pub arg_perigee_deg: f64,
    pub true_anomaly_deg: f64,
    pub epoch_sim_seconds: f64,
}

impl KeplerElements {
    pub fn altitude_km(&self) -> f64 {
        self.semi_major_axis_km - crate::constants::EARTH_RADIUS_KM
    }
}

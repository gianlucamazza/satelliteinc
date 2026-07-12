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
    /// Mean altitude above a body of the given radius.
    #[must_use]
    pub fn altitude_km(&self, body_radius_km: f64) -> f64 {
        self.semi_major_axis_km - body_radius_km
    }
}

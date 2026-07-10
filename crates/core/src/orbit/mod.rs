mod elements;
mod maneuver;
mod propagate;
mod sgp4_cat;
mod state_vectors;

pub use sgp4_cat::{ReferenceSatellite, TleEntry, TlePropagator};

pub use elements::KeplerElements;
pub use maneuver::{
    apply_impulsive_burn, delta_v_capacity_km_s, hohmann_plan, propellant_for_delta_v,
    BurnDirection, HohmannPlan,
};
pub use propagate::{
    eci_position_km, eci_velocity_km_s, orbit_period_seconds, orbit_trail_samples,
};
pub use state_vectors::{at_apogee, at_apsis, at_perigee, elements_from_pv};

use crate::constants::{EARTH_RADIUS_KM, MU_EARTH};

/// Build a circular LEO orbit from altitude and inclination.
pub fn circular_leo(
    altitude_km: f64,
    inclination_deg: f64,
    epoch_sim_seconds: f64,
) -> KeplerElements {
    let semi_major_axis_km = EARTH_RADIUS_KM + altitude_km;
    KeplerElements {
        semi_major_axis_km,
        eccentricity: 0.0,
        inclination_deg,
        raan_deg: 0.0,
        arg_perigee_deg: 0.0,
        true_anomaly_deg: 0.0,
        epoch_sim_seconds,
    }
}

/// Mean motion (rad/s) for a given semi-major axis.
pub fn mean_motion_rad_s(semi_major_axis_km: f64) -> f64 {
    (MU_EARTH / semi_major_axis_km.powi(3)).sqrt()
}

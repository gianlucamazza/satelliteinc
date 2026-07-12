mod elements;
mod maneuver;
mod propagate;
mod sgp4_cat;
mod state_vectors;
mod transfer;

pub use sgp4_cat::{ReferenceSatellite, TleEntry, TlePropagator};

pub use elements::KeplerElements;
pub use maneuver::{
    BurnDirection, HohmannPlan, apply_impulsive_burn, delta_v_capacity_km_s, hohmann_plan,
    propellant_for_delta_v,
};
pub use propagate::{
    eci_position_km, eci_velocity_km_s, mean_to_true_anomaly_deg, orbit_period_seconds,
    orbit_trail_samples, seconds_to_apsides, true_to_mean_anomaly_deg,
};
pub use state_vectors::{at_apogee, at_apsis, at_perigee, elements_from_pv};
pub use transfer::{
    TransferPlan, WINDOW_TOLERANCE_DEG, next_launch_window, phase_angle_deg, plan_transfer,
    required_phase_angle_deg, window_open,
};

/// Build a circular orbit from altitude above a body of the given radius.
#[must_use]
pub fn circular_orbit(
    body_radius_km: f64,
    altitude_km: f64,
    inclination_deg: f64,
    epoch_sim_seconds: f64,
) -> KeplerElements {
    let semi_major_axis_km = body_radius_km + altitude_km;
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

/// Mean motion (rad/s) for a given semi-major axis around `mu_km3_s2`.
#[must_use]
pub fn mean_motion_rad_s(mu_km3_s2: f64, semi_major_axis_km: f64) -> f64 {
    (mu_km3_s2 / semi_major_axis_km.powi(3)).sqrt()
}

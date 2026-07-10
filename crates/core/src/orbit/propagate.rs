use std::f64::consts::PI;

use crate::constants::MU_EARTH;
use crate::orbit::{mean_motion_rad_s, KeplerElements};

/// Orbital period in seconds (Kepler's third law).
pub fn orbit_period_seconds(semi_major_axis_km: f64) -> f64 {
    2.0 * PI * (semi_major_axis_km.powi(3) / MU_EARTH).sqrt()
}

/// Propagate Kepler elements to ECI position (km) at `sim_seconds`.
pub fn eci_position_km(elements: &KeplerElements, sim_seconds: f64) -> [f64; 3] {
    let dt = sim_seconds - elements.epoch_sim_seconds;
    let n = mean_motion_rad_s(elements.semi_major_axis_km);
    let m0 = true_anomaly_to_mean(
        elements.true_anomaly_deg.to_radians(),
        elements.eccentricity,
    );
    let m = m0 + n * dt;
    let e = solve_kepler(m, elements.eccentricity);
    let nu = eccentric_to_true_anomaly(e, elements.eccentricity);

    let a = elements.semi_major_axis_km;
    let e = elements.eccentricity;
    let r = a * (1.0 - e * e) / (1.0 + e * nu.cos());

    // Perifocal (PQW) position
    let p = [r * nu.cos(), r * nu.sin(), 0.0];

    rotate_pqw_to_eci(
        p,
        elements.inclination_deg.to_radians(),
        elements.raan_deg.to_radians(),
        elements.arg_perigee_deg.to_radians(),
    )
}

/// Propagate to ECI velocity (km/s) at `sim_seconds`.
pub fn eci_velocity_km_s(elements: &KeplerElements, sim_seconds: f64) -> [f64; 3] {
    let dt = sim_seconds - elements.epoch_sim_seconds;
    let n = mean_motion_rad_s(elements.semi_major_axis_km);
    let m0 = true_anomaly_to_mean(
        elements.true_anomaly_deg.to_radians(),
        elements.eccentricity,
    );
    let m = m0 + n * dt;
    let ea = solve_kepler(m, elements.eccentricity);
    let nu = eccentric_to_true_anomaly(ea, elements.eccentricity);

    let a = elements.semi_major_axis_km;
    let e = elements.eccentricity;
    let p = a * (1.0 - e * e);
    let r = p / (1.0 + e * nu.cos());

    let sqrt_mu_p = MU_EARTH.sqrt() / p.sqrt();
    let v_pqw = [-sqrt_mu_p * nu.sin(), sqrt_mu_p * (e + nu.cos()), 0.0];

    // Scale by r for the actual magnitude relationship in perifocal frame
    let _ = r; // r used implicitly via p

    rotate_pqw_to_eci(
        v_pqw,
        elements.inclination_deg.to_radians(),
        elements.raan_deg.to_radians(),
        elements.arg_perigee_deg.to_radians(),
    )
}

/// Sample orbit trail positions for rendering (parent-relative km).
pub fn orbit_trail_samples(
    elements: &KeplerElements,
    sim_seconds: f64,
    count: usize,
) -> Vec<[f64; 3]> {
    let period = orbit_period_seconds(elements.semi_major_axis_km);
    (0..count)
        .map(|i| {
            let t = sim_seconds + (i as f64 / count as f64) * period;
            eci_position_km(elements, t)
        })
        .collect()
}

fn true_anomaly_to_mean(nu: f64, e: f64) -> f64 {
    if e < 1e-10 {
        return nu;
    }
    let ea = 2.0 * ((1.0 - e).sqrt() * (nu / 2.0).tan()).atan2((1.0 + e).sqrt());
    ea - e * ea.sin()
}

fn solve_kepler(m: f64, e: f64) -> f64 {
    if e < 1e-10 {
        return m;
    }
    let m = m.rem_euclid(2.0 * PI);
    let mut ea = if e < 0.8 { m } else { PI };
    for _ in 0..30 {
        let f = ea - e * ea.sin() - m;
        let fp = 1.0 - e * ea.cos();
        let delta = f / fp;
        ea -= delta;
        if delta.abs() < 1e-12 {
            break;
        }
    }
    ea
}

fn eccentric_to_true_anomaly(ea: f64, e: f64) -> f64 {
    if e < 1e-10 {
        return ea;
    }
    let sin_e = ea.sin();
    let cos_e = ea.cos();
    let sin_nu = (1.0 - e * e).sqrt() * sin_e / (1.0 - e * cos_e);
    let cos_nu = (cos_e - e) / (1.0 - e * cos_e);
    sin_nu.atan2(cos_nu)
}

fn rotate_pqw_to_eci(v: [f64; 3], i: f64, raan: f64, argp: f64) -> [f64; 3] {
    let (si, ci) = i.sin_cos();
    let (so, co) = raan.sin_cos();
    let (sw, cw) = argp.sin_cos();

    // R = R3(-Ω) * R1(-i) * R3(-ω)
    let r11 = co * cw - so * sw * ci;
    let r12 = -co * sw - so * cw * ci;
    let r13 = so * si;
    let r21 = so * cw + co * sw * ci;
    let r22 = -so * sw + co * cw * ci;
    let r23 = -co * si;
    let r31 = sw * si;
    let r32 = cw * si;
    let r33 = ci;

    [
        r11 * v[0] + r12 * v[1] + r13 * v[2],
        r21 * v[0] + r22 * v[1] + r23 * v[2],
        r31 * v[0] + r32 * v[1] + r33 * v[2],
    ]
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;

    use super::*;
    use crate::orbit::circular_leo;

    #[test]
    fn period_at_400km_matches_analytical() {
        let alt = 400.0;
        let a = crate::constants::EARTH_RADIUS_KM + alt;
        let period = orbit_period_seconds(a);
        // ~92.4 minutes
        assert_relative_eq!(period, 5_544.0, epsilon = 30.0);
    }

    #[test]
    fn circular_orbit_radius_stable_over_one_period() {
        let elements = circular_leo(400.0, 51.6, 0.0);
        let period = orbit_period_seconds(elements.semi_major_axis_km);
        let r0 = eci_position_km(&elements, 0.0);
        let r1 = eci_position_km(&elements, period);
        let mag0 = (r0[0].powi(2) + r0[1].powi(2) + r0[2].powi(2)).sqrt();
        let mag1 = (r1[0].powi(2) + r1[1].powi(2) + r1[2].powi(2)).sqrt();
        assert_relative_eq!(mag0, mag1, epsilon = 0.01);
        assert_relative_eq!(mag0, elements.semi_major_axis_km, epsilon = 0.01);
    }
}

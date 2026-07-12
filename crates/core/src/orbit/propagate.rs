use std::f64::consts::PI;

use crate::orbit::{KeplerElements, mean_motion_rad_s};

/// Orbital period in seconds (Kepler's third law) around a body with
/// gravitational parameter `mu_km3_s2`.
#[must_use]
pub fn orbit_period_seconds(mu_km3_s2: f64, semi_major_axis_km: f64) -> f64 {
    2.0 * PI * (semi_major_axis_km.powi(3) / mu_km3_s2).sqrt()
}

/// Propagate Kepler elements to body-centered-inertial position (km) at `sim_seconds`.
#[must_use]
pub fn eci_position_km(mu_km3_s2: f64, elements: &KeplerElements, sim_seconds: f64) -> [f64; 3] {
    let dt = sim_seconds - elements.epoch_sim_seconds;
    let n = mean_motion_rad_s(mu_km3_s2, elements.semi_major_axis_km);
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

/// Propagate to body-centered-inertial velocity (km/s) at `sim_seconds`.
#[must_use]
pub fn eci_velocity_km_s(mu_km3_s2: f64, elements: &KeplerElements, sim_seconds: f64) -> [f64; 3] {
    let dt = sim_seconds - elements.epoch_sim_seconds;
    let n = mean_motion_rad_s(mu_km3_s2, elements.semi_major_axis_km);
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

    let sqrt_mu_p = mu_km3_s2.sqrt() / p.sqrt();
    let v_pqw = [-sqrt_mu_p * nu.sin(), sqrt_mu_p * (e + nu.cos()), 0.0];

    rotate_pqw_to_eci(
        v_pqw,
        elements.inclination_deg.to_radians(),
        elements.raan_deg.to_radians(),
        elements.arg_perigee_deg.to_radians(),
    )
}

/// Seconds until the next perigee and apogee passages at `sim_seconds`.
/// Returns (`to_perigee`, `to_apogee`).
#[must_use]
pub fn seconds_to_apsides(
    mu_km3_s2: f64,
    elements: &KeplerElements,
    sim_seconds: f64,
) -> (f64, f64) {
    let two_pi = 2.0 * PI;
    let n = mean_motion_rad_s(mu_km3_s2, elements.semi_major_axis_km);
    let dt = sim_seconds - elements.epoch_sim_seconds;
    let m0 = true_anomaly_to_mean(
        elements.true_anomaly_deg.to_radians(),
        elements.eccentricity,
    );
    let m = (m0 + n * dt).rem_euclid(two_pi);
    // Perigee is at mean anomaly 0, apogee at pi.
    let to_perigee = ((two_pi - m) % two_pi) / n;
    let to_apogee = (PI - m).rem_euclid(two_pi) / n;
    (to_perigee, to_apogee)
}

/// Sample orbit trail positions for rendering (parent-relative km).
#[must_use]
pub fn orbit_trail_samples(
    mu_km3_s2: f64,
    elements: &KeplerElements,
    sim_seconds: f64,
    count: usize,
) -> Vec<[f64; 3]> {
    let period = orbit_period_seconds(mu_km3_s2, elements.semi_major_axis_km);
    (0..count)
        .map(|i| {
            let t = sim_seconds + (i as f64 / count as f64) * period;
            eci_position_km(mu_km3_s2, elements, t)
        })
        .collect()
}

/// Mean anomaly (deg) to true anomaly (deg), via the same Kepler solver the
/// propagator uses. Needed to turn content body ephemerides (tabulated as
/// mean anomaly at epoch) into `KeplerElements`.
#[must_use]
pub fn mean_to_true_anomaly_deg(mean_anomaly_deg: f64, eccentricity: f64) -> f64 {
    let ea = solve_kepler(mean_anomaly_deg.to_radians(), eccentricity);
    eccentric_to_true_anomaly(ea, eccentricity).to_degrees()
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
    use crate::constants::MU_EARTH;
    use crate::orbit::circular_orbit;

    #[test]
    fn period_at_400km_matches_analytical() {
        let alt = 400.0;
        let a = crate::constants::EARTH_RADIUS_KM + alt;
        let period = orbit_period_seconds(MU_EARTH, a);
        // ~92.4 minutes
        assert_relative_eq!(period, 5_544.0, epsilon = 30.0);
    }

    #[test]
    fn apsides_eta_matches_orbit_geometry() {
        // Elliptical orbit starting at perigee (true anomaly 0).
        let mut elements = circular_orbit(crate::constants::EARTH_RADIUS_KM, 400.0, 51.6, 0.0);
        elements.eccentricity = 0.1;
        let period = orbit_period_seconds(MU_EARTH, elements.semi_major_axis_km);

        let (to_perigee, to_apogee) = seconds_to_apsides(MU_EARTH, &elements, 0.0);
        // Sitting exactly at perigee: perigee ETA 0, apogee half a period out.
        assert_relative_eq!(to_perigee, 0.0, epsilon = 1.0);
        assert_relative_eq!(to_apogee, period / 2.0, epsilon = 1.0);

        // A quarter period later both ETAs shrink by the same amount.
        let (p2, a2) = seconds_to_apsides(MU_EARTH, &elements, period / 4.0);
        assert_relative_eq!(p2, period * 0.75, epsilon = 1.0);
        assert_relative_eq!(a2, period / 4.0, epsilon = 1.0);
    }

    #[test]
    fn circular_orbit_radius_stable_over_one_period() {
        let elements = circular_orbit(crate::constants::EARTH_RADIUS_KM, 400.0, 51.6, 0.0);
        let period = orbit_period_seconds(MU_EARTH, elements.semi_major_axis_km);
        let r0 = eci_position_km(MU_EARTH, &elements, 0.0);
        let r1 = eci_position_km(MU_EARTH, &elements, period);
        let mag0 = (r0[0].powi(2) + r0[1].powi(2) + r0[2].powi(2)).sqrt();
        let mag1 = (r1[0].powi(2) + r1[1].powi(2) + r1[2].powi(2)).sqrt();
        assert_relative_eq!(mag0, mag1, epsilon = 0.01);
        assert_relative_eq!(mag0, elements.semi_major_axis_km, epsilon = 0.01);
    }

    #[test]
    fn mean_to_true_round_trips_through_mean() {
        let nu = mean_to_true_anomaly_deg(75.201_271_558_281_72, 0.016_711_23);
        let m_back = true_anomaly_to_mean(nu.to_radians(), 0.016_711_23).to_degrees();
        assert_relative_eq!(m_back, 75.201_271_558_281_72, epsilon = 1e-9);
    }
}

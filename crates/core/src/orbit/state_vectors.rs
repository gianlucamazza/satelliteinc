use crate::constants::MU_EARTH;
use crate::error::SimError;
use crate::orbit::KeplerElements;

/// Convert ECI state vector to Keplerian elements at `epoch_sim_seconds`.
pub fn elements_from_pv(
    position_km: [f64; 3],
    velocity_km_s: [f64; 3],
    epoch_sim_seconds: f64,
) -> Result<KeplerElements, SimError> {
    let r = position_km;
    let v = velocity_km_s;
    let r_mag = norm(r);
    let v_mag = norm(v);
    if r_mag < 1.0 {
        return Err(SimError::InvalidAction("degenerate position".into()));
    }

    let h = cross(r, v);
    let h_mag = norm(h);
    if h_mag < 1e-6 {
        return Err(SimError::InvalidAction("degenerate orbit".into()));
    }

    let rv = dot(r, v);
    let e_vec = [
        ((v_mag * v_mag - MU_EARTH / r_mag) * r[0] - rv * v[0]) / MU_EARTH,
        ((v_mag * v_mag - MU_EARTH / r_mag) * r[1] - rv * v[1]) / MU_EARTH,
        ((v_mag * v_mag - MU_EARTH / r_mag) * r[2] - rv * v[2]) / MU_EARTH,
    ];
    let e = norm(e_vec);

    let energy = 0.5 * v_mag * v_mag - MU_EARTH / r_mag;
    if energy.abs() < 1e-12 {
        return Err(SimError::InvalidAction(
            "parabolic orbit not supported".into(),
        ));
    }
    let semi_major_axis_km = -MU_EARTH / (2.0 * energy);
    if semi_major_axis_km <= 0.0 {
        return Err(SimError::InvalidAction(
            "hyperbolic orbit not supported".into(),
        ));
    }

    let inclination_deg = (h[2] / h_mag).acos().to_degrees();

    // Node vector n = k × h
    let n_vec = [-h[1], h[0], 0.0];
    let n_mag = norm(n_vec);

    let raan_deg = if n_mag > 1e-8 {
        n_vec[1].atan2(n_vec[0]).to_degrees().rem_euclid(360.0)
    } else {
        0.0
    };

    let arg_perigee_deg = if e < 1e-8 {
        0.0
    } else if n_mag > 1e-8 {
        let cos_omega = dot(n_vec, e_vec) / (n_mag * e);
        let mut omega = cos_omega.clamp(-1.0, 1.0).acos().to_degrees();
        if e_vec[2] < 0.0 {
            omega = 360.0 - omega;
        }
        omega
    } else {
        // Equatorial elliptical: longitude of periapsis.
        e_vec[1].atan2(e_vec[0]).to_degrees().rem_euclid(360.0)
    };

    let true_anomaly_deg = if e < 1e-8 {
        if n_mag > 1e-8 {
            // Circular inclined: argument of latitude, measured from the node.
            let cos_u = dot(n_vec, r) / (n_mag * r_mag);
            let mut u = cos_u.clamp(-1.0, 1.0).acos().to_degrees();
            if r[2] < 0.0 {
                u = 360.0 - u;
            }
            u
        } else {
            // Circular equatorial: true longitude.
            r[1].atan2(r[0]).to_degrees().rem_euclid(360.0)
        }
    } else {
        let cos_nu = dot(e_vec, r) / (e * r_mag);
        let mut nu = cos_nu.clamp(-1.0, 1.0).acos().to_degrees();
        if rv < 0.0 {
            nu = 360.0 - nu;
        }
        nu
    };

    Ok(KeplerElements {
        semi_major_axis_km,
        eccentricity: e,
        inclination_deg,
        raan_deg,
        arg_perigee_deg,
        true_anomaly_deg,
        epoch_sim_seconds,
    })
}

/// Radial distance (km).
pub fn norm(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// True when position and velocity are near-perpendicular (apsis).
pub fn at_apsis(position_km: [f64; 3], velocity_km_s: [f64; 3]) -> bool {
    let r = norm(position_km);
    let v = norm(velocity_km_s);
    if r < 1.0 || v < 1e-6 {
        return false;
    }
    dot(position_km, velocity_km_s).abs() / (r * v) < 0.05
}

/// True when at perigee (local minimum radius on elliptical arc).
pub fn at_perigee(
    elements: &KeplerElements,
    position_km: [f64; 3],
    velocity_km_s: [f64; 3],
) -> bool {
    if !at_apsis(position_km, velocity_km_s) {
        return false;
    }
    let r = norm(position_km);
    let rp = elements.semi_major_axis_km * (1.0 - elements.eccentricity);
    r <= rp * 1.02
}

/// True when at apogee (local maximum radius).
pub fn at_apogee(
    elements: &KeplerElements,
    position_km: [f64; 3],
    velocity_km_s: [f64; 3],
) -> bool {
    if !at_apsis(position_km, velocity_km_s) {
        return false;
    }
    let r = norm(position_km);
    let ra = elements.semi_major_axis_km * (1.0 + elements.eccentricity);
    r >= ra * 0.98
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;

    use super::*;
    use crate::orbit::{circular_leo, eci_position_km, eci_velocity_km_s};

    #[test]
    fn pv_round_trip_preserves_position_circular_inclined() {
        let elements = circular_leo(400.0, 51.6, 0.0);
        let t = 1_234.0;
        let r = eci_position_km(&elements, t);
        let v = eci_velocity_km_s(&elements, t);
        let back = elements_from_pv(r, v, t).unwrap();
        let r2 = eci_position_km(&back, t);
        for k in 0..3 {
            assert_relative_eq!(r2[k], r[k], epsilon = 1.0);
        }
    }

    #[test]
    fn pv_round_trip_circular_leo() {
        let elements = circular_leo(400.0, 51.6, 100.0);
        let r = eci_position_km(&elements, 100.0);
        let v = eci_velocity_km_s(&elements, 100.0);
        let back = elements_from_pv(r, v, 100.0).unwrap();
        assert_relative_eq!(
            back.semi_major_axis_km,
            elements.semi_major_axis_km,
            epsilon = 0.1
        );
        assert_relative_eq!(back.eccentricity, 0.0, epsilon = 1e-4);
        assert_relative_eq!(
            back.inclination_deg,
            elements.inclination_deg,
            epsilon = 0.01
        );
    }
}

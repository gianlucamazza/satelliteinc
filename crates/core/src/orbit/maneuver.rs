use crate::constants::{EARTH_RADIUS_KM, MU_EARTH};
use crate::error::SimError;
use crate::orbit::state_vectors::{cross, norm};
use crate::orbit::{eci_position_km, eci_velocity_km_s, elements_from_pv, KeplerElements};

const G0_KM_S2: f64 = 9.80665e-3; // km/s²

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BurnDirection {
    Prograde,
    Retrograde,
}

#[derive(Debug, Clone, Copy)]
pub struct HohmannPlan {
    pub burn1_dv_km_s: f64,
    pub burn2_dv_km_s: f64,
    pub total_dv_km_s: f64,
    pub burn1_direction: BurnDirection,
    pub burn2_direction: BurnDirection,
    pub target_altitude_km: f64,
    pub raising: bool,
}

/// Hohmann transfer Δv between two circular orbits (radii in km).
pub fn hohmann_plan(
    r1_km: f64,
    r2_km: f64,
    target_altitude_km: f64,
) -> Result<HohmannPlan, SimError> {
    if r1_km <= EARTH_RADIUS_KM || r2_km <= EARTH_RADIUS_KM {
        return Err(SimError::InvalidAction("orbit inside Earth".into()));
    }
    if (r1_km - r2_km).abs() < 1.0 {
        return Err(SimError::InvalidAction("target altitude too close".into()));
    }

    let raising = r2_km > r1_km;
    let (dv1, dv2) = if raising {
        let dv1 = (MU_EARTH / r1_km).sqrt() * ((2.0 * r2_km / (r1_km + r2_km)).sqrt() - 1.0);
        let dv2 = (MU_EARTH / r2_km).sqrt() * (1.0 - (2.0 * r1_km / (r1_km + r2_km)).sqrt());
        (dv1, dv2)
    } else {
        let dv1 = (MU_EARTH / r1_km).sqrt() * (1.0 - (2.0 * r2_km / (r1_km + r2_km)).sqrt());
        let dv2 = (MU_EARTH / r2_km).sqrt() * ((2.0 * r1_km / (r1_km + r2_km)).sqrt() - 1.0);
        (dv1, dv2)
    };

    Ok(HohmannPlan {
        burn1_dv_km_s: dv1,
        burn2_dv_km_s: dv2,
        total_dv_km_s: dv1 + dv2,
        burn1_direction: if raising {
            BurnDirection::Prograde
        } else {
            BurnDirection::Retrograde
        },
        burn2_direction: if raising {
            BurnDirection::Prograde
        } else {
            BurnDirection::Retrograde
        },
        target_altitude_km,
        raising,
    })
}

/// Propellant consumed (kg) via Tsiolkovsky rocket equation.
pub fn propellant_for_delta_v(
    delta_v_km_s: f64,
    dry_mass_kg: f64,
    propellant_kg: f64,
    isp_s: f64,
) -> f64 {
    if delta_v_km_s <= 0.0 {
        return 0.0;
    }
    let m0 = dry_mass_kg + propellant_kg;
    let m1 = m0 * (-delta_v_km_s / (isp_s * G0_KM_S2)).exp();
    // May exceed propellant_kg: callers use that to reject infeasible burns.
    (m0 - m1).max(0.0)
}

/// Remaining Δv capacity (km/s) from propellant on board.
pub fn delta_v_capacity_km_s(dry_mass_kg: f64, propellant_kg: f64, isp_s: f64) -> f64 {
    if propellant_kg <= 0.0 {
        return 0.0;
    }
    let m0 = dry_mass_kg + propellant_kg;
    let m1 = dry_mass_kg;
    isp_s * G0_KM_S2 * (m0 / m1).ln()
}

/// Apply impulsive burn; returns new elements at `epoch_sim_seconds`.
pub fn apply_impulsive_burn(
    elements: &KeplerElements,
    sim_seconds: f64,
    delta_v_km_s: f64,
    direction: BurnDirection,
) -> Result<KeplerElements, SimError> {
    if delta_v_km_s <= 0.0 {
        return Err(SimError::InvalidAction("delta-v must be positive".into()));
    }
    let r = eci_position_km(elements, sim_seconds);
    let v = eci_velocity_km_s(elements, sim_seconds);
    let h = cross(r, v);
    let t_hat = unit(cross(h, r))?;
    let sign = if direction == BurnDirection::Prograde {
        1.0
    } else {
        -1.0
    };
    let dv = [
        t_hat[0] * delta_v_km_s * sign,
        t_hat[1] * delta_v_km_s * sign,
        t_hat[2] * delta_v_km_s * sign,
    ];
    let v_new = [v[0] + dv[0], v[1] + dv[1], v[2] + dv[2]];
    elements_from_pv(r, v_new, sim_seconds)
}

fn unit(v: [f64; 3]) -> Result<[f64; 3], SimError> {
    let n = norm(v);
    if n < 1e-12 {
        return Err(SimError::InvalidAction("zero vector".into()));
    }
    Ok([v[0] / n, v[1] / n, v[2] / n])
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;

    use super::*;
    use crate::orbit::circular_leo;

    #[test]
    fn hohmann_400_to_600km_total_dv() {
        let r1 = EARTH_RADIUS_KM + 400.0;
        let r2 = EARTH_RADIUS_KM + 600.0;
        let plan = hohmann_plan(r1, r2, 600.0).unwrap();
        // ~111 m/s total for 400→600 km Hohmann
        assert_relative_eq!(plan.total_dv_km_s, 0.111, epsilon = 0.02);
        assert!(plan.raising);
    }

    #[test]
    fn prograde_burn_raises_semi_major_axis() {
        let elements = circular_leo(400.0, 51.6, 0.0);
        let new = apply_impulsive_burn(&elements, 0.0, 0.05, BurnDirection::Prograde).unwrap();
        assert!(new.semi_major_axis_km > elements.semi_major_axis_km);
    }
}

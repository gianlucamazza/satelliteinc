//! Interplanetary transfers on "Hohmann rails": closed-form transfer
//! ellipses in the parent body's frame, launch windows from phase angles,
//! patched-conic departure/capture Δv. Deliberately Lambert-less — the game
//! has no mid-course controls, so window timing and Δv margins are the
//! decisions, not porkchop precision.

use std::f64::consts::PI;

use crate::body::BodyRegistry;
use crate::error::SimError;
use crate::orbit::{KeplerElements, mean_motion_rad_s};

/// Phase-angle tolerance for an open window (sibling transfers).
pub const WINDOW_TOLERANCE_DEG: f64 = 3.0;

#[derive(Debug, Clone)]
pub struct TransferPlan {
    pub origin_body_id: String,
    pub target_body_id: String,
    /// Body whose frame the transfer ellipse lives in (origin's parent for
    /// sibling transfers; the origin itself for transfers to its own moon).
    pub frame_body_id: String,
    pub transfer_seconds: f64,
    /// Departure burn Δv from the parking orbit (physical; paid by the
    /// launch provider as the injection, shown for realism).
    pub departure_dv_km_s: f64,
    /// Capture burn Δv into the target parking orbit (paid in propellant).
    pub capture_dv_km_s: f64,
    /// Transfer ellipse in the frame body's coordinates, periapsis at the
    /// departure longitude, epoch at departure.
    pub elements: KeplerElements,
}

/// The transfer frame for `origin -> target`, or None when no route exists.
/// Sibling bodies transfer in their shared parent's frame; a body's own
/// moon is reached in the body's frame.
fn transfer_frame(reg: &BodyRegistry, origin: usize, target: usize) -> Option<usize> {
    let origin_body = reg.body(origin);
    let target_body = reg.body(target);
    if target_body.parent == Some(origin) {
        return Some(origin);
    }
    match (origin_body.parent, target_body.parent) {
        (Some(a), Some(b)) if a == b => Some(a),
        _ => None,
    }
}

/// True ecliptic longitude (deg) of a body around the frame body — used to
/// anchor the rendered transfer ellipse to the real departure point.
fn longitude_in_frame_deg(reg: &BodyRegistry, body: usize, sim_seconds: f64) -> f64 {
    let pos = reg.parent_relative_position_km(body, sim_seconds);
    pos[1].atan2(pos[0]).to_degrees().rem_euclid(360.0)
}

/// MEAN ecliptic longitude (deg): linear in time by construction, which
/// keeps the closed-form window solve exact for the mean-element model
/// (true longitudes wobble by the equation of center — up to ~11 deg for
/// Mars — and would make successive window roots inconsistent).
fn mean_longitude_deg(reg: &BodyRegistry, body: usize, sim_seconds: f64) -> f64 {
    let b = reg.body(body);
    let Some(orbit) = &b.orbit else { return 0.0 };
    let Some(parent) = b.parent else { return 0.0 };
    let mu = reg.body(parent).mu_km3_s2;
    let n_deg_s = mean_motion_rad_s(mu, orbit.semi_major_axis_km).to_degrees();
    let m0 = crate::orbit::true_to_mean_anomaly_deg(orbit.true_anomaly_deg, orbit.eccentricity);
    (orbit.raan_deg + orbit.arg_perigee_deg + m0 + n_deg_s * sim_seconds).rem_euclid(360.0)
}

/// Current target-minus-origin phase angle on mean longitudes
/// (deg, [0, 360)). Sibling transfers only.
pub fn phase_angle_deg(reg: &BodyRegistry, origin: usize, target: usize, sim_seconds: f64) -> f64 {
    (mean_longitude_deg(reg, target, sim_seconds) - mean_longitude_deg(reg, origin, sim_seconds))
        .rem_euclid(360.0)
}

fn orbit_radius_km(reg: &BodyRegistry, body: usize) -> Result<f64, SimError> {
    reg.body(body)
        .orbit
        .as_ref()
        .map(|o| o.semi_major_axis_km)
        .ok_or_else(|| SimError::InvalidAction("body has no orbit".into()))
}

/// Hohmann time of flight between circular radii around `mu`.
fn hohmann_seconds(mu: f64, r1: f64, r2: f64) -> f64 {
    let a_t = f64::midpoint(r1, r2);
    PI * (a_t.powi(3) / mu).sqrt()
}

/// Phase angle (deg) the target must lead the origin by at departure so it
/// sits at the transfer apoapsis on arrival.
pub fn required_phase_angle_deg(
    reg: &BodyRegistry,
    origin: usize,
    target: usize,
) -> Result<f64, SimError> {
    let frame = transfer_frame(reg, origin, target)
        .ok_or_else(|| SimError::InvalidAction("no transfer route".into()))?;
    let mu = reg.body(frame).mu_km3_s2;
    let r1 = if frame == origin {
        // Moon-style transfers depart from a fast parking orbit: the window
        // is effectively always open. Callers special-case this.
        return Ok(0.0);
    } else {
        orbit_radius_km(reg, origin)?
    };
    let r2 = orbit_radius_km(reg, target)?;
    let t_h = hohmann_seconds(mu, r1, r2);
    let n_target = mean_motion_rad_s(mu, r2).to_degrees();
    Ok((180.0 - n_target * t_h).rem_euclid(360.0))
}

/// Whether the transfer window is open at `sim_seconds`.
pub fn window_open(
    reg: &BodyRegistry,
    origin: usize,
    target: usize,
    sim_seconds: f64,
) -> Result<bool, SimError> {
    let frame = transfer_frame(reg, origin, target)
        .ok_or_else(|| SimError::InvalidAction("no transfer route".into()))?;
    if frame == origin {
        return Ok(true);
    }
    let current = phase_angle_deg(reg, origin, target, sim_seconds);
    let required = required_phase_angle_deg(reg, origin, target)?;
    let delta = (current - required + 180.0).rem_euclid(360.0) - 180.0;
    Ok(delta.abs() <= WINDOW_TOLERANCE_DEG)
}

/// Absolute sim time of the next window opening at or after `after`.
/// Closed-form from the synodic rate on mean longitudes.
pub fn next_launch_window(
    reg: &BodyRegistry,
    origin: usize,
    target: usize,
    after_sim_seconds: f64,
) -> Result<f64, SimError> {
    let frame = transfer_frame(reg, origin, target)
        .ok_or_else(|| SimError::InvalidAction("no transfer route".into()))?;
    if frame == origin {
        return Ok(after_sim_seconds);
    }
    let mu = reg.body(frame).mu_km3_s2;
    let r1 = orbit_radius_km(reg, origin)?;
    let r2 = orbit_radius_km(reg, target)?;
    // Synodic rate: how fast the phase angle drifts (deg/s, signed).
    let rate = mean_motion_rad_s(mu, r2).to_degrees() - mean_motion_rad_s(mu, r1).to_degrees();
    if rate.abs() < 1e-15 {
        return Err(SimError::InvalidAction("degenerate synodic rate".into()));
    }
    let current = phase_angle_deg(reg, origin, target, after_sim_seconds);
    let required = required_phase_angle_deg(reg, origin, target)?;
    // Solve required = current + rate * dt (mod 360) for the smallest dt >= 0.
    let mut delta = (required - current).rem_euclid(360.0);
    if rate < 0.0 {
        // Phase drifts backward: reach the target angle going the other way.
        delta -= 360.0;
    }
    let dt = delta / rate;
    Ok(after_sim_seconds + dt.max(0.0))
}

/// Build the full plan for departing at `departure_sim_seconds` from a
/// circular parking orbit of radius `parking_radius_km` around the origin,
/// capturing into `capture_altitude_km` at the target.
pub fn plan_transfer(
    reg: &BodyRegistry,
    origin: usize,
    target: usize,
    parking_radius_km: f64,
    capture_altitude_km: f64,
    departure_sim_seconds: f64,
) -> Result<TransferPlan, SimError> {
    let frame = transfer_frame(reg, origin, target)
        .ok_or_else(|| SimError::InvalidAction("no transfer route".into()))?;
    let frame_body = reg.body(frame);
    let mu = frame_body.mu_km3_s2;
    let target_body = reg.body(target);

    let (r1, departure_longitude_deg) = if frame == origin {
        // To the origin's own moon: the parking orbit is already in frame.
        (
            parking_radius_km,
            longitude_in_frame_deg(reg, target, departure_sim_seconds) + 180.0,
        )
    } else {
        (
            orbit_radius_km(reg, origin)?,
            longitude_in_frame_deg(reg, origin, departure_sim_seconds),
        )
    };
    let r2 = orbit_radius_km(reg, target)?;
    if (r1 - r2).abs() < 1.0 {
        return Err(SimError::InvalidAction(
            "already at the target radius".into(),
        ));
    }

    let a_t = f64::midpoint(r1, r2);
    let transfer_seconds = hohmann_seconds(mu, r1, r2);

    // Vis-viva speeds at both ends of the transfer ellipse.
    let v1_transfer = (mu * (2.0 / r1 - 1.0 / a_t)).sqrt();
    let v2_transfer = (mu * (2.0 / r2 - 1.0 / a_t)).sqrt();
    let v1_body = (mu / r1).sqrt();
    let v2_body = (mu / r2).sqrt();

    let departure_dv_km_s = if frame == origin {
        // The injection burn from the parking orbit IS the transfer burn.
        (v1_transfer - v1_body).abs()
    } else {
        // Hyperbolic escape from the origin's parking orbit with the
        // required excess speed (patched conic).
        let v_inf = (v1_transfer - v1_body).abs();
        let mu_o = reg.body(origin).mu_km3_s2;
        let r_p = parking_radius_km;
        (v_inf * v_inf + 2.0 * mu_o / r_p).sqrt() - (mu_o / r_p).sqrt()
    };

    // Hyperbolic capture at periapsis into the body's capture orbit —
    // circular for small bodies, a Juno-style high-e ellipse for giants
    // (which is what keeps insertion at Jupiter near ~1.5 km/s).
    let v_inf_arr = (v2_body - v2_transfer).abs();
    let r_cap = target_body.radius_km + capture_altitude_km;
    let mu_t = target_body.mu_km3_s2;
    let e_cap = target_body.capture_eccentricity;
    let a_cap = r_cap / (1.0 - e_cap);
    let v_capture_orbit = (mu_t * (2.0 / r_cap - 1.0 / a_cap)).sqrt();
    let capture_dv_km_s = (v_inf_arr * v_inf_arr + 2.0 * mu_t / r_cap).sqrt() - v_capture_orbit;

    // Transfer ellipse: periapsis (or apoapsis when descending) at the
    // departure longitude in the frame body's plane.
    let raising = r2 > r1;
    let (periapsis_longitude, true_anomaly_deg) = if raising {
        (departure_longitude_deg, 0.0)
    } else {
        // Departing from apoapsis toward a lower target.
        (departure_longitude_deg + 180.0, 180.0)
    };
    let eccentricity = (r1.max(r2) - r1.min(r2)) / (r1 + r2);
    let elements = KeplerElements {
        semi_major_axis_km: a_t,
        eccentricity,
        inclination_deg: 0.0,
        raan_deg: 0.0,
        arg_perigee_deg: periapsis_longitude.rem_euclid(360.0),
        true_anomaly_deg,
        epoch_sim_seconds: departure_sim_seconds,
    };

    Ok(TransferPlan {
        origin_body_id: reg.body(origin).id.clone(),
        target_body_id: target_body.id.clone(),
        frame_body_id: frame_body.id.clone(),
        transfer_seconds,
        departure_dv_km_s,
        capture_dv_km_s,
        elements,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::GameContent;

    fn registry() -> BodyRegistry {
        let content =
            GameContent::from_json(include_str!("../../../../content/game.json")).unwrap();
        BodyRegistry::from_content(&content.bodies).unwrap()
    }

    #[test]
    fn earth_mars_hohmann_matches_textbook() {
        let reg = registry();
        let earth = reg.idx("earth").unwrap();
        let mars = reg.idx("mars").unwrap();
        let plan = plan_transfer(&reg, earth, mars, 6_771.0, 400.0, 0.0).unwrap();
        // ~259 days of flight, ~3.6 km/s TMI from LEO, ~2.1 km/s capture.
        let days = plan.transfer_seconds / 86_400.0;
        assert!((250.0..270.0).contains(&days), "tof {days} days");
        assert!(
            (3.3..3.9).contains(&plan.departure_dv_km_s),
            "TMI {}",
            plan.departure_dv_km_s
        );
        assert!(
            (1.5..2.5).contains(&plan.capture_dv_km_s),
            "capture {}",
            plan.capture_dv_km_s
        );
    }

    #[test]
    fn mars_window_recurs_at_the_synodic_period() {
        let reg = registry();
        let earth = reg.idx("earth").unwrap();
        let mars = reg.idx("mars").unwrap();
        let w1 = next_launch_window(&reg, earth, mars, 0.0).unwrap();
        let w2 = next_launch_window(&reg, earth, mars, w1 + 86_400.0).unwrap();
        let synodic_days = (w2 - w1) / 86_400.0;
        // Earth-Mars synodic period ~780 days (mean-element model tolerance).
        assert!(
            (760.0..800.0).contains(&synodic_days),
            "synodic {synodic_days} days"
        );
        assert!(window_open(&reg, earth, mars, w1).unwrap());
    }

    #[test]
    fn moon_transfer_is_always_open_and_cheap_to_capture() {
        let reg = registry();
        let earth = reg.idx("earth").unwrap();
        let moon = reg.idx("moon").unwrap();
        assert!(window_open(&reg, earth, moon, 12_345.0).unwrap());
        let plan = plan_transfer(&reg, earth, moon, 6_771.0, 100.0, 0.0).unwrap();
        let days = plan.transfer_seconds / 86_400.0;
        assert!((4.0..6.0).contains(&days), "TLI coast {days} days");
        // ~3.1 km/s TLI, well under 1 km/s capture into low lunar orbit.
        assert!((2.9..3.3).contains(&plan.departure_dv_km_s));
        assert!(
            (0.6..1.1).contains(&plan.capture_dv_km_s),
            "LOI {}",
            plan.capture_dv_km_s
        );
    }

    #[test]
    fn no_route_between_moon_and_mars() {
        let reg = registry();
        let moon = reg.idx("moon").unwrap();
        let mars = reg.idx("mars").unwrap();
        assert!(plan_transfer(&reg, moon, mars, 1_837.0, 400.0, 0.0).is_err());
    }
}

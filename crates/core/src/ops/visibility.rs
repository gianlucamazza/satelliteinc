/// Geodetic (lat°, lon°, alt km) to body-fixed (km) on a spherical body.
#[must_use]
pub fn geodetic_to_ecef_km(
    body_radius_km: f64,
    lat_deg: f64,
    lon_deg: f64,
    alt_km: f64,
) -> [f64; 3] {
    let lat = lat_deg.to_radians();
    let lon = lon_deg.to_radians();
    let r = body_radius_km + alt_km;
    [
        r * lat.cos() * lon.cos(),
        r * lat.cos() * lon.sin(),
        r * lat.sin(),
    ]
}

/// Topocentric elevation angle (degrees) from ground station to satellite.
#[must_use]
pub fn elevation_deg(
    body_radius_km: f64,
    station_lat_deg: f64,
    station_lon_deg: f64,
    sat_ecef_km: [f64; 3],
) -> f64 {
    let station = geodetic_to_ecef_km(body_radius_km, station_lat_deg, station_lon_deg, 0.0);
    let diff = [
        sat_ecef_km[0] - station[0],
        sat_ecef_km[1] - station[1],
        sat_ecef_km[2] - station[2],
    ];
    let range = (diff[0].powi(2) + diff[1].powi(2) + diff[2].powi(2)).sqrt();
    if range < 1e-6 {
        return 90.0;
    }

    let lat = station_lat_deg.to_radians();
    let lon = station_lon_deg.to_radians();
    // Only the "up" leg of the ENU basis matters for elevation.
    let up = [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()];

    let u = diff[0] * up[0] + diff[1] * up[1] + diff[2] * up[2];
    let elev_rad = (u / range).asin();
    elev_rad.to_degrees()
}

#[must_use]
pub fn is_in_pass(
    body_radius_km: f64,
    station_lat_deg: f64,
    station_lon_deg: f64,
    min_elevation_deg: f64,
    sat_ecef_km: [f64; 3],
) -> bool {
    elevation_deg(
        body_radius_km,
        station_lat_deg,
        station_lon_deg,
        sat_ecef_km,
    ) >= min_elevation_deg
}

/// Seconds until the next pass start (elevation crosses min), searching forward.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn seconds_until_pass(
    body: &crate::body::CelestialBody,
    elements: &crate::orbit::KeplerElements,
    station_lat_deg: f64,
    station_lon_deg: f64,
    min_elevation_deg: f64,
    from_sim_seconds: f64,
    max_search_seconds: f64,
    step_seconds: f64,
) -> Option<f64> {
    use crate::ops::eci_to_ecef_km;
    use crate::orbit::eci_position_km;

    #[allow(clippy::cast_sign_loss)] // both inputs are positive durations
    let steps = (max_search_seconds / step_seconds).ceil() as u64;
    let mut was_in_pass = {
        let eci = eci_position_km(body.mu_km3_s2, elements, from_sim_seconds);
        let ecef = eci_to_ecef_km(body.rotation_rad_s, eci, from_sim_seconds);
        is_in_pass(
            body.radius_km,
            station_lat_deg,
            station_lon_deg,
            min_elevation_deg,
            ecef,
        )
    };

    for i in 1..=steps {
        let t = from_sim_seconds + i as f64 * step_seconds;
        let eci = eci_position_km(body.mu_km3_s2, elements, t);
        let ecef = eci_to_ecef_km(body.rotation_rad_s, eci, t);
        let in_pass = is_in_pass(
            body.radius_km,
            station_lat_deg,
            station_lon_deg,
            min_elevation_deg,
            ecef,
        );
        if in_pass && !was_in_pass {
            return Some(t - from_sim_seconds);
        }
        was_in_pass = in_pass;
    }
    None
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;

    use super::*;
    use crate::constants::{EARTH_RADIUS_KM, EARTH_ROTATION_RAD_S, MU_EARTH};
    use crate::ops::eci_to_ecef_km;
    use crate::orbit::{circular_orbit, eci_position_km};

    #[test]
    fn overhead_satellite_high_elevation() {
        // Station at equator (0°N, 0°E); satellite directly above along local "up" (ECEF +X)
        let sat = [6_771.0, 0.0, 0.0];
        let elev = elevation_deg(EARTH_RADIUS_KM, 0.0, 0.0, sat);
        assert_relative_eq!(elev, 90.0, epsilon = 1.0);
    }

    #[test]
    fn leo_pass_exists_for_high_latitude_station() {
        let elements = circular_orbit(EARTH_RADIUS_KM, 400.0, 97.0, 0.0);
        let station_lat = 78.0;
        let station_lon = 16.0;
        let mut found_pass = false;
        for t in (0..5400).step_by(30) {
            let eci = eci_position_km(MU_EARTH, &elements, f64::from(t));
            let ecef = eci_to_ecef_km(EARTH_ROTATION_RAD_S, eci, f64::from(t));
            if is_in_pass(EARTH_RADIUS_KM, station_lat, station_lon, 5.0, ecef) {
                found_pass = true;
                break;
            }
        }
        assert!(
            found_pass,
            "expected at least one pass over high-latitude station"
        );
    }
}

use crate::constants::EARTH_RADIUS_KM;

/// Geodetic (lat°, lon°, alt km) to ECEF (km).
pub fn geodetic_to_ecef_km(lat_deg: f64, lon_deg: f64, alt_km: f64) -> [f64; 3] {
    let lat = lat_deg.to_radians();
    let lon = lon_deg.to_radians();
    let r = EARTH_RADIUS_KM + alt_km;
    [
        r * lat.cos() * lon.cos(),
        r * lat.cos() * lon.sin(),
        r * lat.sin(),
    ]
}

/// Topocentric elevation angle (degrees) from ground station to satellite.
pub fn elevation_deg(station_lat_deg: f64, station_lon_deg: f64, sat_ecef_km: [f64; 3]) -> f64 {
    let station = geodetic_to_ecef_km(station_lat_deg, station_lon_deg, 0.0);
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
    // ENU basis at station
    let east = [-lon.sin(), lon.cos(), 0.0];
    let north = [-lat.sin() * lon.cos(), -lat.sin() * lon.sin(), lat.cos()];
    let up = [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()];

    let u = diff[0] * up[0] + diff[1] * up[1] + diff[2] * up[2];
    let _ = east;
    let _ = north;
    let elev_rad = (u / range).asin();
    elev_rad.to_degrees()
}

pub fn is_in_pass(
    station_lat_deg: f64,
    station_lon_deg: f64,
    min_elevation_deg: f64,
    sat_ecef_km: [f64; 3],
) -> bool {
    elevation_deg(station_lat_deg, station_lon_deg, sat_ecef_km) >= min_elevation_deg
}

/// Seconds until the next pass start (elevation crosses min), searching forward.
pub fn seconds_until_pass(
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

    let steps = (max_search_seconds / step_seconds).ceil() as u64;
    let mut was_in_pass = {
        let eci = eci_position_km(elements, from_sim_seconds);
        let ecef = eci_to_ecef_km(eci, from_sim_seconds);
        is_in_pass(station_lat_deg, station_lon_deg, min_elevation_deg, ecef)
    };

    for i in 1..=steps {
        let t = from_sim_seconds + i as f64 * step_seconds;
        let eci = eci_position_km(elements, t);
        let ecef = eci_to_ecef_km(eci, t);
        let in_pass = is_in_pass(station_lat_deg, station_lon_deg, min_elevation_deg, ecef);
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
    use crate::ops::eci_to_ecef_km;
    use crate::orbit::{circular_leo, eci_position_km};

    #[test]
    fn overhead_satellite_high_elevation() {
        // Station at equator (0°N, 0°E); satellite directly above along local "up" (ECEF +X)
        let sat = [6_771.0, 0.0, 0.0];
        let elev = elevation_deg(0.0, 0.0, sat);
        assert_relative_eq!(elev, 90.0, epsilon = 1.0);
    }

    #[test]
    fn leo_pass_exists_for_high_latitude_station() {
        let elements = circular_leo(400.0, 97.0, 0.0);
        let station_lat = 78.0;
        let station_lon = 16.0;
        let mut found_pass = false;
        for t in (0..5400).step_by(30) {
            let eci = eci_position_km(&elements, t as f64);
            let ecef = eci_to_ecef_km(eci, t as f64);
            if is_in_pass(station_lat, station_lon, 5.0, ecef) {
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

#![cfg(feature = "sgp4")]

use satelliteinc_core::{orbit::TlePropagator, SimState};

mod common;
use common::load_content;

#[test]
fn tle_catalog_propagates_iss() {
    let content = load_content();
    let entry = &content.tle_catalog[0];
    let prop = TlePropagator::from_entry(entry).unwrap_or_else(|e| panic!("TLE init failed: {e}"));

    // Sim time 0 = TLE epoch: the ISS state must match its catalogued orbit.
    let at_epoch = prop.propagate(0.0).expect("propagate at epoch");
    assert!(
        at_epoch.altitude_km > 400.0 && at_epoch.altitude_km < 440.0,
        "ISS altitude at epoch ~418 km, got {}",
        at_epoch.altitude_km
    );
    let v = at_epoch.velocity_eci_km_s;
    let speed = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    assert!(
        (speed - 7.66).abs() < 0.1,
        "ISS orbital speed ~7.66 km/s, got {speed}"
    );

    let sim = SimState::new(content);
    assert!(!sim.tle_propagators.is_empty(), "ISS TLE should load");

    let snap = sim.snapshot();
    assert!(!snap.reference_satellites.is_empty());
    let iss = &snap.reference_satellites[0];
    assert!(
        iss.altitude_km > 400.0 && iss.altitude_km < 440.0,
        "ISS ~418 km, got {}",
        iss.altitude_km
    );
}

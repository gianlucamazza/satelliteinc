use satelliteinc_core::{SimState, action::PlayerAction, orbit::circular_orbit};

mod common;
use common::load_content;

#[test]
fn hohmann_raise_400_to_600km() {
    let content = load_content();
    let mut sim = SimState::new(content).unwrap();

    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "microsat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "earth_imaging".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();

    let craft = sim.spacecraft.as_mut().unwrap();
    // Place at perigee (true anomaly 0) for Hohmann raise burn 1
    craft.elements = Some(circular_orbit(
        satelliteinc_core::constants::EARTH_RADIUS_KM,
        400.0,
        97.0,
        sim.clock.sim_seconds,
    ));
    craft.maneuver_target_altitude_km = 600.0;

    sim.apply_action(PlayerAction::StartHohmann).unwrap();
    assert!(sim.spacecraft.as_ref().unwrap().pending_hohmann.is_some());

    // Advance ~half orbit to apogee of transfer ellipse
    let period = satelliteinc_core::orbit::orbit_period_seconds(
        satelliteinc_core::constants::MU_EARTH,
        6771.0,
    );
    sim.clock.sim_seconds += period * 0.5;

    sim.apply_action(PlayerAction::CompleteHohmann).unwrap();

    let alt = sim
        .spacecraft
        .as_ref()
        .unwrap()
        .current_altitude_km(satelliteinc_core::constants::EARTH_RADIUS_KM)
        .unwrap();
    assert!(
        (alt - 600.0).abs() < 50.0,
        "expected ~600 km altitude, got {alt}"
    );
    assert!(sim.spacecraft.as_ref().unwrap().pending_hohmann.is_none());
    assert!(sim.spacecraft.as_ref().unwrap().total_delta_v_used_km_s > 0.1);
}

#[test]
fn burn_beyond_propellant_capacity_is_rejected() {
    let content = load_content();
    let mut sim = SimState::new(content).unwrap();

    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "microsat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();

    let craft = sim.spacecraft.as_mut().unwrap();
    craft.propellant_kg = 0.01; // nearly empty tank
    let elements_before = craft.elements;
    let propellant_before = craft.propellant_kg;
    assert!(craft.delta_v_remaining_km_s(1.0) < 1.0);

    let result = sim.apply_action(PlayerAction::ProgradeBurn { delta_v_km_s: 1.0 });
    assert!(result.is_err(), "burn beyond capacity must be rejected");

    let craft = sim.spacecraft.as_ref().unwrap();
    assert_eq!(craft.elements, elements_before, "orbit must be unchanged");
    assert_eq!(
        craft.propellant_kg, propellant_before,
        "propellant must be unchanged"
    );
    assert_eq!(craft.total_delta_v_used_km_s, 0.0);
}

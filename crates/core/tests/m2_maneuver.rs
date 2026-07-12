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
        assigned_craft_id: Some(0),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();

    let now = sim.clock.sim_seconds;
    let craft = common::only_craft_mut(&mut sim);
    // Place at perigee (true anomaly 0) for Hohmann raise burn 1
    craft.elements = Some(circular_orbit(
        satelliteinc_core::constants::EARTH_RADIUS_KM,
        400.0,
        97.0,
        now,
    ));
    craft.maneuver_target_altitude_km = 600.0;

    sim.apply_action(PlayerAction::StartHohmann { craft_id: 0 })
        .unwrap();
    assert!(common::only_craft(&sim).pending_hohmann.is_some());

    // Advance ~half orbit to apogee of transfer ellipse
    let period = satelliteinc_core::orbit::orbit_period_seconds(
        satelliteinc_core::constants::MU_EARTH,
        6771.0,
    );
    sim.clock.sim_seconds += period * 0.5;

    sim.apply_action(PlayerAction::CompleteHohmann { craft_id: 0 })
        .unwrap();

    let alt = common::only_craft(&sim)
        .current_altitude_km(satelliteinc_core::constants::EARTH_RADIUS_KM)
        .unwrap();
    assert!(
        (alt - 600.0).abs() < 50.0,
        "expected ~600 km altitude, got {alt}"
    );
    assert!(common::only_craft(&sim).pending_hohmann.is_none());
    assert!(common::only_craft(&sim).total_delta_v_used_km_s > 0.1);
}

#[test]
fn burn_beyond_propellant_capacity_is_rejected() {
    let content = load_content();
    let mut sim = SimState::new(content).unwrap();

    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "microsat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();

    let craft = common::only_craft_mut(&mut sim);
    craft.propellant_kg = 0.01; // nearly empty tank
    let elements_before = craft.elements;
    let propellant_before = craft.propellant_kg;
    assert!(craft.delta_v_remaining_km_s(1.0) < 1.0);

    let result = sim.apply_action(PlayerAction::ProgradeBurn {
        craft_id: 0,
        delta_v_km_s: 1.0,
    });
    assert!(result.is_err(), "burn beyond capacity must be rejected");

    let craft = common::only_craft(&sim);
    assert_eq!(craft.elements, elements_before, "orbit must be unchanged");
    assert_eq!(
        craft.propellant_kg, propellant_before,
        "propellant must be unchanged"
    );
    assert_eq!(craft.total_delta_v_used_km_s, 0.0);
}

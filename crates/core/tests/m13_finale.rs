//! M13: the Jupiter finale — eccentric capture physics, RTG power,
//! radiation upsets, and the last Program gate.

use satelliteinc_core::{BodyRegistry, SimState, action::PlayerAction};

mod common;
use common::load_content;

#[test]
fn jupiter_capture_fits_the_pioneer_tanks() {
    let content = load_content();
    let reg = BodyRegistry::from_content(&content.bodies).unwrap();
    let earth = reg.idx("earth").unwrap();
    let jupiter = reg.idx("jupiter").unwrap();
    let plan =
        satelliteinc_core::orbit::plan_transfer(&reg, earth, jupiter, 6_771.0, 400_000.0, 0.0)
            .unwrap();
    // Juno-style high-e capture: ~1.3-1.8 km/s, not the ~7 km/s a circular
    // insertion would cost. The Pioneer carries ~1.83 km/s.
    assert!(
        (1.0..1.83).contains(&plan.capture_dv_km_s),
        "JOI {}",
        plan.capture_dv_km_s
    );
    // Cruise is measured in years.
    let years = plan.transfer_seconds / (365.25 * 86_400.0);
    assert!((2.0..3.5).contains(&years), "cruise {years} years");

    let pioneer = content.bus("pioneer").unwrap();
    let dv_capacity = satelliteinc_core::orbit::delta_v_capacity_km_s(
        pioneer.mass_kg,
        pioneer.propellant_kg,
        pioneer.isp_s,
    );
    assert!(
        dv_capacity > plan.capture_dv_km_s,
        "pioneer {dv_capacity} vs JOI {}",
        plan.capture_dv_km_s
    );
}

#[test]
fn rtg_keeps_power_positive_in_eclipse() {
    let content = load_content();
    let pioneer = content.bus("pioneer").unwrap();
    let craft = satelliteinc_core::spacecraft::Spacecraft::new_from_bus(pioneer, "Pioneer");
    // In darkness the RTG still outruns the idle payload draw.
    let balance_dark = craft.power_balance_w(false, 1.0, 0.0, 1.0);
    assert!(
        balance_dark > 0.0,
        "RTG must survive eclipse: {balance_dark} W"
    );
    // A solar bus in the same darkness drains.
    let explorer = content.bus("explorer").unwrap();
    let solar_craft = satelliteinc_core::spacecraft::Spacecraft::new_from_bus(explorer, "Solar");
    assert!(solar_craft.power_balance_w(false, 1.0, 0.0, 1.0) < 0.0);
}

#[test]
fn radiation_upset_wipes_a_fifth_of_the_buffer() {
    let mut sim = SimState::new(load_content()).unwrap();
    // Sandbox craft parked at Jupiter via scenario machinery: reuse venus
    // sandbox then retarget by hand for a minimal deterministic setup.
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "sandbox_venus".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();
    {
        let craft = sim.spacecraft.as_mut().unwrap();
        craft.body_id = "jupiter".into();
        craft.elements = Some(satelliteinc_core::orbit::circular_orbit(
            69_911.0,
            1_500_000.0,
            10.0,
            sim.clock.sim_seconds,
        ));
        craft.buffer_mb = 1_000.0;
        craft.anomaly_cooldown_until = 0.0;
    }
    sim.apply_action(PlayerAction::SetTimeAccel {
        multiplier: 1_000.0,
    })
    .unwrap();
    // Jupiter anomaly rate 2.0 halves the 4000 s bucket; bucket 2 is a
    // buffer corruption. Run until one fires.
    let mut wiped = None;
    for _ in 0..800 {
        sim.step(0.016);
        let craft = sim.spacecraft.as_ref().unwrap();
        if let Some(a) = &craft.active_anomaly {
            if a.kind == satelliteinc_core::ops::AnomalyType::BufferCorruption {
                wiped = Some((a.label().to_string(), craft.buffer_mb));
                break;
            }
            // Clear other anomalies instantly to reach the corruption bucket.
            sim.spacecraft.as_mut().unwrap().active_anomaly = None;
            sim.spacecraft.as_mut().unwrap().anomaly_cooldown_until = 0.0;
        }
    }
    let (label, buffer_after) = wiped.expect("a radiation upset fires");
    assert_eq!(label, "Radiation upset");
    // 20% of whatever was in the buffer at the moment it fired is gone —
    // the craft kept imaging before the hit, so check the wipe factor by
    // reconstructing: after = before_at_hit * 0.8, and imaging adds little
    // within one tick. The buffer started at 1000 and only grew, so a 25%
    // (default) wipe would leave < 790 only if buffer stayed ~1000.
    assert!(buffer_after > 780.0, "wiped too much: {buffer_after}");
}

#[test]
fn jupiter_program_needs_radiation_hardening() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "campaign_start".into(),
    })
    .unwrap();
    sim.company.reputation = 250.0;
    sim.company.cash_usd = 10_000_000.0;
    sim.company
        .completed_contracts
        .push("venus_pathfinder".into());
    // Keystone + rep + cash but no rad-hardening: still gated.
    assert!(
        sim.apply_action(PlayerAction::UnlockProgram {
            program_id: "jupiter_program".into(),
        })
        .is_err()
    );
    sim.company.unlocked_tech.push("radiation_hardening".into());
    sim.apply_action(PlayerAction::UnlockProgram {
        program_id: "jupiter_program".into(),
    })
    .unwrap();
}

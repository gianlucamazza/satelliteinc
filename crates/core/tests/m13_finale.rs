//! M13: the Jupiter finale — eccentric capture physics, RTG power,
//! radiation upsets, and the last Program gate.

use satelliteinc_core::{BodyRegistry, SimEvent, SimState, action::PlayerAction};

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
    let craft = satelliteinc_core::spacecraft::Spacecraft::new_from_bus(pioneer, "Pioneer", 0);
    // In darkness the RTG still outruns the idle payload draw.
    let balance_dark = craft.power_balance_w(false, 1.0, 0.0, 1.0);
    assert!(
        balance_dark > 0.0,
        "RTG must survive eclipse: {balance_dark} W"
    );
    // A solar bus in the same darkness drains.
    let explorer = content.bus("explorer").unwrap();
    let solar_craft = satelliteinc_core::spacecraft::Spacecraft::new_from_bus(explorer, "Solar", 1);
    assert!(solar_craft.power_balance_w(false, 1.0, 0.0, 1.0) < 0.0);
}

#[test]
fn radiation_upset_wipes_a_fifth_of_the_buffer() {
    use satelliteinc_core::spacecraft::PayloadMode;

    // Sandbox craft staged directly at Jupiter (unified sandbox).
    let mut sim = common::sandbox_at("jupiter", 1_500_000.0, 10.0);
    let craft_id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::Launch { craft_id }).unwrap();
    {
        let craft = common::only_craft_mut(&mut sim);
        craft.buffer_mb = 1_000.0;
        craft.anomaly_cooldown_until = 0.0;
        // Freeze imaging so buffer stays fixed until the SEU hits.
        craft.payload_mode = PayloadMode::Safe;
    }
    sim.apply_action(PlayerAction::SetTimeAccel {
        multiplier: 1_000.0,
    })
    .unwrap();
    // Jupiter weights favor SEU; buffer corruption is a one-shot event (not sticky).
    let mut wiped = None;
    for _ in 0..4_000 {
        let before = common::only_craft(&sim).buffer_mb;
        sim.step(0.016);
        for ev in sim.drain_events() {
            if let SimEvent::AnomalyStarted {
                kind: satelliteinc_core::ops::AnomalyType::BufferCorruption,
                label,
                ..
            } = ev
            {
                let buffer_after = common::only_craft(&sim).buffer_mb;
                wiped = Some((label, before, buffer_after));
            }
        }
        // Clear sticky faults so SEU rolls can keep firing.
        if common::only_craft(&sim).active_anomaly.is_some() {
            common::only_craft_mut(&mut sim).active_anomaly = None;
            common::only_craft_mut(&mut sim).anomaly_cooldown_until = 0.0;
        }
        if wiped.is_some() {
            break;
        }
    }
    let (label, before, after) = wiped.expect("a radiation upset fires");
    assert_eq!(label, "Radiation upset");
    // Jupiter flavor wipes 20% of the buffer at hit time.
    let ratio = after / before;
    assert!(
        (0.78..=0.82).contains(&ratio),
        "expected ~20% wipe, before={before} after={after} ratio={ratio}"
    );
    assert!(
        common::only_craft(&sim).active_anomaly.is_none(),
        "SEU must not leave a sticky latch"
    );
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

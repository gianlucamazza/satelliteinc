//! Stored-literal baseline guarding the M8 "bodies as data" refactor.
//!
//! Captured on the pre-refactor build. Every assertion is bit-exact
//! (`f64::to_bits`): the refactor threads mu/radius/rotation through
//! signatures without touching a single formula, so the same action
//! script must reproduce the same bits. Re-capture deliberately (and
//! only) when behavior is *meant* to change (M10 dynamic sun).
//!
//! Run with `M8_CAPTURE=1 cargo test -p satelliteinc-core --test m8_baseline -- --nocapture`
//! to print fresh literals instead of asserting.

use satelliteinc_core::constants::{EARTH_RADIUS_KM, MU_EARTH};
use satelliteinc_core::orbit::{
    eci_position_km, hohmann_plan, orbit_period_seconds, seconds_to_apsides,
};
use satelliteinc_core::{SimState, action::PlayerAction};

mod common;
use common::load_content;

fn capture_mode() -> bool {
    std::env::var("M8_CAPTURE").is_ok()
}

fn check(label: &str, value: f64, expected_bits: u64) {
    if capture_mode() {
        println!("(\"{label}\", {:#018x})", value.to_bits());
    } else {
        assert_eq!(
            value.to_bits(),
            expected_bits,
            "{label} drifted: got {value:?} (bits {:#018x}), expected bits {expected_bits:#018x}",
            value.to_bits()
        );
    }
}

/// Full campaign slice: scenario, contract, launch, downlink, 3000 ticks
/// at 1000x (`48_000` sim-seconds — passes, eclipses, recurring costs).
#[test]
fn campaign_slice_is_bit_identical() {
    let content = load_content();
    let mut sim = SimState::new(content).unwrap();

    for action in [
        PlayerAction::LoadScenario {
            scenario_id: "campaign_start".into(),
        },
        PlayerAction::SetOrbit {
            altitude_km: 500.0,
            inclination_deg: 60.0,
        },
        PlayerAction::Launch,
        PlayerAction::SetDownlink { active: true },
        PlayerAction::SelectDownlinkStation {
            station_id: "chile".into(),
        },
        PlayerAction::SetTimeAccel { multiplier: 1000.0 },
    ] {
        sim.apply_action(action).unwrap();
    }
    for _ in 0..3000 {
        sim.step(0.016);
    }

    let snap = sim.snapshot();
    let craft = snap.spacecraft.expect("craft on orbit");

    check("sim_seconds", snap.sim_seconds, 0x40e7700000000000);
    check("cash_usd", snap.cash_usd, 0x4113f8d8e38dec80);
    check("reputation", snap.reputation, 0x4034000000000000);
    check("pos_x", craft.position_eci_km[0], 0xc0ba4fd72e31487d);
    check("pos_y", craft.position_eci_km[1], 0x40853086d443fc51);
    check("pos_z", craft.position_eci_km[2], 0x409259c724ccf28b);
    check("power_wh", craft.power_wh, 0x403304c527754742);
    check("buffer_mb", craft.buffer_mb, 0x40a0000000000000);
    check(
        "contract_progress",
        snap.active_contract.map_or(-1.0, |c| c.downlink_mb_done),
        0x405c400000000000,
    );
}

/// Pure-function anchors: Hohmann Δv, propagation, apsides, period.
#[test]
fn orbit_math_is_bit_identical() {
    let plan = hohmann_plan(MU_EARTH, EARTH_RADIUS_KM, 6771.0, 7071.0, 700.0).unwrap();
    check("hohmann_dv1", plan.burn1_dv_km_s, 0x3fb52bc44402a1aa);
    check("hohmann_dv2", plan.burn2_dv_km_s, 0x3fb4f156e7a00bb3);

    let elements = satelliteinc_core::orbit::KeplerElements {
        semi_major_axis_km: 7000.0,
        eccentricity: 0.05,
        inclination_deg: 63.4,
        raan_deg: 45.0,
        arg_perigee_deg: 30.0,
        true_anomaly_deg: 120.0,
        epoch_sim_seconds: 0.0,
    };
    let pos = eci_position_km(MU_EARTH, &elements, 12_345.0);
    check("prop_x", pos[0], 0xc0b2754f682832d4);
    check("prop_y", pos[1], 0xc0b57424fc994e76);
    check("prop_z", pos[2], 0xc090eb43514255eb);

    let (to_perigee, to_apogee) = seconds_to_apsides(MU_EARTH, &elements, 12_345.0);
    check("to_perigee", to_perigee, 0x40a99f1b14df4123);
    check("to_apogee", to_apogee, 0x4076d4b694099e8d);
    check(
        "period",
        orbit_period_seconds(MU_EARTH, elements.semi_major_axis_km),
        0x40b6c484425e0d51,
    );
}

//! M10: the system moves — deterministic ephemerides drive body positions
//! and the per-body sun direction.

use satelliteinc_core::{BodyRegistry, SimState, action::PlayerAction};

mod common;
use common::load_content;

fn registry() -> BodyRegistry {
    BodyRegistry::from_content(&load_content().bodies).unwrap()
}

const YEAR_SECONDS: f64 = 365.25 * 86_400.0;

#[test]
fn mars_position_period_matches_keplers_third_law() {
    let reg = registry();
    let mars = reg.idx("mars").unwrap();
    let sun_mu = reg.get("sun").unwrap().mu_km3_s2;
    let a = reg.get("mars").unwrap().orbit.unwrap().semi_major_axis_km;
    let period = satelliteinc_core::orbit::orbit_period_seconds(sun_mu, a);
    // Mars sidereal year ~687 days.
    assert!((period / 86_400.0 - 687.0).abs() < 2.0);

    let p0 = reg.heliocentric_position_km(mars, 0.0);
    let p1 = reg.heliocentric_position_km(mars, period);
    for k in 0..3 {
        // One full revolution returns to the starting point (same ellipse).
        assert!(
            (p0[k] - p1[k]).abs() < 1_000.0,
            "axis {k}: {} vs {}",
            p0[k],
            p1[k]
        );
    }
    // Half a revolution is far away.
    let ph = reg.heliocentric_position_km(mars, period / 2.0);
    let d = ((p0[0] - ph[0]).powi(2) + (p0[1] - ph[1]).powi(2)).sqrt();
    assert!(d > 3.0e8, "half-period displacement too small: {d}");
}

#[test]
fn sun_direction_is_always_unit_norm_and_rotates() {
    let reg = registry();
    let earth = reg.idx("earth").unwrap();
    let mut min_dot: f64 = 1.0;
    let d0 = reg.sun_direction_from(earth, 0.0);
    for i in 0..12 {
        let t = f64::from(i) * YEAR_SECONDS / 12.0;
        let d = reg.sun_direction_from(earth, t);
        let norm = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        assert!((norm - 1.0).abs() < 1e-12);
        min_dot = min_dot.min(d0[0] * d[0] + d0[1] * d[1] + d0[2] * d[2]);
    }
    // Six months in, the sun points the other way.
    assert!(
        min_dot < -0.9,
        "sun direction never swept the year: {min_dot}"
    );
}

#[test]
fn snapshot_bodies_move_with_time() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "campaign_start".into(),
    })
    .unwrap();
    let before = sim.snapshot();
    let mars0 = before
        .bodies
        .iter()
        .find(|b| b.id == "mars")
        .unwrap()
        .position_helio_km;
    // A month of sim time.
    sim.apply_action(PlayerAction::SetTimeAccel {
        multiplier: 10_000.0,
    })
    .unwrap();
    for _ in 0..2_600 {
        sim.step(0.1);
    }
    let after = sim.snapshot();
    let mars1 = after
        .bodies
        .iter()
        .find(|b| b.id == "mars")
        .unwrap()
        .position_helio_km;
    let d = ((mars0[0] - mars1[0]).powi(2) + (mars0[1] - mars1[1]).powi(2)).sqrt();
    assert!(d > 1.0e7, "Mars barely moved in a month: {d} km");
    assert_eq!(after.epoch_utc, "2026-03-20T00:00:00Z");
}

#[test]
fn light_delay_scales_with_distance() {
    let mut earth_sim = SimState::new(load_content()).unwrap();
    earth_sim
        .apply_action(PlayerAction::LoadScenario {
            scenario_id: "campaign_start".into(),
        })
        .unwrap();
    earth_sim
        .apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();
    let earth_delay = common::snap_craft(&earth_sim).light_delay_seconds;
    assert!(earth_delay < 0.1, "LEO light delay ~ms, got {earth_delay}");

    let mut mars_sim = common::sandbox_at("mars", 400.0, 60.0);
    let mars_id = mars_sim.fleet.last().unwrap().id;
    mars_sim
        .apply_action(PlayerAction::Launch { craft_id: mars_id })
        .unwrap();
    let mars_delay = common::snap_craft(&mars_sim).light_delay_seconds;
    // Earth-Mars one-way light time is between ~3 and ~22 minutes.
    assert!(
        (180.0..1_400.0).contains(&mars_delay),
        "Mars light delay out of range: {mars_delay} s"
    );
}

#[test]
fn year_long_run_is_deterministic() {
    let run = || {
        let mut sim = common::sandbox_at("moon", 100.0, 85.0);
        let id = sim.fleet.last().unwrap().id;
        for action in [
            PlayerAction::Launch { craft_id: id },
            PlayerAction::SetDownlink {
                craft_id: id,
                active: true,
            },
            PlayerAction::SetTimeAccel {
                multiplier: 10_000.0,
            },
        ] {
            sim.apply_action(action).unwrap();
        }
        // ~37 sim-days at 10000x: enough for dozens of lunar orbits and a
        // visibly moved ephemeris while keeping the suite fast.
        for _ in 0..320 {
            sim.step(1.0);
        }
        let snap = sim.snapshot();
        let craft = snap.spacecraft.into_iter().next().unwrap();
        (
            craft.position_eci_km[0].to_bits(),
            craft.power_wh.to_bits(),
            snap.bodies
                .iter()
                .find(|b| b.id == "moon")
                .unwrap()
                .position_helio_km[0]
                .to_bits(),
        )
    };
    assert_eq!(run(), run());
}

//! M9: operating around other bodies — lunar orbits, DSN links with the
//! farside blackout, per-body anomaly flavor, cross-body contract gating.

use satelliteinc_core::{SimState, action::PlayerAction};

mod common;
use common::load_content;

fn luna_sim() -> SimState {
    let mut sim = common::sandbox_at("moon", 100.0, 85.0);
    let craft_id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::Launch { craft_id }).unwrap();
    sim
}

#[test]
fn lunar_orbit_uses_lunar_gravity() {
    let mut sim = luna_sim();
    let craft = common::only_craft(&sim);
    assert_eq!(craft.body_id, "moon");
    let elements = craft.elements.unwrap();
    // a = 1737.4 + 100 km; period from the Moon's mu, not Earth's.
    let moon = sim.bodies.get("moon").unwrap();
    let period =
        satelliteinc_core::orbit::orbit_period_seconds(moon.mu_km3_s2, elements.semi_major_axis_km);
    assert!(
        (6_500.0..8_500.0).contains(&period),
        "low lunar orbit is ~2 h, got {period} s"
    );
    // Around Earth the same semi-major axis would orbit in minutes; make
    // sure the sim actually propagates with the lunar mu: after a quarter
    // period the craft has moved ~90 degrees, not several revolutions.
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 1.0 })
        .unwrap();
    let p0 = common::snap_craft(&sim).position_eci_km;
    #[allow(clippy::cast_sign_loss)] // period is positive by construction
    let quarter_ticks = (period / 4.0) as usize;
    for _ in 0..quarter_ticks {
        sim.step(1.0);
    }
    let p1 = common::snap_craft(&sim).position_eci_km;
    let dot = p0[0] * p1[0] + p0[1] * p1[1] + p0[2] * p1[2];
    let n0 = (p0[0] * p0[0] + p0[1] * p0[1] + p0[2] * p0[2]).sqrt();
    let n1 = (p1[0] * p1[0] + p1[1] * p1[1] + p1[2] * p1[2]).sqrt();
    let angle = (dot / (n0 * n1)).clamp(-1.0, 1.0).acos().to_degrees();
    assert!(
        (60.0..120.0).contains(&angle),
        "expected ~90 deg of arc after a quarter period, got {angle}"
    );
}

#[test]
fn dsn_link_toggles_over_a_lunar_orbit() {
    let mut sim = luna_sim();
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 100.0 })
        .unwrap();

    // Sample DSN visibility across ~1.5 lunar orbits: the farside leg must
    // black the link out, the nearside leg must restore it.
    let mut saw_link = false;
    let mut saw_blackout = false;
    for _ in 0..300 {
        sim.step(0.4); // 40 sim-seconds per sample
        let craft = common::snap_craft(&sim);
        let snap = sim.snapshot();
        let dsn_ids: Vec<&str> = snap
            .ground_stations
            .iter()
            .filter(|s| s.dsn)
            .map(|s| s.id.as_str())
            .collect();
        let dsn_up = craft
            .station_status
            .iter()
            .any(|st| dsn_ids.contains(&st.station_id.as_str()) && st.in_pass);
        if dsn_up {
            saw_link = true;
        } else {
            saw_blackout = true;
        }
    }
    assert!(saw_link, "DSN link never came up over the nearside");
    assert!(saw_blackout, "farside never blacked out the DSN link");
}

#[test]
fn dsn_bandwidth_scaled_by_body_factor() {
    let sim = luna_sim();
    let craft = common::snap_craft(&sim);
    let goldstone = craft
        .station_status
        .iter()
        .find(|s| s.station_id == "goldstone")
        .unwrap();
    // 40 Mbps dish at the Moon's 0.7 factor.
    assert!((goldstone.bandwidth_mbps - 28.0).abs() < 1e-9);
    // Local Earth dishes are unreachable from the Moon: not even listed.
    assert!(
        !craft
            .station_status
            .iter()
            .any(|s| s.station_id == "svalbard")
    );
}

#[test]
fn contract_data_from_wrong_body_is_rejected() {
    // Earth sandbox craft downlinking against a Moon contract: no accrual.
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "sandbox_lab".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "moon_mapper".into(),
        assigned_craft_id: Some(0),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();
    sim.apply_action(PlayerAction::SetDownlink {
        craft_id: 0,
        active: true,
    })
    .unwrap();
    sim.apply_action(PlayerAction::SelectDownlinkStation {
        craft_id: 0,
        station_id: "chile".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 1000.0 })
        .unwrap();
    for _ in 0..3000 {
        sim.step(0.016);
    }
    let contract = common::only_contract(&sim);
    assert_eq!(
        contract.downlink_mb_done, 0.0,
        "moon contract must not accrue from earth orbit"
    );
    assert_eq!(sim.snapshot().active_contracts[0].orbit_ok, Some(false));
}

#[test]
fn mars_dust_storm_flavor_applies() {
    let mut sim = common::sandbox_at("mars", 400.0, 60.0);
    let craft_id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::Launch { craft_id }).unwrap();
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 1000.0 })
        .unwrap();
    // Mars rate 1.3 shortens the 4000 s bucket to ~3077 s; bucket 1 is a
    // comms dropout. Run far enough to cross it.
    let mut dropout: Option<(String, f64)> = None;
    for _ in 0..500 {
        sim.step(0.016);
        if let Some(a) = sim.fleet.first().and_then(|c| c.active_anomaly.as_ref()) {
            dropout = Some((a.label().to_string(), a.duration_seconds));
            break;
        }
    }
    let (label, duration) = dropout.expect("an anomaly fires within the window");
    assert_eq!(label, "Dust storm");
    assert!((duration - 7_200.0).abs() < 1e-9);
}

#[test]
fn moon_scenario_is_deterministic() {
    let run = || {
        let mut sim = common::sandbox_at("moon", 100.0, 85.0);
        let id = sim.fleet.last().unwrap().id;
        for action in [
            PlayerAction::Launch { craft_id: id },
            PlayerAction::SetDownlink {
                craft_id: id,
                active: true,
            },
            PlayerAction::SetTimeAccel { multiplier: 1000.0 },
        ] {
            sim.apply_action(action).unwrap();
        }
        for _ in 0..1000 {
            sim.step(0.016);
        }
        let snap = sim.snapshot();
        let craft = snap.spacecraft.into_iter().next().unwrap();
        (
            snap.cash_usd.to_bits(),
            craft.position_eci_km[0].to_bits(),
            craft.buffer_mb.to_bits(),
        )
    };
    assert_eq!(run(), run());
}

#[test]
fn earth_stations_rejected_from_the_moon() {
    let mut sim = luna_sim();
    let craft_id = sim.fleet.last().unwrap().id;
    let err = sim.apply_action(PlayerAction::SelectDownlinkStation {
        craft_id,
        station_id: "svalbard".into(),
    });
    assert!(
        err.is_err(),
        "local Earth dish must not serve a lunar craft"
    );
    // DSN dish is fine.
    sim.apply_action(PlayerAction::SelectDownlinkStation {
        craft_id,
        station_id: "goldstone".into(),
    })
    .unwrap();
}

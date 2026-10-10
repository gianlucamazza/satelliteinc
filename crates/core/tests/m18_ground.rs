//! M18 ground network: body-local stations, tech gates, sandbox unlock.

use satelliteinc_core::{SimState, action::PlayerAction};

mod common;
use common::{load_content, sandbox_at, snap_craft};

/// Moon craft in campaign mode with no tech unlocks (local dish locked).
fn luna_campaign_locked() -> SimState {
    let mut sim = sandbox_at("moon", 100.0, 45.0);
    let craft_id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::Launch { craft_id }).unwrap();
    sim.apply_action(PlayerAction::SetGameMode {
        mode: "campaign".into(),
    })
    .unwrap();
    // Mode switch keeps the sandbox fleet; strip tech so station gates apply.
    sim.company.unlocked_tech.clear();
    sim
}

#[test]
fn lunar_relay_locked_in_campaign_until_tech() {
    let sim = luna_campaign_locked();
    let snap = sim.snapshot();
    let lunar = snap
        .ground_stations
        .iter()
        .find(|s| s.id == "lunar_nearside")
        .expect("lunar station in content");
    assert!(!lunar.unlocked, "campaign without lunar_relay tech");
    assert_eq!(lunar.requires_tech.as_deref(), Some("lunar_relay"));
    assert_eq!(lunar.body_id, "moon");

    let craft = snap_craft(&sim);
    assert!(
        !craft
            .station_status
            .iter()
            .any(|s| s.station_id == "lunar_nearside"),
        "locked station must not appear in craft station_status"
    );
    // DSN still reachable from the Moon.
    assert!(
        craft
            .station_status
            .iter()
            .any(|s| s.station_id == "goldstone"),
        "DSN remains the deep-space lifeline"
    );
}

#[test]
fn purchasing_lunar_relay_unlocks_local_moon_dish() {
    let mut sim = luna_campaign_locked();
    let tech = sim.content.tech("lunar_relay").unwrap().clone();
    sim.company.apply_tech(&tech);

    let snap = sim.snapshot();
    let lunar = snap
        .ground_stations
        .iter()
        .find(|s| s.id == "lunar_nearside")
        .unwrap();
    assert!(lunar.unlocked);

    let craft = snap_craft(&sim);
    let local = craft
        .station_status
        .iter()
        .find(|s| s.station_id == "lunar_nearside")
        .expect("unlocked lunar dish listed for moon craft");
    // Local link is full dish bandwidth (no DSN body factor).
    assert!(
        (local.bandwidth_mbps - 80.0).abs() < 1e-9,
        "local moon dish should be 80 Mbps, got {}",
        local.bandwidth_mbps
    );
}

#[test]
fn sandbox_unlocks_all_stations() {
    let mut sim = sandbox_at("moon", 100.0, 30.0);
    let craft_id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::Launch { craft_id }).unwrap();
    let snap = sim.snapshot();
    let lunar = snap
        .ground_stations
        .iter()
        .find(|s| s.id == "lunar_nearside")
        .unwrap();
    assert!(lunar.unlocked, "sandbox ignores station tech gates");
    assert!(
        snap_craft(&sim)
            .station_status
            .iter()
            .any(|s| s.station_id == "lunar_nearside")
    );
}

#[test]
fn cannot_select_locked_station_for_downlink() {
    let mut sim = luna_campaign_locked();
    let craft_id = sim.fleet.last().unwrap().id;
    let err = sim.apply_action(PlayerAction::SelectDownlinkStation {
        craft_id,
        station_id: "lunar_nearside".into(),
    });
    assert!(
        err.is_err(),
        "locked station must reject SelectDownlinkStation"
    );
}

#[test]
fn solar_flux_exposed_on_snapshot() {
    let mut sim = sandbox_at("mars", 400.0, 25.0);
    let craft_id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::Launch { craft_id }).unwrap();
    let flux = snap_craft(&sim).solar_flux;
    assert!(
        (0.35..=0.53).contains(&flux),
        "Mars solar_flux on snapshot should match 1/r² band, got {flux}"
    );
}

#[test]
fn content_accepts_lunar_station_and_tech() {
    let content = load_content();
    assert!(content.tech("lunar_relay").is_some());
    assert!(
        content
            .ground_stations
            .iter()
            .any(|s| s.id == "lunar_nearside" && s.body_id == "moon")
    );
    // Fresh sim loads content without error (validate already ran).
    let _ = SimState::new(content).unwrap();
}

#[test]
fn ground_network_covers_career_bodies() {
    let content = load_content();
    let leo = content
        .ground_stations
        .iter()
        .filter(|s| s.body_id == "earth" && !s.dsn)
        .count();
    assert!(leo >= 6, "LEO commercial net too thin: {leo}");
    // Equatorial coverage for multi-station contracts.
    assert!(
        content
            .ground_stations
            .iter()
            .any(|s| s.id == "singapore" || s.id == "malindi"),
        "need at least one near-equatorial commercial dish"
    );
    for (body, tech) in [
        ("moon", "lunar_relay"),
        ("mars", "mars_relay"),
        ("venus", "venus_relay"),
        ("jupiter", "jupiter_relay"),
    ] {
        assert!(content.tech(tech).is_some(), "missing tech {tech}");
        assert!(
            content.ground_stations.iter().any(|s| s.body_id == body
                && s.requires_tech.as_deref() == Some(tech)
                || (body == "moon" && s.body_id == "moon" && s.requires_tech.is_some())),
            "missing gated local dish on {body}"
        );
    }
    assert!(
        content
            .ground_stations
            .iter()
            .any(|s| s.id == "lunar_farside"),
        "farside lunar relay expected"
    );
    assert!(
        content
            .ground_stations
            .iter()
            .filter(|s| s.body_id == "mars" && !s.dsn)
            .count()
            >= 2,
        "mars should have multi-node proximity net"
    );
}

#[test]
fn launch_binds_reachable_preferred_station_not_content_first() {
    // Moon sandbox launch must NOT leave the craft on Svalbard (first in content).
    let mut sim = sandbox_at("moon", 100.0, 45.0);
    let craft_id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::Launch { craft_id }).unwrap();
    let craft = snap_craft(&sim);
    let dish = craft
        .downlink_station_id
        .as_deref()
        .expect("launch assigns a downlink station");
    assert_ne!(
        dish, "svalbard",
        "must not default to first content station"
    );
    // Sandbox unlocks lunar dish — preferred is local highest-bw (nearside 80).
    assert_eq!(
        dish, "lunar_nearside",
        "sandbox moon launch prefers local high-bw relay, got {dish}"
    );
    // Station must appear in reachable status list.
    assert!(
        craft.station_status.iter().any(|s| s.station_id == dish),
        "selected dish must be in station_status"
    );
}

#[test]
fn earth_launch_prefers_highest_bw_local_dish() {
    let content = load_content();
    let mut sim = SimState::new(content).unwrap();
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetOrbit {
        craft_id: 0,
        altitude_km: 400.0,
        inclination_deg: 97.0,
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();
    let dish = snap_craft(&sim)
        .downlink_station_id
        .expect("earth launch assigns dish");
    // Highest commercial LEO bw in content is SvalSat (24 Mbps).
    assert_eq!(dish, "svalbard");
}

#[test]
fn venus_local_relay_gated_until_tech() {
    let mut sim = sandbox_at("venus", 400.0, 30.0);
    let craft_id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::Launch { craft_id }).unwrap();
    // Force campaign gates.
    sim.apply_action(PlayerAction::SetGameMode {
        mode: "campaign".into(),
    })
    .unwrap();
    sim.company.unlocked_tech.clear();
    let snap = sim.snapshot();
    let venus = snap
        .ground_stations
        .iter()
        .find(|s| s.id == "venus_relay")
        .expect("venus relay in content");
    assert!(!venus.unlocked);
    let craft = snap_craft(&sim);
    assert!(
        !craft
            .station_status
            .iter()
            .any(|s| s.station_id == "venus_relay"),
        "locked venus dish must not be reachable"
    );
    assert!(
        craft
            .station_status
            .iter()
            .any(|s| s.station_id == "goldstone"),
        "DSN still available at Venus"
    );
}

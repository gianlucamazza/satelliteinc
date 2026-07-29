//! M17 realism pass: inverse-square solar flux, J2 secular precession,
//! per-bus payload hardware, node-gated plane changes, and light-time
//! gated commanding.

use satelliteinc_core::{
    SimState, action::PlayerAction, ops::CommandType, spacecraft::PayloadMode,
};

mod common;
use common::{load_content, only_craft, sandbox_at, snap_craft};

/// Signed smallest angular difference b − a in degrees.
fn angle_diff_deg(a: f64, b: f64) -> f64 {
    (b - a + 540.0).rem_euclid(360.0) - 180.0
}

/// A 550 km / 97.8° orbit is sun-synchronous by design: J2 must walk the
/// node eastward ~1°/day (matching the mean sun) and precess the perigee
/// backwards.
#[test]
fn j2_makes_sun_synchronous_orbits_work() {
    let mut sim = sandbox_at("earth", 550.0, 97.8);
    let craft_id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::Launch { craft_id }).unwrap();
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 1000.0 })
        .unwrap();

    let start = only_craft(&sim).elements.unwrap();
    // One sim day at 1000x: 5400 ticks of 16 ms.
    for _ in 0..5400 {
        sim.step(0.016);
        let _ = sim.drain_events();
    }
    let end = only_craft(&sim).elements.unwrap();

    let raan_per_day = angle_diff_deg(start.raan_deg, end.raan_deg);
    assert!(
        (0.85..=1.15).contains(&raan_per_day),
        "sun-synch RAAN drift should be ~+1.0 deg/day, got {raan_per_day}"
    );
    let argp_per_day = angle_diff_deg(start.arg_perigee_deg, end.arg_perigee_deg);
    assert!(
        argp_per_day < -1.0,
        "at i=97.8 deg the perigee must regress (~-3.4 deg/day), got {argp_per_day}"
    );
}

/// Solar panel output follows 1/r² from the live ephemerides.
#[test]
fn solar_flux_is_inverse_square() {
    let sim = SimState::new(load_content()).unwrap();
    let flux = |id: &str| {
        let idx = sim.bodies.idx(id).unwrap();
        sim.bodies.solar_flux_at(idx, 0.0)
    };
    let earth = flux("earth");
    assert!(
        (0.96..=1.04).contains(&earth),
        "Earth flux should be ~1 AU-normalized, got {earth}"
    );
    let mars = flux("mars");
    assert!(
        (0.35..=0.53).contains(&mars),
        "Mars flux should sit in the 1.38–1.67 AU band, got {mars}"
    );
    let jupiter = flux("jupiter");
    assert!(
        (0.031..=0.043).contains(&jupiter),
        "Jupiter flux should be ~4% of Earth's, got {jupiter}"
    );
    assert!(
        earth > mars && mars > jupiter,
        "flux must fall monotonically with distance"
    );
}

/// Plane changes burn 2·v·sin(Δi/2) and only fire at a node.
#[test]
fn plane_change_burns_at_the_node_only() {
    let mut sim = sandbox_at("earth", 500.0, 60.0);
    let craft_id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::Launch { craft_id }).unwrap();

    // Fresh launch starts at the ascending node (argp = ν = 0, z = 0).
    let snap = snap_craft(&sim);
    assert!(snap.at_node, "launch epoch should sit on the node");

    sim.apply_action(PlayerAction::PlaneChangeBurn {
        craft_id,
        delta_inclination_deg: 2.0,
    })
    .unwrap();

    let craft = only_craft(&sim);
    let el = craft.elements.unwrap();
    assert!(
        (el.inclination_deg - 62.0).abs() < 1e-9,
        "inclination should move 60 -> 62, got {}",
        el.inclination_deg
    );
    let v = (satelliteinc_core::constants::MU_EARTH / el.semi_major_axis_km).sqrt();
    let expected_dv = 2.0 * v * (1.0_f64.to_radians()).sin();
    assert!(
        (craft.total_delta_v_used_km_s - expected_dv).abs() / expected_dv < 0.01,
        "plane change dv should be 2 v sin(di/2) = {expected_dv}, got {}",
        craft.total_delta_v_used_km_s
    );

    // A quarter orbit later the craft is far off the equator: no burn.
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 1000.0 })
        .unwrap();
    for _ in 0..90 {
        sim.step(0.016);
        let _ = sim.drain_events();
    }
    assert!(!snap_craft(&sim).at_node);
    let err = sim.apply_action(PlayerAction::PlaneChangeBurn {
        craft_id,
        delta_inclination_deg: 2.0,
    });
    assert!(err.is_err(), "off-node plane change must be rejected");
}

/// A command uplinked to a Mars craft cannot act before one-way light
/// time has elapsed, no matter how soon the next pass starts.
#[test]
fn commands_to_mars_wait_for_light_time() {
    let mut sim = sandbox_at("mars", 500.0, 30.0);
    let craft_id = sim.fleet.last().unwrap().id;
    sim.apply_action(PlayerAction::Launch { craft_id }).unwrap();

    let owlt = snap_craft(&sim).light_delay_seconds;
    assert!(
        owlt > 150.0,
        "Mars is never closer than ~0.37 AU (~185 s), got {owlt}"
    );

    let queued_at = sim.clock.sim_seconds;
    sim.apply_action(PlayerAction::QueueCommand {
        craft_id,
        command: CommandType::SafeMode,
        station_id: "goldstone".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 1000.0 })
        .unwrap();

    let mut executed_at = None;
    for _ in 0..30_000 {
        sim.step(0.016);
        for ev in sim.drain_events() {
            if matches!(ev, satelliteinc_core::SimEvent::CommandExecuted { .. }) {
                executed_at = Some(sim.clock.sim_seconds);
            }
        }
        if executed_at.is_some() {
            break;
        }
    }
    let executed_at = executed_at.expect("DSN pass should execute the command within days");
    assert!(
        executed_at >= queued_at + owlt * 0.9,
        "command ran at {executed_at} but light could only arrive at {}",
        queued_at + owlt
    );
    assert_eq!(only_craft(&sim).payload_mode, PayloadMode::Safe);
}

/// Recorder size and imager speed are bus hardware, not universal
/// constants: a cubesat fills its 512 MB at 2 MB/s peak.
#[test]
fn buffer_and_imaging_rate_are_per_bus() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 1000.0 })
        .unwrap();

    let mut peak_rate: f64 = 0.0;
    for _ in 0..600 {
        sim.step(0.016);
        let _ = sim.drain_events();
        peak_rate = peak_rate.max(snap_craft(&sim).imaging_rate_mb_s);
    }
    let snap = snap_craft(&sim);
    assert!(
        (snap.buffer_capacity_mb - 512.0).abs() < 1e-9,
        "cubesat recorder is 512 MB"
    );
    assert!(
        (snap.buffer_mb - 512.0).abs() < 1e-6,
        "an orbit of imaging without downlink fills the recorder, got {}",
        snap.buffer_mb
    );
    assert!(
        (peak_rate - 2.0).abs() < 1e-9,
        "cubesat peak imaging rate is 2 MB/s, got {peak_rate}"
    );
}

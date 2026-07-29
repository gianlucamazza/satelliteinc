use satelliteinc_core::{
    SimState, action::PlayerAction, ops::CommandType, spacecraft::PayloadMode,
};

mod common;
use common::load_content;

#[test]
fn multi_station_pass_and_command_queue() {
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
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "earth_imaging".into(),
        assigned_craft_id: Some(0),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();
    sim.apply_action(PlayerAction::SelectDownlinkStation {
        craft_id: 0,
        station_id: "svalbard".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::QueueCommand {
        craft_id: 0,
        command: CommandType::ClearAnomaly,
        station_id: "svalbard".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetPayloadMode {
        craft_id: 0,
        mode: PayloadMode::DownlinkPriority,
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 500.0 })
        .unwrap();

    // Content-driven counts (Earth LEO + DSN triad + body relays).
    let earth_local = sim
        .ground_stations
        .iter()
        .filter(|s| s.body_id == "earth" && !s.dsn)
        .count();
    let dsn = sim.ground_stations.iter().filter(|s| s.dsn).count();
    assert!(
        earth_local >= 4,
        "need a dense LEO commercial net, got {earth_local}"
    );
    assert_eq!(dsn, 3, "DSN triad expected");
    assert!(
        sim.ground_stations
            .iter()
            .filter(|s| s.body_id == "moon" && !s.dsn)
            .count()
            >= 1,
        "lunar local dish present in content"
    );
    for body in ["mars", "venus", "jupiter"] {
        assert!(
            sim.ground_stations
                .iter()
                .any(|s| s.body_id == body && !s.dsn),
            "local relay dish expected for {body}"
        );
    }

    let mut command_ran = false;
    // ~3 orbits worth of sim time, bounded sim ticks
    for _ in 0..3_000 {
        sim.step(0.016);
        for ev in sim.drain_events() {
            if matches!(ev, satelliteinc_core::SimEvent::CommandExecuted { .. }) {
                command_ran = true;
            }
        }
        if command_ran {
            break;
        }
    }
    assert!(
        command_ran,
        "queued command should execute on svalbard pass"
    );
}

#[test]
fn anomaly_schedule_independent_of_frame_cadence() {
    fn run(real_dt: f64, steps: usize) -> Vec<satelliteinc_core::ops::AnomalyType> {
        let mut sim = SimState::new(load_content()).unwrap();
        sim.apply_action(PlayerAction::SelectBus {
            bus_id: "cubesat".into(),
        })
        .unwrap();
        sim.apply_action(PlayerAction::Launch { craft_id: 0 })
            .unwrap();
        sim.apply_action(PlayerAction::SetTimeAccel {
            multiplier: 1_000.0,
        })
        .unwrap();
        let mut kinds = Vec::new();
        for _ in 0..steps {
            sim.step(real_dt);
            for ev in sim.drain_events() {
                if let satelliteinc_core::SimEvent::AnomalyStarted { kind, .. } = ev {
                    kinds.push(kind);
                }
            }
        }
        kinds
    }

    // Same total sim time (16,300 s) reached with 10x different frame cadence.
    let fine = run(0.0163, 1_000);
    let coarse = run(0.163, 100);
    assert!(!fine.is_empty(), "expected at least one anomaly");
    assert_eq!(
        fine, coarse,
        "anomaly schedule must not depend on frame cadence"
    );
}

#[test]
fn buffer_corruption_is_not_a_sticky_latch() {
    use satelliteinc_core::ops::{ActiveAnomaly, AnomalyType};

    let content = load_content();
    let mut sim = SimState::new(content).unwrap();

    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch { craft_id: 0 })
        .unwrap();

    let started_at = sim.clock.sim_seconds;
    let craft = common::only_craft_mut(&mut sim);
    craft.buffer_mb = 100.0;
    // Even if a SEU is incorrectly latched, duration 0 / non-sticky expires.
    craft.active_anomaly = Some(ActiveAnomaly::new(
        AnomalyType::BufferCorruption,
        started_at,
    ));
    assert!(
        craft
            .active_anomaly
            .as_ref()
            .unwrap()
            .is_expired(started_at + 0.001)
    );

    sim.step(0.016);
    let craft = common::only_craft(&sim);
    assert!(
        craft.active_anomaly.is_none(),
        "buffer SEU must not remain sticky after a tick"
    );
}

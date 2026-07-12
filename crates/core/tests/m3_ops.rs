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
        altitude_km: 400.0,
        inclination_deg: 97.0,
    })
    .unwrap();
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "earth_imaging".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();
    sim.apply_action(PlayerAction::SelectDownlinkStation {
        station_id: "svalbard".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::QueueCommand {
        command: CommandType::ClearAnomaly,
        station_id: "svalbard".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetPayloadMode {
        mode: PayloadMode::DownlinkPriority,
    })
    .unwrap();
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 500.0 })
        .unwrap();

    assert_eq!(sim.ground_stations.iter().filter(|s| !s.dsn).count(), 4);
    assert_eq!(sim.ground_stations.iter().filter(|s| s.dsn).count(), 3);

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
        sim.apply_action(PlayerAction::Launch).unwrap();
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
fn buffer_corruption_expires_and_does_not_block_future_anomalies() {
    use satelliteinc_core::ops::{ActiveAnomaly, AnomalyType};

    let content = load_content();
    let mut sim = SimState::new(content).unwrap();

    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();

    let started_at = sim.clock.sim_seconds;
    let craft = sim.spacecraft.as_mut().unwrap();
    craft.buffer_mb = 100.0;
    craft.active_anomaly = Some(ActiveAnomaly::new(
        AnomalyType::BufferCorruption,
        started_at,
    ));

    // Advance well past the anomaly duration.
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 100.0 })
        .unwrap();
    for _ in 0..100 {
        sim.step(0.016);
    }

    let craft = sim.spacecraft.as_ref().unwrap();
    assert!(
        craft.active_anomaly.is_none(),
        "buffer corruption must expire and clear"
    );
    assert!(
        craft.anomaly_cooldown_until > started_at,
        "expiry must arm the anomaly cooldown so future anomalies can roll"
    );
}

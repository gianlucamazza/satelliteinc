use satelliteinc_core::{SimEvent, SimState, action::PlayerAction};

mod common;
use common::load_content;

#[test]
fn m1_vertical_slice_completes_contract() {
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
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 5000.0 })
        .unwrap();

    let mut completed = false;
    for _ in 0..200_000 {
        // Image in sunlight; downlink only when buffer has data and in pass
        let snap = sim.snapshot();
        let in_pass = snap.spacecraft.first().is_some_and(|c| c.in_pass);
        let buffer = snap.spacecraft.first().map_or(0.0, |c| c.buffer_mb);
        let downlink = in_pass && buffer > 10.0;
        sim.apply_action(PlayerAction::SetDownlink {
            craft_id: 0,
            active: downlink,
        })
        .ok();
        sim.step(0.016);

        if sim
            .company
            .completed_contracts
            .contains(&"earth_imaging".to_string())
        {
            completed = true;
            break;
        }
        for ev in sim.drain_events() {
            if matches!(ev, SimEvent::ContractFailed { .. }) {
                panic!("contract failed at sim day {:.2}", snap.sim_days);
            }
        }
    }

    assert!(completed, "contract should complete within sim time");
    assert!(
        sim.company.cash_usd > 500_000.0,
        "payout should increase cash"
    );
}

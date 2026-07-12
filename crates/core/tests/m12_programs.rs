//! M12: campaign Programs — gates, advances with clawback, on-station
//! presence contracts, repeat decay, grand victory.

use satelliteinc_core::{SimEvent, SimState, action::PlayerAction};

mod common;
use common::load_content;

fn campaign() -> SimState {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "campaign_start".into(),
    })
    .unwrap();
    sim
}

#[test]
fn program_gates_enforce_rep_keystone_and_cash() {
    let mut sim = campaign();
    // Fresh company: rep 20, no keystone — every gate closed.
    assert!(
        sim.apply_action(PlayerAction::UnlockProgram {
            program_id: "moon_program".into(),
        })
        .is_err()
    );
    // Reputation alone is not enough without the Earth keystone.
    sim.company.reputation = 80.0;
    assert!(
        sim.apply_action(PlayerAction::UnlockProgram {
            program_id: "moon_program".into(),
        })
        .is_err()
    );
    // Keystone + rep + cash opens it and bills the buy-in.
    sim.company.completed_contracts.push("science_multi".into());
    let cash = sim.company.cash_usd;
    sim.apply_action(PlayerAction::UnlockProgram {
        program_id: "moon_program".into(),
    })
    .unwrap();
    assert!((cash - sim.company.cash_usd - 250_000.0).abs() < 1.0);
    assert!(
        sim.drain_events()
            .iter()
            .any(|e| matches!(e, SimEvent::ProgramUnlocked { .. }))
    );
    // Idempotence: cannot buy twice.
    assert!(
        sim.apply_action(PlayerAction::UnlockProgram {
            program_id: "moon_program".into(),
        })
        .is_err()
    );
}

#[test]
fn locked_program_hides_contracts_in_campaign() {
    let sim = campaign();
    let market = sim.company.marketplace(&sim.content);
    let moon_offer = market
        .iter()
        .find(|o| o.template_id == "moon_mapper")
        .unwrap();
    assert!(
        !moon_offer.available,
        "moon contracts must be locked behind the Program"
    );
}

#[test]
fn advance_pays_up_front_and_claws_back_on_failure() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "lunar_express".into(),
    })
    .unwrap();
    // The tutorial pre-accepts moon_mapper WITHOUT an advance (scenario
    // seeding): drop it and re-accept through the marketplace instead.
    sim.company.active_contract = None;
    let cash_before = sim.company.cash_usd;
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "moon_mapper".into(),
    })
    .unwrap();
    // 20% of 650k paid on acceptance.
    let advance = sim.company.cash_usd - cash_before;
    assert!((advance - 130_000.0).abs() < 1.0);

    // Let the deadline lapse: clawback + extra reputation hit.
    let rep_before = sim.company.reputation;
    let deadline = sim
        .company
        .active_contract
        .as_ref()
        .unwrap()
        .deadline_sim_seconds;
    let cash_at_fail = sim.company.cash_usd;
    sim.clock.sim_seconds = deadline + 1.0;
    sim.check_contract();
    assert!(
        (cash_at_fail - sim.company.cash_usd - 130_000.0).abs() < 1.0,
        "advance must be clawed back"
    );
    // Standard campaign penalty 10 plus 5 for the broken mobilization.
    assert!((rep_before - sim.company.reputation - 15.0).abs() < 1e-9);
}

#[test]
fn on_station_requirement_gates_completion() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "mars_window".into(),
    })
    .unwrap();
    // Swap the pre-accepted contract for the presence-based one.
    sim.company.active_contract = None;
    sim.company.reputation = 150.0; // clear dust_storm_watch's gate
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "dust_storm_watch".into(),
    })
    .unwrap();
    if let Some(c) = sim.company.active_contract.as_mut() {
        // Data done, presence not: must NOT complete.
        c.downlink_mb_done = c.downlink_mb_required;
    }
    sim.check_contract();
    assert!(
        sim.company.active_contract.is_some(),
        "30 on-station days still missing"
    );
    if let Some(c) = sim.company.active_contract.as_mut() {
        c.on_station_seconds = 30.0 * 86_400.0;
    }
    sim.check_contract();
    assert!(sim.company.active_contract.is_none(), "now it completes");
}

#[test]
fn repeat_payouts_decay() {
    let mut sim = campaign();
    let base = sim.content.contract("earth_imaging").unwrap().payout_usd;
    for (completions, fraction) in [(1_usize, 0.6), (2, 0.45), (3, 0.35), (5, 0.35)] {
        sim.company.completed_contracts = vec!["earth_imaging".into(); completions];
        let offer = sim
            .company
            .marketplace(&sim.content)
            .into_iter()
            .find(|o| o.template_id == "earth_imaging")
            .unwrap();
        assert!(
            (offer.payout_usd - base * fraction).abs() < 1.0,
            "completions {completions}: got {}",
            offer.payout_usd
        );
    }
}

#[test]
fn grand_victory_needs_every_program() {
    let mut sim = campaign();
    sim.company.reputation = 200.0;
    // Seed everything except one Venus contract.
    let all: Vec<(String, String)> = sim
        .content
        .contracts
        .iter()
        .map(|c| (c.id.clone(), c.body_id.clone()))
        .collect();
    for (id, _) in all.iter().filter(|(id, _)| id != "venus_pathfinder") {
        sim.company.completed_contracts.push(id.clone());
    }
    for program in [
        "earth_program",
        "moon_program",
        "mars_program",
        "jupiter_program",
    ] {
        sim.company.completed_programs.push(program.into());
    }
    sim.company.unlocked_programs.push("venus_program".into());
    // campaign_start pre-seeds earth_imaging as active: clear the slot.
    sim.company.active_contract = None;

    // Complete the last keystone through the normal flow.
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "venus_pathfinder".into(),
    })
    .unwrap();
    if let Some(c) = sim.company.active_contract.as_mut() {
        c.downlink_mb_done = c.downlink_mb_required;
        c.stations_used = vec![
            "goldstone".into(),
            "madrid_dsn".into(),
            "canberra_dsn".into(),
        ];
    }
    sim.check_contract();

    let events = sim.drain_events();
    assert!(events.iter().any(|e| matches!(
        e,
        SimEvent::ProgramCompleted { program_id, .. } if program_id == "venus_program"
    )));
    assert!(events.iter().any(|e| matches!(e, SimEvent::GameWon)));
    assert_eq!(
        sim.company.outcome,
        Some(satelliteinc_core::GameOutcome::Victory)
    );
}

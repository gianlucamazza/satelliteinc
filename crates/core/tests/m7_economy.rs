use satelliteinc_core::{GameOutcome, SimEvent, SimState, action::PlayerAction};

mod common;
use common::load_content;

fn advance_days(sim: &mut SimState, days: f64) {
    sim.apply_action(PlayerAction::SetTimeAccel {
        multiplier: 10_000.0,
    })
    .unwrap();
    let total_sim_seconds = days * 86_400.0;
    let mut advanced = 0.0;
    while advanced < total_sim_seconds {
        sim.step(1.0);
        advanced += 10_000.0;
    }
}

#[test]
fn base_overhead_accrues_without_a_satellite() {
    let mut sim = SimState::new(load_content()).unwrap();
    let start = sim.company.cash_usd;
    advance_days(&mut sim, 2.0);
    let spent = start - sim.company.cash_usd;
    // $1k/day base overhead, no satellite on orbit.
    assert!(
        (spent - 2_000.0).abs() < 200.0,
        "expected ~2k overhead over 2 days, spent {spent}"
    );
}

#[test]
fn on_orbit_ops_cost_accrues_per_day() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();
    let start = sim.company.cash_usd;
    advance_days(&mut sim, 2.0);
    let spent = start - sim.company.cash_usd;
    // $1k overhead + $4k cubesat ops = $5k/day.
    assert!(
        (spent - 10_000.0).abs() < 500.0,
        "expected ~10k ops cost over 2 days, spent {spent}"
    );
}

#[test]
fn bankruptcy_fires_once_and_gates_actions() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();
    sim.company.cash_usd = 500.0;

    advance_days(&mut sim, 15.0);

    let events = sim.drain_events();
    let negative_warnings = events
        .iter()
        .filter(|e| matches!(e, SimEvent::CashNegative { .. }))
        .count();
    assert_eq!(
        negative_warnings, 1,
        "cash-negative warning must be one-shot"
    );
    assert!(events.iter().any(|e| matches!(
        e,
        SimEvent::GameOver {
            reason: GameOutcome::Bankrupt
        }
    )));
    assert_eq!(sim.company.outcome, Some(GameOutcome::Bankrupt));
    assert!(sim.company.cash_usd < -50_000.0 + 1_000.0);

    // World is frozen and actions are gated.
    let cash_after = sim.company.cash_usd;
    sim.step(10.0);
    assert_eq!(sim.company.cash_usd, cash_after, "world must freeze");
    assert!(
        sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 1.0 })
            .is_err()
    );

    // LoadScenario recovers.
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "campaign_start".into(),
    })
    .unwrap();
    assert_eq!(sim.company.outcome, None);
}

#[test]
fn completing_earth_contracts_completes_the_earth_program() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();

    // Seed all but one as already completed, then finish the last one.
    for id in ["earth_imaging", "comms_relay", "science_multi", "rapid_eye"] {
        sim.company.completed_contracts.push(id.into());
    }
    sim.company.reputation = 50.0; // clear polar_watch's rep gate
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "polar_watch".into(),
    })
    .unwrap();
    if let Some(c) = sim.company.active_contract.as_mut() {
        c.downlink_mb_done = c.downlink_mb_required;
    }
    sim.check_contract();

    // Earth completion is a Program milestone now, not the campaign win —
    // the grand victory needs every Program (Moon, Mars, Venus...) done.
    assert_eq!(sim.company.outcome, None);
    let events = sim.drain_events();
    assert!(events.iter().any(|e| matches!(
        e,
        SimEvent::ProgramCompleted { program_id, .. } if program_id == "earth_program"
    )));
    assert!(!events.iter().any(|e| matches!(e, SimEvent::GameWon)));
    assert!(
        sim.company
            .completed_programs
            .contains(&"earth_program".to_string())
    );
    // Victory does not gate actions: play continues.
    assert!(
        sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 2.0 })
            .is_ok()
    );
}

#[test]
fn repeated_contract_pays_less_and_gives_no_reputation() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.company.completed_contracts.push("earth_imaging".into());

    let market = sim.company.marketplace(&sim.content);
    let offer = market
        .iter()
        .find(|o| o.template_id == "earth_imaging")
        .unwrap();
    assert!(offer.repeat);
    assert!(offer.available, "completed contracts stay repeatable");
    assert!((offer.payout_usd - 210_000.0).abs() < 1.0);

    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: "earth_imaging".into(),
    })
    .unwrap();
    let contract = sim.company.active_contract.as_ref().unwrap();
    assert!((contract.payout_usd - 210_000.0).abs() < 1.0);
    assert_eq!(contract.reputation_reward, 0.0);
}

#[test]
fn time_accel_zero_pauses_the_simulation() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::SelectBus {
        bus_id: "cubesat".into(),
    })
    .unwrap();
    sim.apply_action(PlayerAction::Launch).unwrap();
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 0.0 })
        .unwrap();

    let sim_seconds_before = sim.clock.sim_seconds;
    let cash_before = sim.company.cash_usd;
    for _ in 0..100 {
        sim.step(1.0);
    }
    assert_eq!(
        sim.clock.sim_seconds, sim_seconds_before,
        "paused sim must not advance"
    );
    assert_eq!(
        sim.company.cash_usd, cash_before,
        "paused sim must not burn cash"
    );
}

#[test]
fn sandbox_never_reaches_an_outcome() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::SetGameMode {
        mode: "sandbox".into(),
    })
    .unwrap();
    sim.company.cash_usd = -100_000.0;
    advance_days(&mut sim, 3.0);
    assert_eq!(sim.company.outcome, None);
    assert_eq!(sim.company.cash_usd, -100_000.0, "sandbox has no ops cost");
}

//! M22 agency prestige scoring and contested wins.

use satelliteinc_core::{
    SimState, action::PlayerAction, prestige::PrestigeRank,
};

mod common;
use common::load_content;

#[test]
fn prestige_starts_at_startup() {
    let sim = SimState::new(load_content()).unwrap();
    let p = sim.snapshot().prestige;
    assert_eq!(p.rank, PrestigeRank::Startup);
    // Starting cash + rep contribute to score, but stay below regional.
    assert!(p.score < 100, "fresh company should still be Startup, got {}", p.score);
    assert!(!p.goals.is_empty());
}

#[test]
fn completing_contracts_raises_score() {
    let mut sim = SimState::new(load_content()).unwrap();
    for _ in 0..5 {
        sim.company.completed_contracts.push("earth_imaging".into());
    }
    sim.company.reputation = 40.0;
    let p = sim.snapshot().prestige;
    assert!(p.score >= 75, "score {p:?}");
    assert!(p.goals.iter().any(|g| g.id == "first_contract" && g.done));
}

#[test]
fn contested_accept_increments_wins() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.company.cash_usd = 2_000_000.0;
    sim.company.reputation = 50.0;
    sim.market.rivals[0].next_action_sim_seconds = 0.0;
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 1.0 })
        .unwrap();
    sim.step(0.016);
    let _ = sim.drain_events();
    let id = sim
        .snapshot()
        .marketplace
        .iter()
        .find(|o| o.contested_by.is_some())
        .unwrap()
        .template_id
        .clone();
    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: id,
        assigned_craft_id: None,
    })
    .unwrap();
    assert_eq!(sim.company.contested_wins, 1);
    assert!(sim.snapshot().prestige.contested_wins >= 1);
}

#[test]
fn victory_bonus_applied() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.company.outcome = Some(satelliteinc_core::company::GameOutcome::Victory);
    let p = sim.snapshot().prestige;
    assert!(p.score >= 250);
    assert!(
        p.goals
            .iter()
            .any(|g| g.id == "grand_victory" && g.done)
    );
}

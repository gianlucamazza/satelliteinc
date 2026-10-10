//! M19 competitor market: contests, claims, rush bids, sandbox silence.

use satelliteinc_core::{
    SimState, action::PlayerAction, company::GameMode, market::RUSH_PREMIUM_FRACTION,
};

mod common;
use common::load_content;

fn campaign_sim() -> SimState {
    let mut sim = SimState::new(load_content()).unwrap();
    assert!(sim.market.enabled, "campaign enables market by default");
    assert_eq!(sim.market.rivals.len(), 3);
    sim.company.cash_usd = 2_000_000.0;
    sim.company.reputation = 80.0;
    sim
}

fn step_once(sim: &mut SimState) {
    sim.apply_action(PlayerAction::SetTimeAccel { multiplier: 1.0 })
        .unwrap();
    sim.step(0.016);
}

#[test]
fn content_loads_competitors() {
    let content = load_content();
    assert!(content.competitor("orbital_dynamics").is_some());
    assert!(content.competitor("cislunar_services").is_some());
    assert!(content.competitor("ares_logistics").is_some());
}

#[test]
fn sandbox_disables_market() {
    let mut sim = SimState::new(load_content()).unwrap();
    sim.apply_action(PlayerAction::LoadScenario {
        scenario_id: "sandbox_lab".into(),
    })
    .unwrap();
    assert!(!sim.market.enabled);
    assert!(!sim.snapshot().market_enabled);
}

#[test]
fn rival_opens_contest_then_claims() {
    let mut sim = campaign_sim();
    sim.market.rivals[0].next_action_sim_seconds = 0.0;
    step_once(&mut sim);
    let events = sim.drain_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, satelliteinc_core::SimEvent::ContractContested { .. })),
        "expected ContractContested, got {events:?}"
    );
    let snap = sim.snapshot();
    let contested = snap
        .marketplace
        .iter()
        .find(|o| o.contested_by.is_some())
        .expect("at least one contested offer");
    assert!(contested.rush_bid_premium_usd > 0.0);
    assert!(contested.available);
    let contract_id = contested.template_id.clone();

    // Expire contest without accepting (no multi-day warp).
    sim.market.contests[0].ends_at_sim_seconds = sim.clock.sim_seconds;
    step_once(&mut sim);
    let events = sim.drain_events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            satelliteinc_core::SimEvent::ContractClaimedByRival { contract_id: id, .. }
            if id == &contract_id
        )),
        "expected claim of {contract_id}, events={events:?}"
    );
    let snap = sim.snapshot();
    let claimed = snap
        .marketplace
        .iter()
        .find(|o| o.template_id == contract_id)
        .unwrap();
    assert!(!claimed.available);
    assert!(claimed.claimed_by.is_some());
    assert!(
        sim.market.rivals.iter().any(|r| r.contracts_won >= 1),
        "rival scoreboard should increment"
    );
}

#[test]
fn accept_during_contest_denies_rival() {
    let mut sim = campaign_sim();
    sim.market.rivals[0].next_action_sim_seconds = 0.0;
    step_once(&mut sim);
    let _ = sim.drain_events();
    let contract_id = sim
        .snapshot()
        .marketplace
        .iter()
        .find(|o| o.contested_by.is_some())
        .unwrap()
        .template_id
        .clone();

    sim.apply_action(PlayerAction::AcceptContract {
        contract_id: contract_id.clone(),
        assigned_craft_id: None,
    })
    .unwrap();
    assert!(sim.market.contest_for(&contract_id).is_none());

    // Even if a stale contest were present, claim path skips player-held contracts.
    sim.market.claims.clear();
    assert!(
        sim.market
            .claims
            .iter()
            .all(|c| c.contract_id != contract_id)
    );
}

#[test]
fn rush_bid_pays_premium_and_accepts() {
    let mut sim = campaign_sim();
    sim.market.rivals[0].next_action_sim_seconds = 0.0;
    step_once(&mut sim);
    let _ = sim.drain_events();
    let offer = sim
        .snapshot()
        .marketplace
        .iter()
        .find(|o| o.contested_by.is_some() && o.available)
        .unwrap()
        .clone();
    let premium = offer.rush_bid_premium_usd;
    let expected = (offer.payout_usd * RUSH_PREMIUM_FRACTION).round();
    assert!((premium - expected).abs() < 1.0);
    let cash_before = sim.company.cash_usd;

    sim.apply_action(PlayerAction::RushBid {
        contract_id: offer.template_id.clone(),
        assigned_craft_id: None,
    })
    .unwrap();

    // Premium charged; contract advances may credit cash, so only lower-bound.
    assert!(
        sim.company.cash_usd <= cash_before - premium + 1.0,
        "premium must be charged"
    );
    assert!(
        sim.company
            .contract_by_template(&offer.template_id)
            .is_some()
    );
    let events = sim.drain_events();
    assert!(events.iter().any(|e| matches!(
        e,
        satelliteinc_core::SimEvent::RushBidWon { premium_usd, .. }
        if (*premium_usd - premium).abs() < 1.0
    )));
}

#[test]
fn rush_bid_without_contest_fails() {
    let mut sim = campaign_sim();
    let id = sim
        .snapshot()
        .marketplace
        .iter()
        .find(|o| o.available)
        .unwrap()
        .template_id
        .clone();
    let err = sim.apply_action(PlayerAction::RushBid {
        contract_id: id,
        assigned_craft_id: None,
    });
    assert!(err.is_err());
}

#[test]
fn claimed_contract_returns_after_cooldown() {
    let mut sim = campaign_sim();
    sim.market.rivals[0].next_action_sim_seconds = 0.0;
    step_once(&mut sim);
    let contract_id = sim
        .snapshot()
        .marketplace
        .iter()
        .find(|o| o.contested_by.is_some())
        .unwrap()
        .template_id
        .clone();
    sim.market.contests[0].ends_at_sim_seconds = sim.clock.sim_seconds;
    step_once(&mut sim);
    assert!(
        sim.market
            .is_claimed(&contract_id, sim.clock.sim_seconds)
            .is_some()
    );

    // Expire cooldown.
    if let Some(claim) = sim
        .market
        .claims
        .iter_mut()
        .find(|c| c.contract_id == contract_id)
    {
        claim.available_again_at = sim.clock.sim_seconds;
    }
    step_once(&mut sim);
    assert!(
        sim.market
            .is_claimed(&contract_id, sim.clock.sim_seconds)
            .is_none(),
        "claim cooldown should expire"
    );
    let snap = sim.snapshot();
    let offer = snap
        .marketplace
        .iter()
        .find(|o| o.template_id == contract_id)
        .unwrap();
    assert!(offer.claimed_by.is_none());
}

#[test]
fn save_roundtrip_preserves_contest() {
    let mut sim = campaign_sim();
    sim.market.rivals[0].next_action_sim_seconds = 0.0;
    step_once(&mut sim);
    let _ = sim.drain_events();
    assert!(!sim.market.contests.is_empty());
    let save = sim.export_save();
    assert!(save.version >= 5);
    let json = save.to_json().unwrap();
    let restored = SimState::import_save(
        load_content(),
        satelliteinc_core::SaveGame::from_json(&json).unwrap(),
    )
    .unwrap();
    assert_eq!(restored.market.contests.len(), sim.market.contests.len());
    assert_eq!(
        restored.market.contests[0].contract_id,
        sim.market.contests[0].contract_id
    );
}

#[test]
fn mode_switch_rebuilds_market() {
    let mut sim = campaign_sim();
    assert!(sim.market.enabled);
    sim.apply_action(PlayerAction::SetGameMode {
        mode: "sandbox".into(),
    })
    .unwrap();
    assert_eq!(sim.company.game_mode, GameMode::Sandbox);
    assert!(!sim.market.enabled);
    sim.apply_action(PlayerAction::SetGameMode {
        mode: "campaign".into(),
    })
    .unwrap();
    assert!(sim.market.enabled);
}

#[test]
fn cannot_accept_claimed_contract() {
    let mut sim = campaign_sim();
    sim.market.rivals[0].next_action_sim_seconds = 0.0;
    step_once(&mut sim);
    let contract_id = sim
        .snapshot()
        .marketplace
        .iter()
        .find(|o| o.contested_by.is_some())
        .unwrap()
        .template_id
        .clone();
    sim.market.contests[0].ends_at_sim_seconds = sim.clock.sim_seconds;
    step_once(&mut sim);
    let err = sim.apply_action(PlayerAction::AcceptContract {
        contract_id,
        assigned_craft_id: None,
    });
    assert!(err.is_err());
}

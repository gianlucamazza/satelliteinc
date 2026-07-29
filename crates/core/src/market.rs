//! Competitor market (M19): deterministic rival contests and claims.
//!
//! Rivals periodically contest open marketplace offers. If the player does
//! not accept before the contest ends, the rival claims the contract for a
//! cooldown period. A rush bid pays a premium to snatch a contested offer
//! immediately (accept + premium fee).

use serde::{Deserialize, Serialize};

use crate::company::{ContractOffer, GameMode};
use crate::constants::SECONDS_PER_DAY;
use crate::content::{CompetitorDef, GameContent};

/// How long a contest stays open before the rival claims.
pub const CONTEST_DAYS: f64 = 2.0;
/// After a rival claim, the offer reappears after this many sim-days.
pub const CLAIM_COOLDOWN_DAYS: f64 = 10.0;
/// Rush-bid premium as a fraction of the offer's current payout.
pub const RUSH_PREMIUM_FRACTION: f64 = 0.15;
/// Base days between rival actions at aggressiveness = 1.0.
pub const BASE_ACTION_INTERVAL_DAYS: f64 = 3.5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RivalState {
    pub id: String,
    pub contracts_won: u32,
    pub next_action_sim_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contest {
    pub contract_id: String,
    pub competitor_id: String,
    pub ends_at_sim_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RivalClaim {
    pub contract_id: String,
    pub competitor_id: String,
    pub available_again_at: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MarketState {
    /// When false (typical sandbox lab), rivals are frozen.
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub rivals: Vec<RivalState>,
    #[serde(default)]
    pub contests: Vec<Contest>,
    #[serde(default)]
    pub claims: Vec<RivalClaim>,
}

fn default_true() -> bool {
    true
}

impl MarketState {
    /// Seed rivals from content. Stagger first actions so they don't all
    /// fire on the same tick (stable order by competitor id).
    #[must_use]
    pub fn from_content(content: &GameContent, enabled: bool, now: f64) -> Self {
        let mut defs = content.competitors.clone();
        defs.sort_by(|a, b| a.id.cmp(&b.id));
        let rivals: Vec<RivalState> = defs
            .iter()
            .enumerate()
            .map(|(i, def)| {
                let interval = action_interval_seconds(def);
                RivalState {
                    id: def.id.clone(),
                    contracts_won: 0,
                    // Stagger by half-interval * index so early game is readable.
                    next_action_sim_seconds: now + interval * (0.35 + 0.15 * i as f64),
                }
            })
            .collect();
        Self {
            enabled: enabled && !rivals.is_empty(),
            rivals,
            contests: Vec::new(),
            claims: Vec::new(),
        }
    }

    /// Re-sync rival roster after content reload / save import without wiping
    /// wins or timers for known ids.
    pub fn reconcile(&mut self, content: &GameContent, now: f64) {
        let mut defs = content.competitors.clone();
        defs.sort_by(|a, b| a.id.cmp(&b.id));
        let mut next = Vec::with_capacity(defs.len());
        for (i, def) in defs.iter().enumerate() {
            if let Some(existing) = self.rivals.iter().find(|r| r.id == def.id) {
                next.push(existing.clone());
            } else {
                let interval = action_interval_seconds(def);
                next.push(RivalState {
                    id: def.id.clone(),
                    contracts_won: 0,
                    next_action_sim_seconds: now + interval * (0.35 + 0.15 * i as f64),
                });
            }
        }
        self.rivals = next;
        // Drop contests/claims for contracts that vanished from content.
        self.contests
            .retain(|c| content.contract(&c.contract_id).is_some());
        self.claims
            .retain(|c| content.contract(&c.contract_id).is_some());
        if self.rivals.is_empty() {
            self.enabled = false;
        }
    }

    #[must_use]
    pub fn is_claimed(&self, contract_id: &str, now: f64) -> Option<&RivalClaim> {
        self.claims
            .iter()
            .find(|c| c.contract_id == contract_id && c.available_again_at > now)
    }

    #[must_use]
    pub fn contest_for(&self, contract_id: &str) -> Option<&Contest> {
        self.contests.iter().find(|c| c.contract_id == contract_id)
    }

    /// Annotate marketplace offers with contest / claim / rush-bid fields.
    pub fn decorate_offers(
        &self,
        offers: &mut [ContractOffer],
        content: &GameContent,
        now: f64,
    ) {
        for offer in offers.iter_mut() {
            offer.contested_by = None;
            offer.contest_ends_sim_seconds = None;
            offer.claimed_by = None;
            offer.rush_bid_premium_usd = 0.0;

            if let Some(claim) = self.is_claimed(&offer.template_id, now) {
                let name = content
                    .competitor(&claim.competitor_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| claim.competitor_id.clone());
                offer.claimed_by = Some(name);
                offer.available = false;
                continue;
            }

            if let Some(contest) = self.contest_for(&offer.template_id) {
                let name = content
                    .competitor(&contest.competitor_id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| contest.competitor_id.clone());
                offer.contested_by = Some(name);
                offer.contest_ends_sim_seconds = Some(contest.ends_at_sim_seconds);
                if offer.available {
                    offer.rush_bid_premium_usd =
                        (offer.payout_usd * RUSH_PREMIUM_FRACTION).round();
                }
            }
        }
    }
}

#[must_use]
pub fn action_interval_seconds(def: &CompetitorDef) -> f64 {
    action_interval_seconds_scaled(def, 1.0)
}

/// Rival action interval with career pressure (`scale` from [`career_pressure_interval_scale`]).
#[must_use]
pub fn action_interval_seconds_scaled(def: &CompetitorDef, career_interval_scale: f64) -> f64 {
    let agg = def.aggressiveness.clamp(0.25, 2.0);
    let scale = career_interval_scale.clamp(0.5, 2.0);
    (BASE_ACTION_INTERVAL_DAYS / agg) * SECONDS_PER_DAY * scale
}

#[must_use]
pub fn contest_duration_seconds() -> f64 {
    CONTEST_DAYS * SECONDS_PER_DAY
}

#[must_use]
pub fn claim_cooldown_seconds() -> f64 {
    CLAIM_COOLDOWN_DAYS * SECONDS_PER_DAY
}

/// Whether market pressure is on for this mode / scenario override.
#[must_use]
pub fn market_enabled_for(mode: GameMode, scenario_override: Option<bool>) -> bool {
    if let Some(flag) = scenario_override {
        return flag;
    }
    matches!(mode, GameMode::Campaign)
}

/// Snapshot DTO for the HUD rival strip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompetitorSnapshot {
    pub id: String,
    pub name: String,
    pub contracts_won: u32,
    pub aggressiveness: f64,
    pub focus_bodies: Vec<String>,
    pub next_action_sim_seconds: f64,
}

//! Agency prestige (M22): score and rank from campaign progress.
//! Pure functions — no side effects; snapshot and victory UI read these.

use serde::{Deserialize, Serialize};

use crate::company::CompanyState;
use crate::content::GameContent;
use crate::market::MarketState;
use crate::spacecraft::Spacecraft;

/// Named ladder shown in the Company tab and endscreen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrestigeRank {
    Startup,
    RegionalOperator,
    NationalAgency,
    InterplanetaryAgency,
    GrandAgency,
}

impl PrestigeRank {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Startup => "Startup",
            Self::RegionalOperator => "Regional Operator",
            Self::NationalAgency => "National Agency",
            Self::InterplanetaryAgency => "Interplanetary Agency",
            Self::GrandAgency => "Grand Agency",
        }
    }

    #[must_use]
    pub fn from_score(score: u32) -> Self {
        match score {
            0..=99 => Self::Startup,
            100..=299 => Self::RegionalOperator,
            300..=599 => Self::NationalAgency,
            600..=999 => Self::InterplanetaryAgency,
            _ => Self::GrandAgency,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrestigeSnapshot {
    pub score: u32,
    pub rank: PrestigeRank,
    pub rank_label: String,
    pub contracts_completed: u32,
    pub programs_completed: u32,
    pub contested_wins: u32,
    pub lifetime_revenue_usd: f64,
    /// Next rank threshold (None at top).
    pub next_rank_at: Option<u32>,
    /// Career path milestones for the HUD board.
    pub goals: Vec<PrestigeGoal>,
}

impl Default for PrestigeSnapshot {
    fn default() -> Self {
        Self {
            score: 0,
            rank: PrestigeRank::Startup,
            rank_label: PrestigeRank::Startup.label().to_string(),
            contracts_completed: 0,
            programs_completed: 0,
            contested_wins: 0,
            lifetime_revenue_usd: 0.0,
            next_rank_at: Some(100),
            goals: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrestigeGoal {
    pub id: String,
    pub label: String,
    pub done: bool,
}

/// Compute prestige from company + fleet + market state.
#[must_use]
pub fn compute_prestige(
    company: &CompanyState,
    fleet: &[Spacecraft],
    market: &MarketState,
    content: &GameContent,
) -> PrestigeSnapshot {
    let contracts_completed = company.completed_contracts.len() as u32;
    let programs_completed = company.completed_programs.len() as u32;
    let contested_wins = company.contested_wins;
    let lifetime_revenue_usd: f64 = fleet.iter().map(|c| c.lifetime_revenue_usd).sum();
    let rival_wins: u32 = market.rivals.iter().map(|r| r.contracts_won).sum();

    // Weighted score — transparent so players can optimize.
    let mut score = 0u32;
    score = score.saturating_add(contracts_completed.saturating_mul(15));
    score = score.saturating_add(programs_completed.saturating_mul(120));
    score = score.saturating_add(contested_wins.saturating_mul(25));
    score = score.saturating_add((company.reputation.max(0.0) as u32).saturating_mul(2));
    score = score.saturating_add((company.cash_usd.max(0.0) / 8_000.0) as u32);
    score = score.saturating_add((lifetime_revenue_usd.max(0.0) / 120_000.0) as u32);
    // Slight penalty for rivals that stole work (encourages contest response).
    score = score.saturating_sub(rival_wins.saturating_mul(8));

    if company.outcome == Some(crate::company::GameOutcome::Victory) {
        score = score.saturating_add(250);
    }

    let rank = PrestigeRank::from_score(score);
    let next_rank_at = match rank {
        PrestigeRank::Startup => Some(100),
        PrestigeRank::RegionalOperator => Some(300),
        PrestigeRank::NationalAgency => Some(600),
        PrestigeRank::InterplanetaryAgency => Some(1000),
        PrestigeRank::GrandAgency => None,
    };

    let goals = career_goals(company, content, contested_wins);

    PrestigeSnapshot {
        score,
        rank,
        rank_label: rank.label().to_string(),
        contracts_completed,
        programs_completed,
        contested_wins,
        lifetime_revenue_usd,
        next_rank_at,
        goals,
    }
}

/// Single career map: Earth → deep space → grand victory.
fn career_goals(
    company: &CompanyState,
    content: &GameContent,
    contested_wins: u32,
) -> Vec<PrestigeGoal> {
    let earth_done = company.completed_programs.iter().any(|p| p == "earth_program");
    let moon_unlocked = company.unlocked_programs.iter().any(|p| p == "moon_program");
    let mars_done = company.completed_programs.iter().any(|p| p == "mars_program");
    let programs_done = company.completed_programs.len();
    let total_programs = content.programs.len();

    vec![
        PrestigeGoal {
            id: "first_contract".into(),
            label: "Complete first contract".into(),
            done: !company.completed_contracts.is_empty(),
        },
        PrestigeGoal {
            id: "earth_program".into(),
            label: "Complete Earth Program".into(),
            done: earth_done,
        },
        PrestigeGoal {
            id: "moon_unlock".into(),
            label: "Unlock Moon Program".into(),
            done: moon_unlocked,
        },
        PrestigeGoal {
            id: "mars_program".into(),
            label: "Complete Mars Program".into(),
            done: mars_done,
        },
        PrestigeGoal {
            id: "snatch_contest".into(),
            label: "Win a contested offer".into(),
            done: contested_wins > 0,
        },
        PrestigeGoal {
            id: "deep_space".into(),
            label: "Complete 3 programs".into(),
            done: programs_done >= 3,
        },
        PrestigeGoal {
            id: "grand_victory".into(),
            label: if total_programs > 0 {
                format!("Grand victory (all {total_programs} programs)")
            } else {
                "Grand victory (all programs)".into()
            },
            done: company.outcome == Some(crate::company::GameOutcome::Victory)
                || company.all_contracts_completed(content),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_ladder() {
        assert_eq!(PrestigeRank::from_score(0), PrestigeRank::Startup);
        assert_eq!(PrestigeRank::from_score(100), PrestigeRank::RegionalOperator);
        assert_eq!(PrestigeRank::from_score(1000), PrestigeRank::GrandAgency);
    }
}

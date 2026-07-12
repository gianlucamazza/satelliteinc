use serde::{Deserialize, Serialize};

use crate::content::{ContractTemplate, TechNode};

/// Payout fraction for repeating an already-completed campaign contract.
pub const REPEAT_PAYOUT_FRACTION: f64 = 0.6;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GameMode {
    Sandbox,
    Campaign,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractOffer {
    pub template_id: String,
    pub name: String,
    pub description: String,
    pub downlink_mb: f64,
    pub deadline_days: f64,
    pub payout_usd: f64,
    pub reputation_required: f64,
    pub min_distinct_stations: u32,
    pub min_altitude_km: Option<f64>,
    pub max_altitude_km: Option<f64>,
    pub min_inclination_deg: Option<f64>,
    /// Body whose orbit fulfills this contract.
    pub body_id: String,
    pub available: bool,
    /// Campaign only: already completed once — repeatable at reduced payout, no reputation.
    pub repeat: bool,
}

impl ContractOffer {
    #[must_use]
    pub fn from_template(
        template: &ContractTemplate,
        reputation: f64,
        completed: &[String],
        mode: GameMode,
    ) -> Self {
        let rep_ok = match mode {
            GameMode::Sandbox => true,
            GameMode::Campaign => reputation >= template.reputation_required,
        };
        let repeat = mode == GameMode::Campaign && completed.contains(&template.id);
        let payout_usd = if repeat {
            template.payout_usd * REPEAT_PAYOUT_FRACTION
        } else {
            template.payout_usd
        };
        Self {
            template_id: template.id.clone(),
            name: template.name.clone(),
            description: template.description.clone(),
            downlink_mb: template.downlink_mb,
            deadline_days: template.deadline_days,
            payout_usd,
            reputation_required: template.reputation_required,
            min_distinct_stations: template.min_distinct_stations,
            min_altitude_km: template.min_altitude_km,
            max_altitude_km: template.max_altitude_km,
            min_inclination_deg: template.min_inclination_deg,
            body_id: template.body_id.clone(),
            available: rep_ok,
            repeat,
        }
    }
}

pub fn can_purchase_tech(
    tech: &TechNode,
    unlocked: &[String],
    cash: f64,
) -> Result<(), &'static str> {
    if unlocked.contains(&tech.id) {
        return Err("already unlocked");
    }
    for req in &tech.requires {
        if !unlocked.contains(req) {
            return Err("missing prerequisite");
        }
    }
    if cash < tech.cost_usd {
        return Err("insufficient funds");
    }
    Ok(())
}

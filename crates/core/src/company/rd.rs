use serde::{Deserialize, Serialize};

use crate::content::{ContractTemplate, TechNode};

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
    pub available: bool,
}

impl ContractOffer {
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
        let repeat_ok = match mode {
            GameMode::Sandbox => true,
            GameMode::Campaign => !completed.contains(&template.id),
        };
        let available = rep_ok && repeat_ok;
        Self {
            template_id: template.id.clone(),
            name: template.name.clone(),
            description: template.description.clone(),
            downlink_mb: template.downlink_mb,
            deadline_days: template.deadline_days,
            payout_usd: template.payout_usd,
            reputation_required: template.reputation_required,
            min_distinct_stations: template.min_distinct_stations,
            available,
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

mod rd;

pub use rd::{can_purchase_tech, ContractOffer, GameMode};

use serde::{Deserialize, Serialize};

use crate::constants::SECONDS_PER_DAY;
use crate::content::{ContractTemplate, GameContent, TechNode};

/// A contract in `CompanyState::active_contract` is active by definition:
/// completion or failure drops it (see `SimState::check_contract`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveContract {
    pub template_id: String,
    pub name: String,
    pub downlink_mb_required: f64,
    pub downlink_mb_done: f64,
    pub deadline_sim_seconds: f64,
    pub payout_usd: f64,
    pub reputation_reward: f64,
    pub min_distinct_stations: u32,
    pub stations_used: Vec<String>,
}

impl ActiveContract {
    pub fn from_template(template: &ContractTemplate, accepted_at_sim_seconds: f64) -> Self {
        Self {
            template_id: template.id.clone(),
            name: template.name.clone(),
            downlink_mb_required: template.downlink_mb,
            downlink_mb_done: 0.0,
            deadline_sim_seconds: accepted_at_sim_seconds
                + template.deadline_days * SECONDS_PER_DAY,
            payout_usd: template.payout_usd,
            reputation_reward: template.reputation_reward,
            min_distinct_stations: template.min_distinct_stations,
            stations_used: Vec::new(),
        }
    }

    pub fn progress(&self) -> f64 {
        if self.downlink_mb_required <= 0.0 {
            return 1.0;
        }
        (self.downlink_mb_done / self.downlink_mb_required).clamp(0.0, 1.0)
    }

    pub fn stations_progress(&self) -> f64 {
        if self.min_distinct_stations == 0 {
            return 1.0;
        }
        (self.stations_used.len() as f64 / self.min_distinct_stations as f64).clamp(0.0, 1.0)
    }

    pub fn is_complete(&self) -> bool {
        self.downlink_mb_done >= self.downlink_mb_required
            && (self.min_distinct_stations == 0
                || self.stations_used.len() >= self.min_distinct_stations as usize)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyState {
    pub name: String,
    pub cash_usd: f64,
    pub reputation: f64,
    pub game_mode: GameMode,
    pub unlocked_tech: Vec<String>,
    pub completed_contracts: Vec<String>,
    pub active_contract: Option<ActiveContract>,
    pub comms_multiplier: f64,
    pub solar_multiplier: f64,
    pub unlocked_buses: Vec<String>,
}

impl CompanyState {
    pub fn new(name: &str, starting_cash: f64, game_mode: GameMode, content: &GameContent) -> Self {
        let mut company = Self {
            name: name.to_string(),
            cash_usd: starting_cash,
            reputation: 10.0,
            game_mode,
            unlocked_tech: Vec::new(),
            completed_contracts: Vec::new(),
            active_contract: None,
            comms_multiplier: 1.0,
            solar_multiplier: 1.0,
            unlocked_buses: Vec::new(),
        };
        company.apply_mode_buses(content);
        company
    }

    pub fn apply_mode_buses(&mut self, content: &GameContent) {
        match self.game_mode {
            GameMode::Sandbox => {
                self.unlocked_buses = content.buses.iter().map(|b| b.id.clone()).collect();
            }
            GameMode::Campaign => {
                self.unlocked_buses = content
                    .buses
                    .iter()
                    .filter(|b| b.unlocked_by_default)
                    .map(|b| b.id.clone())
                    .collect();
                for tech_id in &self.unlocked_tech.clone() {
                    if let Some(tech) = content.tech(tech_id) {
                        if let Some(bus_id) = &tech.unlocks_bus {
                            if !self.unlocked_buses.contains(bus_id) {
                                self.unlocked_buses.push(bus_id.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn failure_rep_penalty(&self) -> f64 {
        match self.game_mode {
            GameMode::Campaign => 10.0,
            GameMode::Sandbox => 5.0,
        }
    }

    pub fn can_afford(&self, cost: f64) -> bool {
        self.cash_usd >= cost
    }

    pub fn marketplace(&self, content: &GameContent) -> Vec<ContractOffer> {
        content
            .contracts
            .iter()
            .map(|c| {
                ContractOffer::from_template(
                    c,
                    self.reputation,
                    &self.completed_contracts,
                    self.game_mode,
                )
            })
            .collect()
    }

    pub fn apply_tech(&mut self, tech: &TechNode) {
        if !self.unlocked_tech.contains(&tech.id) {
            self.unlocked_tech.push(tech.id.clone());
        }
        self.comms_multiplier *= tech.comms_multiplier;
        self.solar_multiplier *= tech.solar_multiplier;
        if let Some(bus_id) = &tech.unlocks_bus {
            if !self.unlocked_buses.contains(bus_id) {
                self.unlocked_buses.push(bus_id.clone());
            }
        }
    }

    pub fn on_mode_changed(&mut self, content: &GameContent) {
        self.apply_mode_buses(content);
    }

    pub fn is_bus_unlocked(&self, bus_id: &str) -> bool {
        self.unlocked_buses.iter().any(|b| b == bus_id)
    }
}

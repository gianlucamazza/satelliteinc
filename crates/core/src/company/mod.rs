mod rd;

pub use rd::{ContractOffer, GameMode, REPEAT_PAYOUT_FRACTION, can_purchase_tech};

use serde::{Deserialize, Serialize};

use crate::constants::SECONDS_PER_DAY;
use crate::content::{ContractTemplate, GameContent, TechNode};
use crate::orbit::KeplerElements;

/// Terminal state of a campaign. Sandbox never sets one.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GameOutcome {
    Victory,
    Bankrupt,
    ReputationLost,
}

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
    /// Body whose orbit fulfills this contract.
    #[serde(default = "crate::content::default_body_id")]
    pub body_id: String,
    #[serde(default)]
    pub min_altitude_km: Option<f64>,
    #[serde(default)]
    pub max_altitude_km: Option<f64>,
    #[serde(default)]
    pub min_inclination_deg: Option<f64>,
}

impl ActiveContract {
    #[must_use]
    pub fn from_template(
        template: &ContractTemplate,
        accepted_at_sim_seconds: f64,
        is_repeat: bool,
    ) -> Self {
        let (payout_usd, reputation_reward) = if is_repeat {
            (template.payout_usd * REPEAT_PAYOUT_FRACTION, 0.0)
        } else {
            (template.payout_usd, template.reputation_reward)
        };
        Self {
            template_id: template.id.clone(),
            name: template.name.clone(),
            downlink_mb_required: template.downlink_mb,
            downlink_mb_done: 0.0,
            deadline_sim_seconds: accepted_at_sim_seconds
                + template.deadline_days * SECONDS_PER_DAY,
            payout_usd,
            reputation_reward,
            min_distinct_stations: template.min_distinct_stations,
            stations_used: Vec::new(),
            body_id: template.body_id.clone(),
            min_altitude_km: template.min_altitude_km,
            max_altitude_km: template.max_altitude_km,
            min_inclination_deg: template.min_inclination_deg,
        }
    }

    /// Whether the current orbit satisfies this contract's requirements.
    #[must_use]
    pub fn orbit_ok(&self, elements: &KeplerElements, body_radius_km: f64) -> bool {
        let altitude_km = elements.semi_major_axis_km - body_radius_km;
        if self.min_altitude_km.is_some_and(|min| altitude_km < min) {
            return false;
        }
        if self.max_altitude_km.is_some_and(|max| altitude_km > max) {
            return false;
        }
        if self
            .min_inclination_deg
            .is_some_and(|min| elements.inclination_deg < min)
        {
            return false;
        }
        true
    }

    #[must_use]
    pub fn progress(&self) -> f64 {
        if self.downlink_mb_required <= 0.0 {
            return 1.0;
        }
        (self.downlink_mb_done / self.downlink_mb_required).clamp(0.0, 1.0)
    }

    #[must_use]
    pub fn stations_progress(&self) -> f64 {
        if self.min_distinct_stations == 0 {
            return 1.0;
        }
        (self.stations_used.len() as f64 / f64::from(self.min_distinct_stations)).clamp(0.0, 1.0)
    }

    #[must_use]
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
    #[serde(default = "default_one")]
    pub isp_multiplier: f64,
    #[serde(default = "default_one")]
    pub anomaly_mtbf_multiplier: f64,
    #[serde(default = "default_one")]
    pub ops_cost_multiplier: f64,
    #[serde(default = "default_one")]
    pub deep_space_comms_multiplier: f64,
    #[serde(default = "default_one")]
    pub capture_dv_multiplier: f64,
    pub unlocked_buses: Vec<String>,
    /// Terminal campaign state; None while the game is live.
    #[serde(default)]
    pub outcome: Option<GameOutcome>,
}

fn default_one() -> f64 {
    1.0
}

impl CompanyState {
    #[must_use]
    pub fn new(name: &str, starting_cash: f64, game_mode: GameMode, content: &GameContent) -> Self {
        let mut company = Self {
            name: name.to_string(),
            cash_usd: starting_cash,
            reputation: 20.0,
            game_mode,
            unlocked_tech: Vec::new(),
            completed_contracts: Vec::new(),
            active_contract: None,
            comms_multiplier: 1.0,
            solar_multiplier: 1.0,
            isp_multiplier: 1.0,
            anomaly_mtbf_multiplier: 1.0,
            ops_cost_multiplier: 1.0,
            deep_space_comms_multiplier: 1.0,
            capture_dv_multiplier: 1.0,
            unlocked_buses: Vec::new(),
            outcome: None,
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
                    if let Some(tech) = content.tech(tech_id)
                        && let Some(bus_id) = &tech.unlocks_bus
                        && !self.unlocked_buses.contains(bus_id)
                    {
                        self.unlocked_buses.push(bus_id.clone());
                    }
                }
            }
        }
    }

    #[must_use]
    pub fn failure_rep_penalty(&self) -> f64 {
        match self.game_mode {
            GameMode::Campaign => 10.0,
            GameMode::Sandbox => 5.0,
        }
    }

    #[must_use]
    pub fn can_afford(&self, cost: f64) -> bool {
        self.cash_usd >= cost
    }

    #[must_use]
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
        self.isp_multiplier *= tech.isp_multiplier;
        self.anomaly_mtbf_multiplier *= tech.anomaly_mtbf_multiplier;
        self.ops_cost_multiplier *= tech.ops_cost_multiplier;
        self.deep_space_comms_multiplier *= tech.deep_space_comms_multiplier;
        self.capture_dv_multiplier *= tech.capture_dv_multiplier;
        if let Some(bus_id) = &tech.unlocks_bus
            && !self.unlocked_buses.contains(bus_id)
        {
            self.unlocked_buses.push(bus_id.clone());
        }
    }

    pub fn on_mode_changed(&mut self, content: &GameContent) {
        self.apply_mode_buses(content);
        if self.game_mode == GameMode::Sandbox {
            // Sandbox has no end states: switching to it clears any game over.
            self.outcome = None;
        }
    }

    /// Campaign victory condition: every contract template completed at least once.
    #[must_use]
    pub fn all_contracts_completed(&self, content: &GameContent) -> bool {
        // Victory counts Earth contracts only until interplanetary Programs
        // land (M12): off-world contracts are unreachable in campaign while
        // transfers don't exist and must not make it unwinnable.
        content
            .contracts
            .iter()
            .filter(|c| c.body_id == "earth")
            .all(|c| self.completed_contracts.contains(&c.id))
    }

    #[must_use]
    pub fn is_bus_unlocked(&self, bus_id: &str) -> bool {
        self.unlocked_buses.iter().any(|b| b == bus_id)
    }
}

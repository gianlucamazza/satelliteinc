use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameContent {
    pub buses: Vec<BusTemplate>,
    pub ground_stations: Vec<GroundStationDef>,
    pub contracts: Vec<ContractTemplate>,
    pub tech_tree: Vec<TechNode>,
    pub tle_catalog: Vec<crate::orbit::TleEntry>,
    #[serde(default)]
    pub scenarios: Vec<crate::scenario::ScenarioTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusTemplate {
    pub id: String,
    pub name: String,
    pub mass_kg: f64,
    pub solar_power_w: f64,
    pub battery_wh: f64,
    pub comms_mbps: f64,
    /// Radio transmit power at full downlink (watts).
    pub comms_power_w: f64,
    pub payload_power_w: f64,
    pub launch_cost_usd: f64,
    pub propellant_kg: f64,
    pub isp_s: f64,
    #[serde(default = "default_unlocked")]
    pub unlocked_by_default: bool,
}

fn default_unlocked() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundStationDef {
    pub id: String,
    pub name: String,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub min_elevation_deg: f64,
    #[serde(default = "default_bandwidth")]
    pub bandwidth_mbps: f64,
}

fn default_bandwidth() -> f64 {
    50.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub downlink_mb: f64,
    pub deadline_days: f64,
    pub payout_usd: f64,
    #[serde(default)]
    pub reputation_required: f64,
    #[serde(default)]
    pub reputation_reward: f64,
    /// Minimum distinct ground stations that must receive downlink (0 = no requirement).
    #[serde(default)]
    pub min_distinct_stations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechNode {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cost_usd: f64,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub unlocks_bus: Option<String>,
    #[serde(default = "default_one")]
    pub comms_multiplier: f64,
    #[serde(default = "default_one")]
    pub solar_multiplier: f64,
}

fn default_one() -> f64 {
    1.0
}

impl GameContent {
    pub fn from_json(json: &str) -> Result<Self, crate::error::SimError> {
        let content: Self = serde_json::from_str(json)
            .map_err(|e| crate::error::SimError::Content(format!("content parse: {e}")))?;
        content.validate()?;
        Ok(content)
    }

    /// Reject content whose physical parameters would produce NaN/inf in the
    /// sim (e.g. Δv capacity) or whose cross-references dangle.
    fn validate(&self) -> Result<(), crate::error::SimError> {
        use crate::error::SimError;
        for bus in &self.buses {
            if bus.mass_kg <= 0.0
                || bus.isp_s <= 0.0
                || bus.battery_wh <= 0.0
                || bus.propellant_kg < 0.0
                || bus.comms_mbps <= 0.0
                || bus.comms_power_w < 0.0
                || bus.solar_power_w <= 0.0
            {
                return Err(SimError::Content(format!(
                    "bus '{}' has non-physical parameters",
                    bus.id
                )));
            }
        }
        for tech in &self.tech_tree {
            for req in &tech.requires {
                if self.tech(req).is_none() {
                    return Err(SimError::Content(format!(
                        "tech '{}' requires unknown tech '{req}'",
                        tech.id
                    )));
                }
            }
            if let Some(bus_id) = &tech.unlocks_bus {
                if self.bus(bus_id).is_none() {
                    return Err(SimError::Content(format!(
                        "tech '{}' unlocks unknown bus '{bus_id}'",
                        tech.id
                    )));
                }
            }
        }
        for scenario in &self.scenarios {
            if self.bus(&scenario.bus_id).is_none() {
                return Err(SimError::Content(format!(
                    "scenario '{}' references unknown bus '{}'",
                    scenario.id, scenario.bus_id
                )));
            }
            if let Some(contract_id) = &scenario.contract_id {
                if self.contract(contract_id).is_none() {
                    return Err(SimError::Content(format!(
                        "scenario '{}' references unknown contract '{contract_id}'",
                        scenario.id
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn bus(&self, id: &str) -> Option<&BusTemplate> {
        self.buses.iter().find(|b| b.id == id)
    }

    pub fn contract(&self, id: &str) -> Option<&ContractTemplate> {
        self.contracts.iter().find(|c| c.id == id)
    }

    pub fn tech(&self, id: &str) -> Option<&TechNode> {
        self.tech_tree.iter().find(|t| t.id == id)
    }

    pub fn scenario(&self, id: &str) -> Option<&crate::scenario::ScenarioTemplate> {
        self.scenarios.iter().find(|s| s.id == id)
    }
}

use serde::{Deserialize, Serialize};

use crate::SimState;
use crate::company::{ActiveContract, CompanyState, GameMode};
use crate::error::SimError;
use crate::spacecraft::Spacecraft;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub game_mode: GameMode,
    pub starting_cash: f64,
    pub starting_reputation: f64,
    pub bus_id: String,
    /// Body the scenario's spacecraft operates around.
    #[serde(default = "crate::content::default_body_id")]
    pub body_id: String,
    #[serde(default)]
    pub contract_id: Option<String>,
    #[serde(default = "default_alt")]
    pub altitude_km: f64,
    #[serde(default = "default_inc")]
    pub inclination_deg: f64,
}

fn default_alt() -> f64 {
    400.0
}

fn default_inc() -> f64 {
    97.0
}

impl SimState {
    pub fn apply_scenario(&mut self, scenario_id: &str) -> Result<(), SimError> {
        let scenario = self
            .content
            .scenario(scenario_id)
            .ok_or_else(|| SimError::Content(format!("unknown scenario: {scenario_id}")))?
            .clone();

        // Resolve everything before mutating so a bad scenario leaves the
        // sim untouched.
        let bus = self
            .content
            .bus(&scenario.bus_id)
            .ok_or_else(|| SimError::Content(format!("unknown bus: {}", scenario.bus_id)))?
            .clone();
        let contract_template = match &scenario.contract_id {
            Some(contract_id) => Some(
                self.content
                    .contract(contract_id)
                    .ok_or_else(|| SimError::Content(format!("unknown contract: {contract_id}")))?
                    .clone(),
            ),
            None => None,
        };

        self.clock = crate::clock::SimClock::new();
        self.pass_active.fill(false);
        self.event_queue.clear();

        let mut company = CompanyState::new(
            "Satellite Inc",
            scenario.starting_cash,
            scenario.game_mode,
            &self.content,
        );
        company.reputation = scenario.starting_reputation;
        self.company = company;

        let mut craft = Spacecraft::new_from_bus(&bus, &bus.name);
        craft.body_id.clone_from(&scenario.body_id);
        craft.target_altitude_km = scenario.altitude_km;
        craft.target_inclination_deg = scenario.inclination_deg;
        self.spacecraft = Some(craft);

        // Scenario-seeded contracts bypass marketplace availability gates
        // (reputation), unlike player-accepted ones.
        if let Some(template) = contract_template {
            self.company.active_contract = Some(ActiveContract::from_template(
                &template,
                self.clock.sim_seconds,
                false,
            ));
        }

        Ok(())
    }
}

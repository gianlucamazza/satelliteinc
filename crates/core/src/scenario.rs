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
    /// Tech already researched at scenario start (no cost).
    #[serde(default)]
    pub starting_tech: Vec<String>,
    /// Programs already bought into at scenario start (no cost).
    #[serde(default)]
    pub starting_programs: Vec<String>,
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
        self.fleet.clear();
        self.pass_active.clear();
        self.next_craft_id = 0;
        self.event_queue.clear();

        let mut company = CompanyState::new(
            "Satellite Inc",
            scenario.starting_cash,
            scenario.game_mode,
            &self.content,
        );
        company.reputation = scenario.starting_reputation;
        for tech_id in &scenario.starting_tech {
            if let Some(tech) = self.content.tech(tech_id) {
                company.apply_tech(tech);
            }
        }
        for program_id in &scenario.starting_programs {
            if !company.unlocked_programs.contains(program_id) {
                company.unlocked_programs.push(program_id.clone());
            }
        }
        self.company = company;

        let mut craft = Spacecraft::new_from_bus(&bus, &bus.name, 0);
        craft.body_id.clone_from(&scenario.body_id);
        craft.target_altitude_km = scenario.altitude_km;
        craft.target_inclination_deg = scenario.inclination_deg;
        self.add_craft(craft);

        // Scenario-seeded contracts bypass marketplace availability gates
        // (reputation), unlike player-accepted ones.
        if let Some(template) = contract_template {
            let mut contract = ActiveContract::from_template(&template, self.clock.sim_seconds, 0);
            // The scenario craft services its seeded contract.
            contract.assigned_craft_id = Some(0);
            self.company.active_contracts.push(contract);
        }

        Ok(())
    }
}

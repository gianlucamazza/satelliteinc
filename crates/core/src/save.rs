use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::clock::SimClock;
use crate::company::CompanyState;
use crate::content::GameContent;
use crate::error::SimError;
use crate::ops::GroundStation;
use crate::orbit::TlePropagator;
use crate::spacecraft::Spacecraft;
use crate::SimState;

pub const SAVE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveGame {
    pub version: u32,
    pub clock: SimClock,
    pub company: CompanyState,
    pub spacecraft: Option<Spacecraft>,
    pub pass_active: HashMap<String, bool>,
}

impl SaveGame {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl SimState {
    pub fn export_save(&self) -> SaveGame {
        SaveGame {
            version: SAVE_VERSION,
            clock: self.clock,
            company: self.company.clone(),
            spacecraft: self.spacecraft.clone(),
            pass_active: self.pass_active.clone(),
        }
    }

    pub fn import_save(content: GameContent, save: SaveGame) -> Result<Self, SimError> {
        if save.version != SAVE_VERSION {
            // Single format so far. When SAVE_VERSION bumps, migrate older
            // versions here instead of rejecting them.
            return Err(SimError::InvalidAction(format!(
                "unsupported save version: {}",
                save.version
            )));
        }
        let ground_stations: Vec<GroundStation> = content
            .ground_stations
            .iter()
            .map(GroundStation::from)
            .collect();
        let tle_propagators: Vec<TlePropagator> = content
            .tle_catalog
            .iter()
            .filter_map(|e| TlePropagator::from_entry(e).ok())
            .collect();
        // Reconcile against the provided content: an active contract whose
        // template no longer exists cannot be tracked in the marketplace.
        let mut company = save.company;
        if let Some(active) = &company.active_contract {
            if content.contract(&active.template_id).is_none() {
                company.active_contract = None;
            }
        }
        Ok(Self {
            clock: save.clock,
            company,
            spacecraft: save.spacecraft,
            ground_stations,
            content,
            event_queue: Vec::new(),
            tle_propagators,
            pass_active: save.pass_active,
            ref_sat_cache: std::cell::RefCell::new(None),
        })
    }
}

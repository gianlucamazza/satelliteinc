use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::SimState;
use crate::clock::SimClock;
use crate::company::CompanyState;
use crate::content::GameContent;
use crate::error::SimError;
use crate::ops::GroundStation;
use crate::orbit::TlePropagator;
use crate::spacecraft::Spacecraft;

pub const SAVE_VERSION: u32 = 3;

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
    #[must_use]
    pub fn export_save(&self) -> SaveGame {
        // The save keys pass state by station id so the format survives
        // station reordering between content versions.
        let pass_active: HashMap<String, bool> = self
            .ground_stations
            .iter()
            .zip(&self.pass_active)
            .map(|(s, &active)| (s.id.clone(), active))
            .collect();
        SaveGame {
            version: SAVE_VERSION,
            clock: self.clock,
            company: self.company.clone(),
            spacecraft: self.spacecraft.clone(),
            pass_active,
        }
    }

    pub fn import_save(content: GameContent, save: SaveGame) -> Result<Self, SimError> {
        if save.version > SAVE_VERSION {
            return Err(SimError::InvalidAction(format!(
                "unsupported save version: {}",
                save.version
            )));
        }
        // v2 -> v3: pre-solar-system saves carry no body ids; serde defaults
        // already filled `body_id: "earth"` during deserialization.
        let bodies = crate::body::BodyRegistry::from_content(&content.bodies)?;
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
        if let Some(active) = &company.active_contract
            && content.contract(&active.template_id).is_none()
        {
            company.active_contract = None;
        }
        // A craft around a body the content no longer defines cannot be
        // propagated; reject rather than silently relocate it.
        if let Some(craft) = &save.spacecraft
            && bodies.get(&craft.body_id).is_none()
        {
            return Err(SimError::Content(format!(
                "save references unknown body '{}'",
                craft.body_id
            )));
        }
        let pass_active: Vec<bool> = ground_stations
            .iter()
            .map(|s| save.pass_active.get(&s.id).copied().unwrap_or(false))
            .collect();
        Ok(Self {
            clock: save.clock,
            company,
            spacecraft: save.spacecraft,
            bodies,
            pass_active,
            ground_stations,
            content,
            event_queue: Vec::new(),
            tle_propagators,
            ref_sat_cache: std::cell::RefCell::new(None),
        })
    }
}

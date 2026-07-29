use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::SimState;
use crate::clock::SimClock;
use crate::company::CompanyState;
use crate::content::GameContent;
use crate::error::SimError;
use crate::market::{MarketState, market_enabled_for};
use crate::ops::GroundStation;
use crate::orbit::TlePropagator;
use crate::spacecraft::{CraftId, Spacecraft};
use crate::weather::WeatherState;

pub const SAVE_VERSION: u32 = 6;

/// One craft's pass-edge state, keyed by station id so the format survives
/// station reordering between content versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPassState {
    pub craft_id: CraftId,
    pub stations: HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveGame {
    pub version: u32,
    pub clock: SimClock,
    pub company: CompanyState,
    pub spacecraft: Vec<Spacecraft>,
    pub next_craft_id: CraftId,
    pub pass_active: Vec<SavedPassState>,
    /// Rival market (v5+; defaults for older saves).
    #[serde(default)]
    pub market: MarketState,
    /// Space weather (v6+; defaults for older saves).
    #[serde(default)]
    pub weather: WeatherState,
}

impl SaveGame {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let mut v: Value = serde_json::from_str(json)?;
        let ver = v.get("version").and_then(Value::as_u64).unwrap_or(0);
        if ver <= 3 {
            migrate_v3(&mut v);
        }
        if ver <= 4 {
            migrate_v4_to_v5(&mut v);
        }
        if ver <= 5 {
            migrate_v5_to_v6(&mut v);
        }
        serde_json::from_value(v)
    }
}

/// v4 → v5: add market state (empty; import rebuilds from content + mode).
fn migrate_v4_to_v5(v: &mut Value) {
    if v.get("market").is_none() {
        v["market"] = serde_json::json!({
            "enabled": true,
            "rivals": [],
            "contests": [],
            "claims": []
        });
    }
    v["version"] = Value::from(5u32);
}

/// v5 → v6: add weather state.
fn migrate_v5_to_v6(v: &mut Value) {
    if v.get("weather").is_none() {
        v["weather"] = serde_json::json!({
            "enabled": true,
            "active": null,
            "next_roll_sim_seconds": 0.0
        });
    }
    v["version"] = Value::from(6u32);
}

/// v3 -> v4: the fleet turned every craft-shaped singleton into a
/// collection (a TYPE change, so serde defaults cannot cover it). The old
/// craft becomes fleet member id 0; its contract keeps servicing it.
fn migrate_v3(v: &mut Value) {
    let had_craft = v.get("spacecraft").is_some_and(Value::is_object);

    // spacecraft: null → [] | {..} → [{.., "id": 0}]
    let fleet = match v.get_mut("spacecraft").map(Value::take) {
        Some(Value::Object(mut craft)) => {
            craft.insert("id".into(), Value::from(0u32));
            Value::Array(vec![Value::Object(craft)])
        }
        _ => Value::Array(vec![]),
    };
    v["spacecraft"] = fleet;

    // company.active_contract: null → active_contracts: []
    //   | {..} → [{.., assigned_craft_id: 0 when a craft existed}]
    if let Some(company) = v.get_mut("company").and_then(Value::as_object_mut) {
        let contracts = match company.remove("active_contract") {
            Some(Value::Object(mut contract)) => {
                if had_craft {
                    contract.insert("assigned_craft_id".into(), Value::from(0u32));
                }
                Value::Array(vec![Value::Object(contract)])
            }
            _ => Value::Array(vec![]),
        };
        company.insert("active_contracts".into(), contracts);
    }

    // pass_active {station: bool} → [{craft_id: 0, stations: {..}}]
    let stations = v.get_mut("pass_active").map(Value::take);
    v["pass_active"] = match stations {
        Some(map @ Value::Object(_)) if had_craft => Value::Array(vec![serde_json::json!({
            "craft_id": 0,
            "stations": map,
        })]),
        _ => Value::Array(vec![]),
    };

    v["next_craft_id"] = Value::from(u32::from(had_craft));
    v["version"] = Value::from(4u32);
}

impl SimState {
    #[must_use]
    pub fn export_save(&self) -> SaveGame {
        let pass_active: Vec<SavedPassState> = self
            .fleet
            .iter()
            .zip(&self.pass_active)
            .map(|(craft, row)| SavedPassState {
                craft_id: craft.id,
                stations: self
                    .ground_stations
                    .iter()
                    .zip(row)
                    .map(|(s, &active)| (s.id.clone(), active))
                    .collect(),
            })
            .collect();
        SaveGame {
            version: SAVE_VERSION,
            clock: self.clock,
            company: self.company.clone(),
            spacecraft: self.fleet.clone(),
            next_craft_id: self.next_craft_id,
            pass_active,
            market: self.market.clone(),
            weather: self.weather.clone(),
        }
    }

    pub fn import_save(content: GameContent, save: SaveGame) -> Result<Self, SimError> {
        if save.version > SAVE_VERSION {
            return Err(SimError::InvalidAction(format!(
                "unsupported save version: {}",
                save.version
            )));
        }
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

        // A craft around a body the content no longer defines cannot be
        // propagated; reject rather than silently relocate it.
        for craft in &save.spacecraft {
            if bodies.get(&craft.body_id).is_none() {
                return Err(SimError::Content(format!(
                    "save references unknown body '{}'",
                    craft.body_id
                )));
            }
        }

        // Reconcile against the provided content: drop contracts whose
        // template vanished; clear assignments to craft that don't exist.
        let mut company = save.company;
        company
            .active_contracts
            .retain(|c| content.contract(&c.template_id).is_some());
        for contract in &mut company.active_contracts {
            if let Some(id) = contract.assigned_craft_id
                && !save.spacecraft.iter().any(|c| c.id == id)
            {
                contract.assigned_craft_id = None;
            }
        }

        // Rebuild the pass matrix in fleet order; rows for unknown craft are
        // dropped, missing rows default to no active edges.
        let pass_active: Vec<Vec<bool>> = save
            .spacecraft
            .iter()
            .map(|craft| {
                let saved = save.pass_active.iter().find(|p| p.craft_id == craft.id);
                ground_stations
                    .iter()
                    .map(|s| {
                        saved
                            .and_then(|p| p.stations.get(&s.id).copied())
                            .unwrap_or(false)
                    })
                    .collect()
            })
            .collect();

        let next_craft_id = save
            .spacecraft
            .iter()
            .map(|c| c.id + 1)
            .max()
            .unwrap_or(0)
            .max(save.next_craft_id);

        let mut market = save.market;
        if market.rivals.is_empty() {
            // Pre-v5 or empty: seed from content using mode defaults.
            market = MarketState::from_content(
                &content,
                market_enabled_for(company.game_mode, None),
                save.clock.sim_seconds,
            );
        } else {
            market.reconcile(&content, save.clock.sim_seconds);
        }

        let mut weather = save.weather;
        // Fresh / migrated saves: seed schedule from content + mode.
        if weather.next_roll_sim_seconds <= 0.0 && weather.active.is_none() {
            weather = WeatherState::from_content(
                &content,
                WeatherState::weather_enabled_for(company.game_mode, None),
                save.clock.sim_seconds,
            );
        } else if content.world_events.is_empty() {
            weather.enabled = false;
            weather.active = None;
        }

        Ok(Self {
            clock: save.clock,
            company,
            fleet: save.spacecraft,
            next_craft_id,
            bodies,
            pass_active,
            ground_stations,
            content,
            event_queue: Vec::new(),
            tle_propagators,
            ref_sat_cache: std::cell::RefCell::new(None),
            market,
            weather,
        })
    }
}

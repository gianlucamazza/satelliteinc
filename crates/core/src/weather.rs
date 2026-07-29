//! World events / space weather (M20).
//!
//! Deterministic schedule: no RNG. Every `CHECK_INTERVAL_DAYS` the sim may
//! start a content-defined event (weighted pick from a day bucket hash).
//! Effects stack as multipliers on solar power, anomaly rate, and bandwidth.

use serde::{Deserialize, Serialize};

use crate::company::GameMode;
use crate::constants::SECONDS_PER_DAY;
use crate::content::{GameContent, WorldEventDef};

/// Days between weather rolls when idle (no active event).
pub const CHECK_INTERVAL_DAYS: f64 = 3.5;
/// Quiet period after an event ends before the next roll.
pub const COOLDOWN_DAYS: f64 = 1.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveWorldEvent {
    pub event_id: String,
    pub started_at_sim_seconds: f64,
    pub ends_at_sim_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WeatherState {
    /// Campaign on by default; sandbox quiet unless scenario enables it.
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub active: Option<ActiveWorldEvent>,
    /// Next time we attempt to start an event (when idle).
    #[serde(default)]
    pub next_roll_sim_seconds: f64,
}

fn default_true() -> bool {
    true
}

impl WeatherState {
    #[must_use]
    pub fn from_content(content: &GameContent, enabled: bool, now: f64) -> Self {
        Self {
            enabled: enabled && !content.world_events.is_empty(),
            active: None,
            next_roll_sim_seconds: now + CHECK_INTERVAL_DAYS * SECONDS_PER_DAY * 0.6,
        }
    }

    #[must_use]
    pub fn weather_enabled_for(mode: GameMode, scenario_override: Option<bool>) -> bool {
        if let Some(flag) = scenario_override {
            return flag;
        }
        matches!(mode, GameMode::Campaign)
    }

    /// Multipliers while an event is active and applies to `body_id`.
    #[must_use]
    pub fn modifiers(
        &self,
        content: &GameContent,
        body_id: &str,
        station_is_dsn: bool,
    ) -> WeatherModifiers {
        let Some(active) = &self.active else {
            return WeatherModifiers::identity();
        };
        let Some(def) = content.world_event(&active.event_id) else {
            return WeatherModifiers::identity();
        };
        if !def.applies_to_body(body_id) {
            return WeatherModifiers::identity();
        }
        if def.dsn_only && !station_is_dsn {
            // DSN-only events still hit solar/anomaly for craft off-Earth when
            // body matches; bandwidth only for DSN stations.
            return WeatherModifiers {
                solar_penalty: def.solar_penalty,
                anomaly_rate_multiplier: def.anomaly_rate_multiplier,
                bandwidth_multiplier: 1.0,
            };
        }
        WeatherModifiers {
            solar_penalty: def.solar_penalty,
            anomaly_rate_multiplier: def.anomaly_rate_multiplier,
            bandwidth_multiplier: def.bandwidth_multiplier,
        }
    }

    #[must_use]
    pub fn craft_modifiers(&self, content: &GameContent, body_id: &str) -> WeatherModifiers {
        // Craft-wide: use non-DSN path for solar/anomaly; bandwidth is station-scoped.
        self.modifiers(content, body_id, false)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WeatherModifiers {
    /// Multiplies solar panel output (1.0 = normal, 0.5 = half power).
    pub solar_penalty: f64,
    /// Multiplies anomaly rate (higher = more failures).
    pub anomaly_rate_multiplier: f64,
    /// Multiplies link bandwidth.
    pub bandwidth_multiplier: f64,
}

impl WeatherModifiers {
    #[must_use]
    pub fn identity() -> Self {
        Self {
            solar_penalty: 1.0,
            anomaly_rate_multiplier: 1.0,
            bandwidth_multiplier: 1.0,
        }
    }
}

/// Deterministic weighted pick from content using a day-bucket index.
#[must_use]
pub fn pick_event<'a>(defs: &'a [WorldEventDef], day_bucket: u64) -> Option<&'a WorldEventDef> {
    if defs.is_empty() {
        return None;
    }
    let total: u64 = defs.iter().map(|d| u64::from(d.weight.max(1))).sum();
    if total == 0 {
        return defs.first();
    }
    let mut slot = day_bucket.wrapping_mul(31).wrapping_add(17) % total;
    for def in defs {
        let w = u64::from(def.weight.max(1));
        if slot < w {
            return Some(def);
        }
        slot -= w;
    }
    defs.first()
}

/// Snapshot DTO for the HUD banner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEventSnapshot {
    pub event_id: String,
    pub name: String,
    pub description: String,
    pub ends_at_sim_seconds: f64,
    pub body_ids: Vec<String>,
    pub solar_penalty: f64,
    pub anomaly_rate_multiplier: f64,
    pub bandwidth_multiplier: f64,
    pub dsn_only: bool,
}

impl WorldEventSnapshot {
    #[must_use]
    pub fn from_active(def: &WorldEventDef, ends_at: f64) -> Self {
        Self {
            event_id: def.id.clone(),
            name: def.name.clone(),
            description: def.description.clone(),
            ends_at_sim_seconds: ends_at,
            body_ids: def.body_ids.clone(),
            solar_penalty: def.solar_penalty,
            anomaly_rate_multiplier: def.anomaly_rate_multiplier,
            bandwidth_multiplier: def.bandwidth_multiplier,
            dsn_only: def.dsn_only,
        }
    }
}

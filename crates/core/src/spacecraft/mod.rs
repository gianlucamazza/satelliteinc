mod maneuver;

pub use maneuver::{HohmannPlanSnapshot, PendingHohmann};

use serde::{Deserialize, Serialize};

use crate::content::BusTemplate;
use crate::ops::{ActiveAnomaly, AnomalyType, QueuedCommand};
use crate::orbit::{KeplerElements, delta_v_capacity_km_s};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CraftPhase {
    Design,
    OnOrbit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PayloadMode {
    Imaging,
    Safe,
    DownlinkPriority,
}

fn default_body_id() -> String {
    "earth".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spacecraft {
    pub name: String,
    pub bus_id: String,
    pub bus: BusTemplate,
    pub phase: CraftPhase,
    /// Central body whose BCI frame `elements` are expressed in.
    #[serde(default = "default_body_id")]
    pub body_id: String,
    pub elements: Option<KeplerElements>,
    pub power_wh: f64,
    pub buffer_mb: f64,
    pub downlink_active: bool,
    pub target_altitude_km: f64,
    pub target_inclination_deg: f64,
    pub maneuver_target_altitude_km: f64,
    pub propellant_kg: f64,
    pub total_delta_v_used_km_s: f64,
    pub pending_hohmann: Option<PendingHohmann>,
    pub payload_mode: PayloadMode,
    /// Mode to restore when a commanded safe-mode hold expires.
    #[serde(default)]
    pub payload_mode_before_safe: Option<PayloadMode>,
    pub downlink_station_id: Option<String>,
    pub command_queue: Vec<QueuedCommand>,
    pub active_anomaly: Option<ActiveAnomaly>,
    pub anomaly_cooldown_until: f64,
    pub safe_mode_until: f64,
}

impl Spacecraft {
    #[must_use]
    pub fn new_from_bus(bus: &BusTemplate, name: &str) -> Self {
        Self {
            name: name.to_string(),
            bus_id: bus.id.clone(),
            bus: bus.clone(),
            phase: CraftPhase::Design,
            body_id: default_body_id(),
            elements: None,
            power_wh: bus.battery_wh,
            buffer_mb: 0.0,
            downlink_active: false,
            target_altitude_km: 400.0,
            target_inclination_deg: 51.6,
            maneuver_target_altitude_km: 550.0,
            propellant_kg: bus.propellant_kg,
            total_delta_v_used_km_s: 0.0,
            pending_hohmann: None,
            payload_mode: PayloadMode::Imaging,
            payload_mode_before_safe: None,
            downlink_station_id: None,
            command_queue: Vec::new(),
            active_anomaly: None,
            anomaly_cooldown_until: 0.0,
            safe_mode_until: 0.0,
        }
    }

    #[must_use]
    pub fn max_buffer_mb(&self) -> f64 {
        2_048.0
    }

    #[must_use]
    pub fn dry_mass_kg(&self) -> f64 {
        self.bus.mass_kg
    }

    #[must_use]
    pub fn delta_v_remaining_km_s(&self, isp_mult: f64) -> f64 {
        delta_v_capacity_km_s(
            self.dry_mass_kg(),
            self.propellant_kg,
            self.bus.isp_s * isp_mult,
        )
    }

    #[must_use]
    pub fn current_altitude_km(&self, body_radius_km: f64) -> Option<f64> {
        self.elements.map(|e| e.semi_major_axis_km - body_radius_km)
    }

    #[must_use]
    pub fn in_safe_mode(&self, sim_seconds: f64) -> bool {
        self.payload_mode == PayloadMode::Safe || sim_seconds < self.safe_mode_until
    }

    /// `anomaly_solar_penalty` is the per-body flavor multiplier applied to
    /// solar output while an anomaly is active and unsafed (1.0 = none).
    #[must_use]
    pub fn power_balance_w(
        &self,
        in_sunlight: bool,
        solar_mult: f64,
        sim_seconds: f64,
        anomaly_solar_penalty: f64,
    ) -> f64 {
        let mut solar = if in_sunlight {
            self.bus.solar_power_w * solar_mult
        } else {
            0.0
        };
        if let Some(a) = &self.active_anomaly
            && !a.is_expired(sim_seconds)
            && !self.in_safe_mode(sim_seconds)
        {
            solar *= anomaly_solar_penalty;
        }
        if let Some(a) = &self.active_anomaly
            && a.kind == AnomalyType::PowerGlitch
            && !a.is_expired(sim_seconds)
        {
            solar *= 0.5;
        }

        let (mut payload, comms) = if self.in_safe_mode(sim_seconds) {
            (self.bus.payload_power_w * 0.1, 0.0)
        } else {
            match self.payload_mode {
                PayloadMode::Imaging => {
                    let payload = if self.downlink_active {
                        self.bus.payload_power_w
                    } else {
                        self.bus.payload_power_w * 0.2
                    };
                    // Radio shares power with the imaging payload.
                    let comms = if self.downlink_active {
                        self.bus.comms_power_w * 0.6
                    } else {
                        0.0
                    };
                    (payload, comms)
                }
                PayloadMode::Safe => (self.bus.payload_power_w * 0.1, 0.0),
                PayloadMode::DownlinkPriority => {
                    let comms = if self.downlink_active {
                        self.bus.comms_power_w
                    } else {
                        0.0
                    };
                    (self.bus.payload_power_w * 0.15, comms)
                }
            }
        };
        // An unmanaged anomaly stresses the bus; safing shields the draw.
        if let Some(a) = &self.active_anomaly
            && !a.is_expired(sim_seconds)
            && !self.in_safe_mode(sim_seconds)
        {
            payload *= 1.2;
        }
        solar - payload - comms
    }

    #[must_use]
    pub fn downlink_rate_mb_s(
        &self,
        comms_mult: f64,
        station_bandwidth_mbps: Option<f64>,
        sim_seconds: f64,
    ) -> f64 {
        if !self.downlink_active {
            return 0.0;
        }
        if let Some(a) = &self.active_anomaly
            && a.kind == AnomalyType::CommsDropout
            && !a.is_expired(sim_seconds)
        {
            return 0.0;
        }
        // The ground station caps the link after tech multipliers, so
        // station choice stays meaningful post-R&D.
        let mut rate_mbps = self.bus.comms_mbps * comms_mult;
        if let Some(bandwidth) = station_bandwidth_mbps {
            rate_mbps = rate_mbps.min(bandwidth);
        }
        rate_mbps / 8.0
    }

    #[must_use]
    pub fn imaging_rate_mb_s(&self, in_sunlight: bool, sim_seconds: f64) -> f64 {
        if self.phase != CraftPhase::OnOrbit || !in_sunlight || self.in_safe_mode(sim_seconds) {
            return 0.0;
        }
        match self.payload_mode {
            PayloadMode::Imaging => {
                if self.downlink_active {
                    1.0
                } else {
                    5.0
                }
            }
            PayloadMode::DownlinkPriority => 0.5,
            PayloadMode::Safe => 0.0,
        }
    }

    pub fn clear_anomaly(&mut self, sim_seconds: f64) {
        self.active_anomaly = None;
        self.anomaly_cooldown_until = sim_seconds + crate::ops::ANOMALY_COOLDOWN_SECONDS;
    }
}

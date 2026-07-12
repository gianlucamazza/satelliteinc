use serde::{Deserialize, Serialize};

use crate::company::ActiveContract;
use crate::company::{ContractOffer, GameMode, GameOutcome};
use crate::ops::{AnomalyType, QueuedCommand};
use crate::orbit::{KeplerElements, ReferenceSatellite};
use crate::spacecraft::{CraftPhase, HohmannPlanSnapshot, PayloadMode, PendingHohmann};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Celestial bodies with positions at the current sim time.
    pub bodies: Vec<BodySnapshot>,
    /// UTC instant that sim_seconds = 0 represents.
    pub epoch_utc: String,
    pub sim_seconds: f64,
    pub sim_days: f64,
    pub time_accel: f64,
    pub tick: u64,
    pub company_name: String,
    pub cash_usd: f64,
    pub reputation: f64,
    pub game_mode: GameMode,
    pub unlocked_tech: Vec<String>,
    pub unlocked_buses: Vec<String>,
    pub marketplace: Vec<ContractOffer>,
    pub active_contract: Option<ActiveContract>,
    /// Whether the current orbit satisfies the active contract's requirements
    /// (None when there is no contract or no orbit).
    pub active_contract_orbit_ok: Option<bool>,
    pub spacecraft: Option<SpacecraftSnapshot>,
    pub ground_stations: Vec<StationSnapshot>,
    pub reference_satellites: Vec<ReferenceSatellite>,
    pub phase: GamePhase,
    pub outcome: Option<GameOutcome>,
    /// Current recurring burn rate (base overhead + on-orbit ops cost).
    pub ops_cost_usd_per_day: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GamePhase {
    Design,
    AwaitingLaunch,
    OnOrbit,
    ContractComplete,
    Victory,
    GameOver,
}

/// A celestial body's state at snapshot time. `position_*` are km;
/// `sun_direction` is the unit vector from the body toward the sun in its
/// body-centered-inertial frame (shared axis orientation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodySnapshot {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub radius_km: f64,
    pub rotation_rad_s: f64,
    pub position_helio_km: [f64; 3],
    pub position_parent_km: [f64; 3],
    pub sun_direction: [f64; 3],
    /// `None` for the root body (infinite sphere of influence).
    pub soi_radius_km: Option<f64>,
    pub texture: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // presentation DTO mirroring sim state
pub struct SpacecraftSnapshot {
    pub name: String,
    pub bus_id: String,
    pub bus_name: String,
    pub phase: CraftPhase,
    /// Central body of `position_eci_km` / `elements` (body-relative).
    pub body_id: String,
    pub position_eci_km: [f64; 3],
    pub velocity_eci_km_s: [f64; 3],
    pub elements: Option<KeplerElements>,
    pub orbit_trail_km: Vec<[f64; 3]>,
    pub power_wh: f64,
    pub battery_wh_capacity: f64,
    pub buffer_mb: f64,
    pub buffer_capacity_mb: f64,
    pub downlink_active: bool,
    pub in_sunlight: bool,
    pub in_pass: bool,
    pub downlink_rate_mb_s: f64,
    pub imaging_rate_mb_s: f64,
    pub target_altitude_km: f64,
    pub target_inclination_deg: f64,
    pub altitude_km: f64,
    pub propellant_kg: f64,
    pub propellant_capacity_kg: f64,
    pub delta_v_remaining_km_s: f64,
    pub total_delta_v_used_km_s: f64,
    pub pending_hohmann: Option<PendingHohmann>,
    pub at_apsis: bool,
    /// ETA to the next apsis passages (None off-orbit).
    pub seconds_to_perigee: Option<f64>,
    pub seconds_to_apogee: Option<f64>,
    pub hohmann_preview: Option<HohmannPlanSnapshot>,
    pub maneuver_target_altitude_km: f64,
    pub payload_mode: PayloadMode,
    pub downlink_station_id: Option<String>,
    pub command_queue: Vec<QueuedCommand>,
    pub active_anomaly: Option<AnomalyType>,
    pub anomaly_label: Option<String>,
    /// One-way light time to Earth (flavor; 0 in Earth orbit).
    pub light_delay_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationSnapshot {
    pub id: String,
    pub name: String,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub elevation_deg: f64,
    pub in_pass: bool,
    /// Effective link bandwidth for the current craft (DSN-scaled).
    pub bandwidth_mbps: f64,
    pub next_pass_in_seconds: Option<f64>,
    pub body_id: String,
    pub dsn: bool,
}

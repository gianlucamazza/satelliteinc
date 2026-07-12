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
    /// Concurrent contracts with their per-assignment orbit compliance.
    pub active_contracts: Vec<ActiveContractSnapshot>,
    pub spacecraft: Vec<SpacecraftSnapshot>,
    pub ground_stations: Vec<StationSnapshot>,
    pub reference_satellites: Vec<ReferenceSatellite>,
    pub phase: GamePhase,
    pub outcome: Option<GameOutcome>,
    /// Current recurring burn rate (base overhead + on-orbit ops cost).
    pub ops_cost_usd_per_day: f64,
    /// Whether interplanetary transfers are available to the company.
    pub transfers_unlocked: bool,
    /// Earliest upcoming fleet event (warp-to-next-event target).
    pub next_event_sim_seconds: Option<f64>,
    /// Campaign Programs with their gate status.
    pub programs: Vec<ProgramSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // gate-status DTO for the HUD
pub struct ProgramSnapshot {
    pub id: String,
    pub name: String,
    pub body_id: String,
    pub buyin_usd: f64,
    pub reputation_required: f64,
    pub requires_keystone: Option<String>,
    pub keystone_done: bool,
    pub requires_tech: Option<String>,
    pub tech_done: bool,
    pub unlocked: bool,
    pub completed: bool,
    pub completion_grant_usd: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GamePhase {
    Design,
    AwaitingLaunch,
    OnOrbit,
    InTransit,
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

/// An active contract joined with its assigned craft's orbit compliance
/// (None when unassigned or the craft has no orbit).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveContractSnapshot {
    #[serde(flatten)]
    pub contract: ActiveContract,
    pub orbit_ok: Option<bool>,
}

/// Craft-relative station geometry (reachable stations only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftStationStatus {
    pub station_id: String,
    pub elevation_deg: f64,
    pub in_pass: bool,
    pub next_pass_in_seconds: Option<f64>,
    /// Effective link bandwidth for this craft (DSN-scaled).
    pub bandwidth_mbps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // presentation DTO mirroring sim state
pub struct SpacecraftSnapshot {
    /// Stable fleet id.
    pub id: u32,
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
    /// This craft's daily ops line (company multiplier applied).
    pub ops_cost_usd_per_day: f64,
    /// Service record: contracts completed by this craft.
    pub contracts_completed: u32,
    pub lifetime_revenue_usd: f64,
    pub lifetime_ops_usd: f64,
    pub launched_at_sim_seconds: Option<f64>,
    /// Craft-relative station geometry (reachable stations only).
    pub station_status: Vec<CraftStationStatus>,
    /// Cruise in progress (None on orbit / in design).
    pub transfer: Option<TransferSnapshot>,
    /// Planner readout for the selected destination (OnOrbit only).
    pub transfer_planner: Option<TransferPlannerSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferSnapshot {
    pub origin_body_id: String,
    pub target_body_id: String,
    pub frame_body_id: String,
    pub departure_sim_seconds: f64,
    pub arrival_sim_seconds: f64,
    /// 0..1 fraction of the cruise elapsed.
    pub progress: f64,
    pub capture_dv_km_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferPlannerSnapshot {
    pub target_body_id: String,
    pub window_open: bool,
    /// Sibling transfers only (None for a moon of the current body).
    pub phase_angle_deg: Option<f64>,
    pub required_phase_angle_deg: Option<f64>,
    pub next_window_sim_seconds: f64,
    pub transfer_seconds: f64,
    /// Physical injection burn (paid by the launch provider).
    pub departure_dv_km_s: f64,
    /// Capture burn from the craft's tanks (aerobraking applied).
    pub capture_dv_km_s: f64,
    pub capture_propellant_kg: f64,
    pub injection_cost_usd: f64,
    pub affordable: bool,
    pub propellant_ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationSnapshot {
    pub id: String,
    pub name: String,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    /// Base dish bandwidth (craft-effective rates live in station_status).
    pub bandwidth_mbps: f64,
    pub body_id: String,
    pub dsn: bool,
}

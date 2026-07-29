use serde::{Deserialize, Serialize};

use crate::company::GameOutcome;
use crate::ops::AnomalyType;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SimEvent {
    PassStarted {
        craft_id: u32,
        station_id: String,
        station_name: String,
    },
    PassEnded {
        craft_id: u32,
        station_id: String,
    },
    ContractCompleted {
        contract_id: String,
        payout_usd: f64,
    },
    ContractFailed {
        contract_id: String,
    },
    Launched {
        craft_id: u32,
        craft_name: String,
        altitude_km: f64,
        inclination_deg: f64,
    },
    InsufficientFunds {
        required_usd: f64,
    },
    ActionApplied {
        message: String,
    },
    ManeuverExecuted {
        craft_id: u32,
        delta_v_km_s: f64,
        propellant_used_kg: f64,
        new_altitude_km: Option<f64>,
        label: String,
    },
    HohmannBurn1Complete {
        craft_id: u32,
        delta_v_km_s: f64,
        target_altitude_km: f64,
    },
    HohmannTransferComplete {
        craft_id: u32,
        target_altitude_km: f64,
    },
    AnomalyStarted {
        craft_id: u32,
        kind: AnomalyType,
        label: String,
    },
    AnomalyCleared {
        craft_id: u32,
        kind: AnomalyType,
    },
    CommandExecuted {
        craft_id: u32,
        command: String,
        station_name: String,
        message: String,
    },
    TechPurchased {
        tech_id: String,
        name: String,
    },
    ReputationChanged {
        delta: f64,
        new_value: f64,
    },
    Decommissioned {
        craft_id: u32,
        craft_name: String,
        salvage_usd: f64,
    },
    TransferStarted {
        craft_id: u32,
        target_body_id: String,
        arrival_sim_seconds: f64,
        injection_cost_usd: f64,
    },
    TransferArrived {
        craft_id: u32,
        body_id: String,
    },
    CaptureExecuted {
        craft_id: u32,
        delta_v_km_s: f64,
    },
    WarpCompleted {
        seconds: f64,
    },
    ProgramUnlocked {
        program_id: String,
    },
    ProgramCompleted {
        program_id: String,
        grant_usd: f64,
    },
    ContractAssignmentChanged {
        contract_id: String,
        craft_id: Option<u32>,
    },
    /// A rival opened a contest on a marketplace offer.
    ContractContested {
        contract_id: String,
        competitor_id: String,
        competitor_name: String,
        ends_at_sim_seconds: f64,
    },
    /// Contest window elapsed; rival took the contract for a cooldown.
    ContractClaimedByRival {
        contract_id: String,
        competitor_id: String,
        competitor_name: String,
    },
    /// Player paid a rush-bid premium and accepted a contested offer.
    RushBidWon {
        contract_id: String,
        premium_usd: f64,
        competitor_id: String,
    },
    /// A global space-weather / pressure event began.
    WorldEventStarted {
        event_id: String,
        name: String,
        ends_at_sim_seconds: f64,
    },
    /// An active world event expired.
    WorldEventEnded {
        event_id: String,
        name: String,
    },
    GameWon,
    GameOver {
        reason: GameOutcome,
    },
    /// One-shot warning when cash crosses below zero.
    CashNegative {
        cash_usd: f64,
    },
}

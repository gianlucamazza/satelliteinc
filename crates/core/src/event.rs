use serde::{Deserialize, Serialize};

use crate::company::GameOutcome;
use crate::ops::AnomalyType;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SimEvent {
    PassStarted {
        station_id: String,
        station_name: String,
    },
    PassEnded {
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
        delta_v_km_s: f64,
        propellant_used_kg: f64,
        new_altitude_km: Option<f64>,
        label: String,
    },
    HohmannBurn1Complete {
        delta_v_km_s: f64,
        target_altitude_km: f64,
    },
    HohmannTransferComplete {
        target_altitude_km: f64,
    },
    AnomalyStarted {
        kind: AnomalyType,
        label: String,
    },
    AnomalyCleared {
        kind: AnomalyType,
    },
    CommandExecuted {
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
        salvage_usd: f64,
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

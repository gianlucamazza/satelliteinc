use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandType {
    RebootComms,
    SafeMode,
    ClearAnomaly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedCommand {
    pub command: CommandType,
    pub station_id: String,
    pub queued_at_sim_seconds: f64,
}

impl QueuedCommand {
    #[must_use]
    pub fn new(command: CommandType, station_id: String, sim_seconds: f64) -> Self {
        Self {
            command,
            station_id,
            queued_at_sim_seconds: sim_seconds,
        }
    }
}

/// Execute a queued command; returns human-readable result.
/// Callers refine the message when the procedure does not match the fault.
#[must_use]
pub fn execute_command(command: &CommandType) -> &'static str {
    match command {
        CommandType::RebootComms => "Comms reboot sequence complete",
        CommandType::SafeMode => "Safe mode engaged for 5 min (payload safed)",
        CommandType::ClearAnomaly => "EPS recovery / anomaly clear sequence complete",
    }
}

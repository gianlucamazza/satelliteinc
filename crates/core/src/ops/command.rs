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
    pub fn new(command: CommandType, station_id: String, sim_seconds: f64) -> Self {
        Self {
            command,
            station_id,
            queued_at_sim_seconds: sim_seconds,
        }
    }
}

/// Execute a queued command; returns human-readable result.
pub fn execute_command(command: &CommandType) -> &'static str {
    match command {
        CommandType::RebootComms => "Comms subsystem rebooted",
        CommandType::SafeMode => "Safe mode engaged for 5 min",
        CommandType::ClearAnomaly => "Anomaly cleared via ground command",
    }
}

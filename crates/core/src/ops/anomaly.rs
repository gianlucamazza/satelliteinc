use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    PowerGlitch,
    CommsDropout,
    BufferCorruption,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveAnomaly {
    pub kind: AnomalyType,
    pub started_at_sim_seconds: f64,
    pub duration_seconds: f64,
}

impl ActiveAnomaly {
    pub fn new(kind: AnomalyType, sim_seconds: f64) -> Self {
        let duration_seconds = match kind {
            AnomalyType::PowerGlitch => 120.0,
            AnomalyType::CommsDropout => 90.0,
            AnomalyType::BufferCorruption => 30.0,
        };
        Self {
            kind,
            started_at_sim_seconds: sim_seconds,
            duration_seconds,
        }
    }

    pub fn is_expired(&self, sim_seconds: f64) -> bool {
        self.duration_seconds > 0.0
            && sim_seconds > self.started_at_sim_seconds + self.duration_seconds
    }

    pub fn label(&self) -> &'static str {
        match self.kind {
            AnomalyType::PowerGlitch => "Power glitch",
            AnomalyType::CommsDropout => "Comms dropout",
            AnomalyType::BufferCorruption => "Buffer corruption",
        }
    }
}

/// Cooldown applied after an anomaly ends (natural expiry or commanded clear).
pub const ANOMALY_COOLDOWN_SECONDS: f64 = 600.0;

/// Deterministic anomaly roll from sim time: fires when a 4000 s sim-time
/// boundary is crossed during the last `dt` step. Depending only on sim time
/// keeps the schedule independent of frame cadence and time acceleration.
pub fn roll_anomaly(sim_seconds: f64, dt: f64, cooldown_until: f64) -> Option<AnomalyType> {
    if sim_seconds < cooldown_until {
        return None;
    }
    const PERIOD_SECONDS: f64 = 4_000.0;
    let bucket = (sim_seconds / PERIOD_SECONDS).floor() as i64;
    let prev_bucket = ((sim_seconds - dt) / PERIOD_SECONDS).floor() as i64;
    if bucket <= 0 || bucket == prev_bucket {
        return None;
    }
    match bucket % 3 {
        0 => Some(AnomalyType::PowerGlitch),
        1 => Some(AnomalyType::CommsDropout),
        _ => Some(AnomalyType::BufferCorruption),
    }
}

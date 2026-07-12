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
    /// Per-body flavor name (e.g. "Dust storm" on Mars); base label if None.
    #[serde(default)]
    pub label_override: Option<String>,
}

impl ActiveAnomaly {
    #[must_use]
    pub fn new(kind: AnomalyType, sim_seconds: f64) -> Self {
        // Long enough that waiting them out hurts: a queued command
        // (reboot/clear/safe) is the better play.
        let duration_seconds = match kind {
            AnomalyType::PowerGlitch => 480.0,
            AnomalyType::CommsDropout => 360.0,
            AnomalyType::BufferCorruption => 30.0,
        };
        Self {
            kind,
            started_at_sim_seconds: sim_seconds,
            duration_seconds,
            label_override: None,
        }
    }

    #[must_use]
    pub fn is_expired(&self, sim_seconds: f64) -> bool {
        self.duration_seconds > 0.0
            && sim_seconds > self.started_at_sim_seconds + self.duration_seconds
    }

    #[must_use]
    pub fn label(&self) -> &str {
        if let Some(label) = &self.label_override {
            return label;
        }
        match self.kind {
            AnomalyType::PowerGlitch => "Power glitch",
            AnomalyType::CommsDropout => "Comms dropout",
            AnomalyType::BufferCorruption => "Buffer corruption",
        }
    }
}

/// Cooldown applied after an anomaly ends (natural expiry or commanded clear).
pub const ANOMALY_COOLDOWN_SECONDS: f64 = 600.0;

/// Per-craft phase stride: with a shared sim-time schedule, every craft in
/// the fleet would suffer the SAME anomaly at the SAME tick. Offsetting by
/// craft id (a value that is not a divisor/multiple of the 4000 s base
/// period) decorrelates both the timing and the `bucket % 3` type. Craft 0
/// gets offset 0 — bit-identical to the pre-fleet schedule.
pub const ANOMALY_PHASE_STRIDE_SECONDS: f64 = 1_618.0;

/// Deterministic anomaly roll from sim time: fires when a period boundary is
/// crossed during the last `dt` step. Depending only on sim time keeps the
/// schedule independent of frame cadence and time acceleration.
/// `mtbf_multiplier` stretches the base 4000 s period (redundancy tech).
#[must_use]
pub fn roll_anomaly(
    sim_seconds: f64,
    dt: f64,
    cooldown_until: f64,
    mtbf_multiplier: f64,
    phase_offset_seconds: f64,
) -> Option<AnomalyType> {
    if sim_seconds < cooldown_until {
        return None;
    }
    let effective = sim_seconds + phase_offset_seconds;
    let period_seconds = 4_000.0 * mtbf_multiplier.max(0.01);
    let bucket = (effective / period_seconds).floor() as i64;
    let prev_bucket = ((effective - dt) / period_seconds).floor() as i64;
    if bucket <= 0 || bucket == prev_bucket {
        return None;
    }
    match bucket % 3 {
        0 => Some(AnomalyType::PowerGlitch),
        1 => Some(AnomalyType::CommsDropout),
        _ => Some(AnomalyType::BufferCorruption),
    }
}

//! On-orbit anomaly model (FDIR-oriented, not arcade).
//!
//! Three failure *archetypes* map to real recovery procedures:
//! - [`AnomalyType::PowerGlitch`] — EPS / thermal-power upset → safe mode + ground clear
//! - [`AnomalyType::CommsDropout`] — RF / modem fault → ground reboot at next pass
//! - [`AnomalyType::BufferCorruption`] — SEU / bit-flip *event* (instant data loss, not sticky)
//!
//! Schedule is deterministic from sim time (reproducible, acceleration-safe).
//! Environment (body, weather, tech MTBF, eclipse) stretches the period; body
//! weights bias which archetype fires.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    PowerGlitch,
    CommsDropout,
    /// Single-event upset: damages the buffer immediately and does **not**
    /// leave a sticky active anomaly (unlike power/comms faults).
    BufferCorruption,
}

impl AnomalyType {
    /// Sticky faults occupy `active_anomaly` until recovered or expired.
    #[must_use]
    pub fn is_sticky(self) -> bool {
        !matches!(self, Self::BufferCorruption)
    }
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
        // Sticky faults last long enough that waiting them out is costly;
        // ground FDIR (safe / reboot / clear) is the intended path.
        // Buffer corruption uses 0 — callers should treat it as an event.
        let duration_seconds = match kind {
            AnomalyType::PowerGlitch => 1_800.0,  // 30 min unmanaged
            AnomalyType::CommsDropout => 1_200.0, // 20 min unmanaged
            AnomalyType::BufferCorruption => 0.0,
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
        // Non-sticky / zero-duration entries should never latch.
        if !self.kind.is_sticky() || self.duration_seconds <= 0.0 {
            return true;
        }
        sim_seconds > self.started_at_sim_seconds + self.duration_seconds
    }

    #[must_use]
    pub fn label(&self) -> &str {
        if let Some(label) = &self.label_override {
            return label;
        }
        match self.kind {
            AnomalyType::PowerGlitch => "Power / EPS glitch",
            AnomalyType::CommsDropout => "Comms dropout",
            AnomalyType::BufferCorruption => "Buffer SEU (data loss)",
        }
    }

    /// Remaining seconds until natural expiry (0 if expired or non-sticky).
    #[must_use]
    pub fn remaining_seconds(&self, sim_seconds: f64) -> f64 {
        if self.duration_seconds <= 0.0 {
            return 0.0;
        }
        (self.started_at_sim_seconds + self.duration_seconds - sim_seconds).max(0.0)
    }
}

/// Cooldown after a fault ends (natural expiry or commanded recovery).
/// Prevents thrashing the schedule during the same ops window.
pub const ANOMALY_COOLDOWN_SECONDS: f64 = 900.0;

/// Per-craft phase stride: decorrelates fleet anomalies (craft 0 offset 0
/// preserves pre-fleet bit-identity of the schedule).
pub const ANOMALY_PHASE_STRIDE_SECONDS: f64 = 1_618.0;

/// Relative weights for which sticky/event type fires at a period boundary.
#[derive(Debug, Clone, Copy)]
pub struct AnomalyWeights {
    pub power_glitch: u32,
    pub comms_dropout: u32,
    pub buffer_corruption: u32,
}

impl Default for AnomalyWeights {
    fn default() -> Self {
        Self {
            power_glitch: 1,
            comms_dropout: 1,
            buffer_corruption: 1,
        }
    }
}

impl AnomalyWeights {
    #[must_use]
    pub fn from_body_id(body_id: &str) -> Self {
        match body_id {
            // Thermal environment → EPS/thermal stress.
            "venus" => Self {
                power_glitch: 3,
                comms_dropout: 1,
                buffer_corruption: 1,
            },
            // Dust / atmosphere → RF.
            "mars" => Self {
                power_glitch: 1,
                comms_dropout: 3,
                buffer_corruption: 1,
            },
            // Radiation belt → SEU / bit flips.
            "jupiter" => Self {
                power_glitch: 1,
                comms_dropout: 1,
                buffer_corruption: 3,
            },
            // Mascon / pointing + dust static.
            "moon" => Self {
                power_glitch: 2,
                comms_dropout: 1,
                buffer_corruption: 2,
            },
            _ => Self::default(),
        }
    }

    fn pick(self, bucket: i64) -> AnomalyType {
        let p = self.power_glitch.max(1);
        let c = self.comms_dropout.max(1);
        let b = self.buffer_corruption.max(1);
        let total = u64::from(p + c + b);
        let slot = (bucket.unsigned_abs()) % total;
        if slot < u64::from(p) {
            AnomalyType::PowerGlitch
        } else if slot < u64::from(p + c) {
            AnomalyType::CommsDropout
        } else {
            AnomalyType::BufferCorruption
        }
    }
}

/// Deterministic anomaly roll from sim time: fires when a period boundary is
/// crossed during the last `dt` step. Independent of frame cadence / accel.
///
/// `mtbf_multiplier` stretches the base period (redundancy tech, environment).
/// `weights` bias archetype selection by environment.
#[must_use]
pub fn roll_anomaly(
    sim_seconds: f64,
    dt: f64,
    cooldown_until: f64,
    mtbf_multiplier: f64,
    phase_offset_seconds: f64,
) -> Option<AnomalyType> {
    roll_anomaly_weighted(
        sim_seconds,
        dt,
        cooldown_until,
        mtbf_multiplier,
        phase_offset_seconds,
        AnomalyWeights::default(),
    )
}

#[must_use]
pub fn roll_anomaly_weighted(
    sim_seconds: f64,
    dt: f64,
    cooldown_until: f64,
    mtbf_multiplier: f64,
    phase_offset_seconds: f64,
    weights: AnomalyWeights,
) -> Option<AnomalyType> {
    if sim_seconds < cooldown_until {
        return None;
    }
    let effective = sim_seconds + phase_offset_seconds;
    // ~67 min base MTBF at 1× — stretched by tech, compressed by harsh bodies.
    let period_seconds = 4_000.0 * mtbf_multiplier.max(0.01);
    let bucket = (effective / period_seconds).floor() as i64;
    let prev_bucket = ((effective - dt) / period_seconds).floor() as i64;
    if bucket <= 0 || bucket == prev_bucket {
        return None;
    }
    Some(weights.pick(bucket))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_corruption_is_not_sticky() {
        assert!(!AnomalyType::BufferCorruption.is_sticky());
        assert!(AnomalyType::PowerGlitch.is_sticky());
        assert!(AnomalyType::CommsDropout.is_sticky());
    }

    #[test]
    fn jupiter_weights_favor_seu() {
        let w = AnomalyWeights::from_body_id("jupiter");
        let mut seu = 0;
        let mut other = 0;
        for b in 1..300 {
            match w.pick(b) {
                AnomalyType::BufferCorruption => seu += 1,
                _ => other += 1,
            }
        }
        assert!(seu > other, "seu={seu} other={other}");
    }

    #[test]
    fn sticky_durations_are_ops_relevant() {
        let p = ActiveAnomaly::new(AnomalyType::PowerGlitch, 0.0);
        let c = ActiveAnomaly::new(AnomalyType::CommsDropout, 0.0);
        assert!(p.duration_seconds >= 1_500.0);
        assert!(c.duration_seconds >= 900.0);
        assert_eq!(
            ActiveAnomaly::new(AnomalyType::BufferCorruption, 0.0).duration_seconds,
            0.0
        );
    }
}

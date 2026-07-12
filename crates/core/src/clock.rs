use serde::{Deserialize, Serialize};

use crate::constants::SECONDS_PER_DAY;

/// Simulation clock anchored to sim-seconds since mission start.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SimClock {
    pub sim_seconds: f64,
    pub time_accel: f64,
    pub tick: u64,
}

impl SimClock {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sim_seconds: 0.0,
            time_accel: 1.0,
            tick: 0,
        }
    }

    #[must_use]
    pub fn sim_days(&self) -> f64 {
        self.sim_seconds / SECONDS_PER_DAY
    }
}

impl Default for SimClock {
    fn default() -> Self {
        Self::new()
    }
}

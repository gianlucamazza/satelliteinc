use serde::{Deserialize, Serialize};

use crate::orbit::HohmannPlan;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingHohmann {
    pub plan: HohmannPlanSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HohmannPlanSnapshot {
    pub burn1_dv_km_s: f64,
    pub burn2_dv_km_s: f64,
    pub total_dv_km_s: f64,
    pub target_altitude_km: f64,
    pub raising: bool,
}

impl From<&HohmannPlan> for HohmannPlanSnapshot {
    fn from(plan: &HohmannPlan) -> Self {
        Self {
            burn1_dv_km_s: plan.burn1_dv_km_s,
            burn2_dv_km_s: plan.burn2_dv_km_s,
            total_dv_km_s: plan.total_dv_km_s,
            target_altitude_km: plan.target_altitude_km,
            raising: plan.raising,
        }
    }
}

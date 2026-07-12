use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameContent {
    /// Celestial bodies (sun-rooted tree). Defaults to sun+earth so content
    /// predating the solar system still loads.
    #[serde(default = "default_bodies")]
    pub bodies: Vec<CelestialBodyDef>,
    /// UTC instant that `sim_seconds` = 0 represents (body epochs anchor here).
    #[serde(default = "default_epoch_utc")]
    pub epoch_utc: String,
    pub buses: Vec<BusTemplate>,
    pub ground_stations: Vec<GroundStationDef>,
    pub contracts: Vec<ContractTemplate>,
    pub tech_tree: Vec<TechNode>,
    pub tle_catalog: Vec<crate::orbit::TleEntry>,
    #[serde(default)]
    pub scenarios: Vec<crate::scenario::ScenarioTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CelestialBodyDef {
    pub id: String,
    pub name: String,
    pub mu_km3_s2: f64,
    pub radius_km: f64,
    /// Sidereal rotation rate (rad/s); negative = retrograde.
    pub rotation_rad_s: f64,
    /// Parent body id; `None` only for the root ("sun").
    #[serde(default)]
    pub parent: Option<String>,
    /// Parent-relative orbit; required for non-root bodies.
    #[serde(default)]
    pub orbit: Option<BodyOrbitDef>,
    /// Sphere of influence; computed as `a·(μ/μ_parent)^(2/5)` when absent.
    #[serde(default)]
    pub soi_radius_km: Option<f64>,
    #[serde(default = "default_min_parking")]
    pub min_parking_altitude_km: f64,
    #[serde(default = "default_max_parking")]
    pub max_parking_altitude_km: f64,
    /// Renderer pass-through; physics never reads it.
    #[serde(default)]
    pub texture: Option<String>,
}

/// Parent-relative Kepler elements; mean anomaly given at sim t = 0.
/// All body frames share the heliocentric (ecliptic) axis orientation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BodyOrbitDef {
    pub semi_major_axis_km: f64,
    pub eccentricity: f64,
    pub inclination_deg: f64,
    pub raan_deg: f64,
    pub arg_periapsis_deg: f64,
    pub mean_anomaly_epoch_deg: f64,
}

fn default_min_parking() -> f64 {
    200.0
}

fn default_max_parking() -> f64 {
    2_000.0
}

fn default_epoch_utc() -> String {
    "2026-03-20T00:00:00Z".into()
}

/// Built-in sun+earth pair: earth values are the exact literals from
/// `constants.rs`, so pre-solar-system content behaves identically.
fn default_bodies() -> Vec<CelestialBodyDef> {
    vec![
        CelestialBodyDef {
            id: "sun".into(),
            name: "Sun".into(),
            mu_km3_s2: 1.327_124_400_18e11,
            radius_km: 695_700.0,
            rotation_rad_s: 2.865_329_084_571_725_6e-6,
            parent: None,
            orbit: None,
            soi_radius_km: None,
            min_parking_altitude_km: 1_000_000.0,
            max_parking_altitude_km: 10_000_000.0,
            texture: None,
        },
        CelestialBodyDef {
            id: "earth".into(),
            name: "Earth".into(),
            mu_km3_s2: crate::constants::MU_EARTH,
            radius_km: crate::constants::EARTH_RADIUS_KM,
            rotation_rad_s: crate::constants::EARTH_ROTATION_RAD_S,
            parent: Some("sun".into()),
            orbit: Some(BodyOrbitDef {
                semi_major_axis_km: 1.495_982_612e8,
                eccentricity: 0.016_711_23,
                inclination_deg: 0.0,
                raan_deg: 0.0,
                arg_periapsis_deg: 102.937_681_93,
                mean_anomaly_epoch_deg: 75.201_271_558_281_72,
            }),
            soi_radius_km: None,
            min_parking_altitude_km: 200.0,
            max_parking_altitude_km: 2_000.0,
            texture: Some("earth".into()),
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusTemplate {
    pub id: String,
    pub name: String,
    pub mass_kg: f64,
    pub solar_power_w: f64,
    pub battery_wh: f64,
    pub comms_mbps: f64,
    /// Radio transmit power at full downlink (watts).
    pub comms_power_w: f64,
    pub payload_power_w: f64,
    pub launch_cost_usd: f64,
    pub propellant_kg: f64,
    pub isp_s: f64,
    /// Recurring operating cost while this bus is on orbit.
    #[serde(default = "default_ops_cost")]
    pub ops_cost_usd_per_day: f64,
    #[serde(default = "default_unlocked")]
    pub unlocked_by_default: bool,
}

fn default_unlocked() -> bool {
    true
}

fn default_ops_cost() -> f64 {
    5_000.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundStationDef {
    pub id: String,
    pub name: String,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub min_elevation_deg: f64,
    #[serde(default = "default_bandwidth")]
    pub bandwidth_mbps: f64,
}

fn default_bandwidth() -> f64 {
    50.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub downlink_mb: f64,
    pub deadline_days: f64,
    pub payout_usd: f64,
    #[serde(default)]
    pub reputation_required: f64,
    #[serde(default)]
    pub reputation_reward: f64,
    /// Minimum distinct ground stations that must receive downlink (0 = no requirement).
    #[serde(default)]
    pub min_distinct_stations: u32,
    /// Orbit requirements: contract progress only accrues while these hold.
    #[serde(default)]
    pub min_altitude_km: Option<f64>,
    #[serde(default)]
    pub max_altitude_km: Option<f64>,
    #[serde(default)]
    pub min_inclination_deg: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechNode {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cost_usd: f64,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub unlocks_bus: Option<String>,
    #[serde(default = "default_one")]
    pub comms_multiplier: f64,
    #[serde(default = "default_one")]
    pub solar_multiplier: f64,
    #[serde(default = "default_one")]
    pub isp_multiplier: f64,
    /// Multiplies the mean time between anomalies (higher = rarer).
    #[serde(default = "default_one")]
    pub anomaly_mtbf_multiplier: f64,
}

fn default_one() -> f64 {
    1.0
}

impl GameContent {
    pub fn from_json(json: &str) -> Result<Self, crate::error::SimError> {
        let content: Self = serde_json::from_str(json)
            .map_err(|e| crate::error::SimError::Content(format!("content parse: {e}")))?;
        content.validate()?;
        Ok(content)
    }

    /// Reject content whose physical parameters would produce NaN/inf in the
    /// sim (e.g. Δv capacity) or whose cross-references dangle.
    fn validate(&self) -> Result<(), crate::error::SimError> {
        use crate::error::SimError;
        let roots: Vec<&CelestialBodyDef> =
            self.bodies.iter().filter(|b| b.parent.is_none()).collect();
        if roots.len() != 1 || roots[0].id != "sun" {
            return Err(SimError::Content(
                "bodies must have exactly one root body with id 'sun'".into(),
            ));
        }
        if !self.bodies.iter().any(|b| b.id == "earth") {
            return Err(SimError::Content("bodies must include 'earth'".into()));
        }
        for (i, body) in self.bodies.iter().enumerate() {
            if self.bodies[..i].iter().any(|b| b.id == body.id) {
                return Err(SimError::Content(format!("duplicate body '{}'", body.id)));
            }
        }
        for body in &self.bodies {
            if body.mu_km3_s2 <= 0.0 || body.radius_km <= 0.0 {
                return Err(SimError::Content(format!(
                    "body '{}' has non-physical mu/radius",
                    body.id
                )));
            }
            if body.min_parking_altitude_km <= 0.0
                || body.min_parking_altitude_km >= body.max_parking_altitude_km
            {
                return Err(SimError::Content(format!(
                    "body '{}' has an invalid parking range",
                    body.id
                )));
            }
            match (&body.parent, &body.orbit) {
                (None, Some(_)) => {
                    return Err(SimError::Content(format!(
                        "root body '{}' must not define an orbit",
                        body.id
                    )));
                }
                (Some(_), None) => {
                    return Err(SimError::Content(format!(
                        "body '{}' has a parent but no orbit",
                        body.id
                    )));
                }
                (Some(parent_id), Some(orbit)) => {
                    let parent =
                        self.bodies
                            .iter()
                            .find(|b| &b.id == parent_id)
                            .ok_or_else(|| {
                                SimError::Content(format!(
                                    "body '{}' has unknown parent '{parent_id}'",
                                    body.id
                                ))
                            })?;
                    if !(0.0..0.9).contains(&orbit.eccentricity) || orbit.semi_major_axis_km <= 0.0
                    {
                        return Err(SimError::Content(format!(
                            "body '{}' has a non-physical orbit",
                            body.id
                        )));
                    }
                    let periapsis = orbit.semi_major_axis_km * (1.0 - orbit.eccentricity);
                    if periapsis <= parent.radius_km {
                        return Err(SimError::Content(format!(
                            "body '{}' orbit dips inside its parent",
                            body.id
                        )));
                    }
                }
                (None, None) => {}
            }
        }
        // Acyclic parent chains: walking up must reach the root within len() hops.
        for body in &self.bodies {
            let mut current = body.parent.as_deref();
            let mut hops = 0;
            while let Some(pid) = current {
                hops += 1;
                if hops > self.bodies.len() {
                    return Err(SimError::Content(format!(
                        "body '{}' has a cyclic parent chain",
                        body.id
                    )));
                }
                current = self
                    .bodies
                    .iter()
                    .find(|b| b.id == pid)
                    .and_then(|b| b.parent.as_deref());
            }
        }
        for bus in &self.buses {
            if bus.mass_kg <= 0.0
                || bus.isp_s <= 0.0
                || bus.battery_wh <= 0.0
                || bus.propellant_kg < 0.0
                || bus.comms_mbps <= 0.0
                || bus.comms_power_w < 0.0
                || bus.solar_power_w <= 0.0
                || bus.ops_cost_usd_per_day < 0.0
            {
                return Err(SimError::Content(format!(
                    "bus '{}' has non-physical parameters",
                    bus.id
                )));
            }
        }
        // Contracts are Earth-bound until they carry a body id (M9): their
        // requirements must be reachable within Earth's parking clamps.
        let earth = self
            .bodies
            .iter()
            .find(|b| b.id == "earth")
            .expect("checked above");
        let parking = earth.min_parking_altitude_km..=earth.max_parking_altitude_km;
        for contract in &self.contracts {
            let min_alt = contract
                .min_altitude_km
                .unwrap_or(earth.min_parking_altitude_km);
            let max_alt = contract
                .max_altitude_km
                .unwrap_or(earth.max_parking_altitude_km);
            if !parking.contains(&min_alt)
                || !parking.contains(&max_alt)
                || min_alt > max_alt
                || contract
                    .min_inclination_deg
                    .is_some_and(|i| !(0.0..=98.0).contains(&i))
            {
                return Err(SimError::Content(format!(
                    "contract '{}' has unsatisfiable orbit requirements",
                    contract.id
                )));
            }
        }
        for tech in &self.tech_tree {
            if tech.comms_multiplier <= 0.0
                || tech.solar_multiplier <= 0.0
                || tech.isp_multiplier <= 0.0
                || tech.anomaly_mtbf_multiplier <= 0.0
            {
                return Err(SimError::Content(format!(
                    "tech '{}' has non-positive multipliers",
                    tech.id
                )));
            }
            for req in &tech.requires {
                if self.tech(req).is_none() {
                    return Err(SimError::Content(format!(
                        "tech '{}' requires unknown tech '{req}'",
                        tech.id
                    )));
                }
            }
            if let Some(bus_id) = &tech.unlocks_bus
                && self.bus(bus_id).is_none()
            {
                return Err(SimError::Content(format!(
                    "tech '{}' unlocks unknown bus '{bus_id}'",
                    tech.id
                )));
            }
        }
        for scenario in &self.scenarios {
            if self.bus(&scenario.bus_id).is_none() {
                return Err(SimError::Content(format!(
                    "scenario '{}' references unknown bus '{}'",
                    scenario.id, scenario.bus_id
                )));
            }
            if let Some(contract_id) = &scenario.contract_id
                && self.contract(contract_id).is_none()
            {
                return Err(SimError::Content(format!(
                    "scenario '{}' references unknown contract '{contract_id}'",
                    scenario.id
                )));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn bus(&self, id: &str) -> Option<&BusTemplate> {
        self.buses.iter().find(|b| b.id == id)
    }

    #[must_use]
    pub fn contract(&self, id: &str) -> Option<&ContractTemplate> {
        self.contracts.iter().find(|c| c.id == id)
    }

    #[must_use]
    pub fn tech(&self, id: &str) -> Option<&TechNode> {
        self.tech_tree.iter().find(|t| t.id == id)
    }

    #[must_use]
    pub fn scenario(&self, id: &str) -> Option<&crate::scenario::ScenarioTemplate> {
        self.scenarios.iter().find(|s| s.id == id)
    }
}

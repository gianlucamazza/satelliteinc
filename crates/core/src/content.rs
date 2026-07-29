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
    /// Campaign structure: one Program per body, gated by reputation, a
    /// cash buy-in, and the previous Program's keystone contract.
    #[serde(default)]
    pub programs: Vec<ProgramDef>,
    /// Rival firms that contest marketplace offers (M19). Empty = no market pressure.
    #[serde(default)]
    pub competitors: Vec<CompetitorDef>,
    /// Global space-weather / campaign pressure events (M20).
    #[serde(default)]
    pub world_events: Vec<WorldEventDef>,
}

/// A scheduled world pressure event (solar storm, DSN congestion, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEventDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Relative weight for deterministic picks (min 1).
    #[serde(default = "default_event_weight")]
    pub weight: u32,
    /// How long the event lasts once started.
    pub duration_days: f64,
    /// Empty = all bodies; otherwise only listed bodies feel the effects.
    #[serde(default)]
    pub body_ids: Vec<String>,
    /// Multiplies solar panel output while active (1.0 = none).
    #[serde(default = "default_one")]
    pub solar_penalty: f64,
    /// Multiplies anomaly frequency (1.0 = none).
    #[serde(default = "default_one")]
    pub anomaly_rate_multiplier: f64,
    /// Multiplies link bandwidth (1.0 = none).
    #[serde(default = "default_one")]
    pub bandwidth_multiplier: f64,
    /// When true, bandwidth effect applies only to DSN (deep-space) links.
    #[serde(default)]
    pub dsn_only: bool,
}

fn default_event_weight() -> u32 {
    1
}

impl WorldEventDef {
    #[must_use]
    pub fn applies_to_body(&self, body_id: &str) -> bool {
        self.body_ids.is_empty() || self.body_ids.iter().any(|b| b == body_id)
    }
}

/// A rival agency that competes for contracts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompetitorDef {
    pub id: String,
    pub name: String,
    /// Higher = more frequent contest actions (clamped 0.25–2.0 at runtime).
    #[serde(default = "default_aggressiveness")]
    pub aggressiveness: f64,
    /// Prefer contracts on these bodies; empty = any body.
    #[serde(default)]
    pub focus_bodies: Vec<String>,
}

fn default_aggressiveness() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramDef {
    pub id: String,
    pub name: String,
    pub body_id: String,
    #[serde(default)]
    pub buyin_usd: f64,
    #[serde(default)]
    pub reputation_required: f64,
    /// Contract that must be completed before this Program can be bought.
    #[serde(default)]
    pub requires_keystone: Option<String>,
    /// This Program's own keystone (feeds the next gate).
    pub keystone_contract_id: String,
    /// Tech that must be researched before buying in (e.g. rad-hardening).
    #[serde(default)]
    pub requires_tech: Option<String>,
    /// One-time payout when every contract of the body is completed.
    #[serde(default)]
    pub completion_grant_usd: f64,
    /// Unlocked from the start (Earth).
    #[serde(default)]
    pub unlocked_by_default: bool,
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
    /// Effective DSN bandwidth fraction from this body (difficulty dial).
    #[serde(default = "default_one")]
    pub dsn_bandwidth_factor: f64,
    /// Divides the anomaly MTBF at this body (>1 = more anomalies).
    #[serde(default = "default_one")]
    pub anomaly_rate_multiplier: f64,
    /// Optional per-body anomaly reskin/behavior.
    #[serde(default)]
    pub anomaly_flavor: Option<AnomalyFlavorDef>,
    /// Launch-provider surcharge for an injection toward this body.
    #[serde(default = "default_injection_multiplier")]
    pub injection_cost_multiplier: f64,
    /// Whether aerobraking can shave the capture burn here.
    #[serde(default)]
    pub has_atmosphere: bool,
    /// Eccentricity of the post-capture orbit. Zero (circular) suits small
    /// bodies; giants need a Juno-style high-e capture ellipse to keep the
    /// insertion burn physical (~1.5 km/s instead of ~7).
    #[serde(default)]
    pub capture_eccentricity: f64,
    /// Second zonal harmonic: drives nodal regression and apsidal
    /// precession (sun-synchronous orbits become real).
    #[serde(default)]
    pub j2: f64,
}

fn default_injection_multiplier() -> f64 {
    2.0
}

/// Per-body anomaly flavor: relabel the base archetypes and optionally
/// change their bite (all data, no code per body).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyFlavorDef {
    #[serde(default)]
    pub power_glitch_label: Option<String>,
    #[serde(default)]
    pub comms_dropout_label: Option<String>,
    #[serde(default)]
    pub buffer_corruption_label: Option<String>,
    /// Override the comms-dropout duration (e.g. Mars dust storms run long).
    #[serde(default)]
    pub comms_dropout_duration_s: Option<f64>,
    /// Extra solar multiplier while an anomaly is active and unsafed.
    #[serde(default = "default_one")]
    pub solar_penalty_while_active: f64,
    /// Fraction of the buffer lost to a corruption event.
    #[serde(default = "default_corruption_fraction")]
    pub buffer_corruption_fraction: f64,
}

fn default_corruption_fraction() -> f64 {
    0.25
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
            dsn_bandwidth_factor: 1.0,
            anomaly_rate_multiplier: 1.0,
            anomaly_flavor: None,
            injection_cost_multiplier: 2.0,
            has_atmosphere: false,
            capture_eccentricity: 0.0,
            j2: 0.0,
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
            dsn_bandwidth_factor: 1.0,
            anomaly_rate_multiplier: 1.0,
            anomaly_flavor: None,
            injection_cost_multiplier: 2.0,
            has_atmosphere: true,
            capture_eccentricity: 0.0,
            j2: 1.082_63e-3,
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
    /// Radioisotope generator output: constant watts, indifferent to
    /// sunlight, eclipse, and dust.
    #[serde(default)]
    pub rtg_power_w: f64,
    /// Onboard data buffer capacity.
    #[serde(default = "default_buffer_mb")]
    pub buffer_mb: f64,
    /// Peak imaging rate (idle, in sunlight); modes scale it down.
    #[serde(default = "default_imaging_rate")]
    pub imaging_rate_mb_s: f64,
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

fn default_buffer_mb() -> f64 {
    2_048.0
}

fn default_imaging_rate() -> f64 {
    5.0
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
    /// Body this station sits on.
    #[serde(default = "default_body_id")]
    pub body_id: String,
    /// Deep Space Network dish: reaches craft at other bodies over a
    /// line-of-sight link at `body.dsn_bandwidth_factor` of its bandwidth.
    #[serde(default)]
    pub dsn: bool,
    /// Optional tech gate. Sandbox ignores this (all stations available).
    #[serde(default)]
    pub requires_tech: Option<String>,
}

pub(crate) fn default_body_id() -> String {
    "earth".into()
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
    /// Body whose orbit fulfills this contract.
    #[serde(default = "default_body_id")]
    pub body_id: String,
    /// Fraction of the payout advanced on acceptance (clawed back with an
    /// extra reputation hit on failure).
    #[serde(default)]
    pub advance_fraction: f64,
    /// Cumulative days the craft must spend in a compliant orbit.
    #[serde(default)]
    pub min_on_station_days: f64,
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
    /// Multiplies daily ops cost (<1 = cheaper missions).
    #[serde(default = "default_one")]
    pub ops_cost_multiplier: f64,
    /// Multiplies DSN link bandwidth to craft beyond Earth.
    #[serde(default = "default_one")]
    pub deep_space_comms_multiplier: f64,
    /// Multiplies capture burn Δv at bodies with an atmosphere (aerobraking).
    #[serde(default = "default_one")]
    pub capture_dv_multiplier: f64,
    /// Extra Mission Control slots (concurrent flights).
    #[serde(default)]
    pub flight_slots: u32,
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
                || bus.buffer_mb <= 0.0
                || bus.imaging_rate_mb_s <= 0.0
            {
                return Err(SimError::Content(format!(
                    "bus '{}' has non-physical parameters",
                    bus.id
                )));
            }
        }
        // Contract requirements must be reachable within the parking clamps
        // of the contract's own body.
        for contract in &self.contracts {
            let body = self
                .bodies
                .iter()
                .find(|b| b.id == contract.body_id)
                .ok_or_else(|| {
                    SimError::Content(format!(
                        "contract '{}' references unknown body '{}'",
                        contract.id, contract.body_id
                    ))
                })?;
            let parking = body.min_parking_altitude_km..=body.max_parking_altitude_km;
            let min_alt = contract
                .min_altitude_km
                .unwrap_or(body.min_parking_altitude_km);
            let max_alt = contract
                .max_altitude_km
                .unwrap_or(body.max_parking_altitude_km);
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
        for station in &self.ground_stations {
            let body_known = self.bodies.iter().any(|b| b.id == station.body_id);
            if !body_known {
                return Err(SimError::Content(format!(
                    "station '{}' references unknown body '{}'",
                    station.id, station.body_id
                )));
            }
            if station.dsn && station.body_id != "earth" {
                return Err(SimError::Content(format!(
                    "DSN station '{}' must be on earth",
                    station.id
                )));
            }
            if let Some(tech_id) = &station.requires_tech
                && self.tech(tech_id).is_none()
            {
                return Err(SimError::Content(format!(
                    "station '{}' requires unknown tech '{tech_id}'",
                    station.id
                )));
            }
        }
        for body in &self.bodies {
            if !(0.0..0.95).contains(&body.capture_eccentricity) {
                return Err(SimError::Content(format!(
                    "body '{}' has a non-physical capture eccentricity",
                    body.id
                )));
            }
            if body.dsn_bandwidth_factor <= 0.0 || body.anomaly_rate_multiplier <= 0.0 {
                return Err(SimError::Content(format!(
                    "body '{}' has non-positive gameplay multipliers",
                    body.id
                )));
            }
            if let Some(flavor) = &body.anomaly_flavor
                && (flavor.solar_penalty_while_active <= 0.0
                    || flavor.comms_dropout_duration_s.is_some_and(|d| d <= 0.0))
            {
                {
                    return Err(SimError::Content(format!(
                        "body '{}' has a non-physical anomaly flavor",
                        body.id
                    )));
                }
            }
        }
        for tech in &self.tech_tree {
            if tech.comms_multiplier <= 0.0
                || tech.solar_multiplier <= 0.0
                || tech.isp_multiplier <= 0.0
                || tech.anomaly_mtbf_multiplier <= 0.0
                || tech.ops_cost_multiplier <= 0.0
                || tech.deep_space_comms_multiplier <= 0.0
                || tech.capture_dv_multiplier <= 0.0
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
        for program in &self.programs {
            if !self.bodies.iter().any(|b| b.id == program.body_id) {
                return Err(SimError::Content(format!(
                    "program '{}' references unknown body '{}'",
                    program.id, program.body_id
                )));
            }
            if self.contract(&program.keystone_contract_id).is_none() {
                return Err(SimError::Content(format!(
                    "program '{}' has unknown keystone '{}'",
                    program.id, program.keystone_contract_id
                )));
            }
            if let Some(tech_id) = &program.requires_tech
                && self.tech(tech_id).is_none()
            {
                return Err(SimError::Content(format!(
                    "program '{}' requires unknown tech '{tech_id}'",
                    program.id
                )));
            }
            if let Some(req) = &program.requires_keystone
                && self.contract(req).is_none()
            {
                return Err(SimError::Content(format!(
                    "program '{}' requires unknown keystone '{req}'",
                    program.id
                )));
            }
            if self
                .programs
                .iter()
                .filter(|other| other.body_id == program.body_id)
                .count()
                > 1
            {
                return Err(SimError::Content(format!(
                    "body '{}' has more than one program",
                    program.body_id
                )));
            }
        }
        for contract in &self.contracts {
            if !(0.0..=0.5).contains(&contract.advance_fraction)
                || contract.min_on_station_days < 0.0
            {
                return Err(SimError::Content(format!(
                    "contract '{}' has invalid advance/on-station terms",
                    contract.id
                )));
            }
        }
        for scenario in &self.scenarios {
            if !self.bodies.iter().any(|b| b.id == scenario.body_id) {
                return Err(SimError::Content(format!(
                    "scenario '{}' references unknown body '{}'",
                    scenario.id, scenario.body_id
                )));
            }
            for program_id in &scenario.starting_programs {
                if self.program(program_id).is_none() {
                    return Err(SimError::Content(format!(
                        "scenario '{}' references unknown program '{program_id}'",
                        scenario.id
                    )));
                }
            }
            for tech_id in &scenario.starting_tech {
                if self.tech(tech_id).is_none() {
                    return Err(SimError::Content(format!(
                        "scenario '{}' references unknown tech '{tech_id}'",
                        scenario.id
                    )));
                }
            }
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
        for (i, rival) in self.competitors.iter().enumerate() {
            if self.competitors[..i].iter().any(|r| r.id == rival.id) {
                return Err(SimError::Content(format!(
                    "duplicate competitor '{}'",
                    rival.id
                )));
            }
            if rival.aggressiveness <= 0.0 {
                return Err(SimError::Content(format!(
                    "competitor '{}' has non-positive aggressiveness",
                    rival.id
                )));
            }
            for body_id in &rival.focus_bodies {
                if !self.bodies.iter().any(|b| b.id == *body_id) {
                    return Err(SimError::Content(format!(
                        "competitor '{}' focuses unknown body '{body_id}'",
                        rival.id
                    )));
                }
            }
        }
        for (i, ev) in self.world_events.iter().enumerate() {
            if self.world_events[..i].iter().any(|e| e.id == ev.id) {
                return Err(SimError::Content(format!(
                    "duplicate world event '{}'",
                    ev.id
                )));
            }
            if ev.duration_days <= 0.0 {
                return Err(SimError::Content(format!(
                    "world event '{}' has non-positive duration",
                    ev.id
                )));
            }
            if ev.solar_penalty <= 0.0
                || ev.anomaly_rate_multiplier <= 0.0
                || ev.bandwidth_multiplier <= 0.0
            {
                return Err(SimError::Content(format!(
                    "world event '{}' has non-positive multipliers",
                    ev.id
                )));
            }
            for body_id in &ev.body_ids {
                if !self.bodies.iter().any(|b| b.id == *body_id) {
                    return Err(SimError::Content(format!(
                        "world event '{}' references unknown body '{body_id}'",
                        ev.id
                    )));
                }
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
    pub fn competitor(&self, id: &str) -> Option<&CompetitorDef> {
        self.competitors.iter().find(|c| c.id == id)
    }

    #[must_use]
    pub fn world_event(&self, id: &str) -> Option<&WorldEventDef> {
        self.world_events.iter().find(|e| e.id == id)
    }

    #[must_use]
    pub fn program_for_body(&self, body_id: &str) -> Option<&ProgramDef> {
        self.programs.iter().find(|p| p.body_id == body_id)
    }

    pub fn program(&self, id: &str) -> Option<&ProgramDef> {
        self.programs.iter().find(|p| p.id == id)
    }

    pub fn scenario(&self, id: &str) -> Option<&crate::scenario::ScenarioTemplate> {
        let id = crate::scenario::resolve_scenario_id(id);
        self.scenarios.iter().find(|s| s.id == id)
    }
}

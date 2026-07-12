use crate::action::PlayerAction;
use crate::body::{BodyRegistry, CelestialBody};
use crate::clock::SimClock;
use crate::company::{ActiveContract, GameMode, GameOutcome, can_purchase_tech};
use crate::constants::SECONDS_PER_DAY;
use crate::content::GameContent;
use crate::error::SimError;
use crate::event::SimEvent;
use crate::maneuver_ops::{
    complete_hohmann, execute_prograde_burn, preview_hohmann, start_hohmann,
};
use crate::ops::{
    ANOMALY_COOLDOWN_SECONDS, ActiveAnomaly, AnomalyType, CommandType, QueuedCommand,
    execute_command, roll_anomaly,
};
use crate::ops::{GroundStation, eci_to_ecef_km, in_sunlight, is_in_pass};
use crate::orbit::{
    ReferenceSatellite, TlePropagator, at_apsis, circular_orbit, eci_position_km,
    eci_velocity_km_s, orbit_trail_samples, seconds_to_apsides,
};
use crate::snapshot::{
    BodySnapshot, GamePhase, ProgramSnapshot, Snapshot, SpacecraftSnapshot, StationSnapshot,
    TransferPlannerSnapshot, TransferSnapshot,
};
use crate::spacecraft::{CraftPhase, HohmannPlanSnapshot, PayloadMode, Spacecraft};

const STEP_DT_SECONDS: f64 = 1.0;
const STARTING_CASH: f64 = 450_000.0;
/// Campaign game over when cash falls below this floor.
pub const BANKRUPTCY_FLOOR_USD: f64 = -50_000.0;
/// Recurring campaign overhead, charged even without a satellite.
pub const BASE_OVERHEAD_USD_PER_DAY: f64 = 1_000.0;
/// Fraction of launch cost refunded on decommission.
pub const SALVAGE_FRACTION: f64 = 0.15;
/// Ops-cost fraction while hibernated on a transfer cruise.
pub const HIBERNATION_OPS_FRACTION: f64 = 0.10;
/// Longest stride a single WarpTo call advances (re-issue to continue).
pub const WARP_CHUNK_SECONDS: f64 = 2_000_000.0;

pub struct SimState {
    pub clock: SimClock,
    pub company: crate::company::CompanyState,
    pub spacecraft: Option<Spacecraft>,
    pub bodies: BodyRegistry,
    pub ground_stations: Vec<GroundStation>,
    pub content: GameContent,
    pub event_queue: Vec<SimEvent>,
    pub tle_propagators: Vec<TlePropagator>,
    /// Per-station pass edge state, indexed parallel to `ground_stations`
    /// (saves serialize it keyed by station id — see `export_save`).
    pub pass_active: Vec<bool>,
    /// Per-tick cache for SGP4 reference-satellite states (snapshot runs per frame).
    pub(crate) ref_sat_cache: std::cell::RefCell<Option<(u64, Vec<ReferenceSatellite>)>>,
}

impl SimState {
    /// Errors when the content's body tree is invalid (`GameContent` has
    /// public fields, so callers are not forced through `from_json`'s
    /// validation).
    pub fn new(content: GameContent) -> Result<Self, SimError> {
        let bodies = BodyRegistry::from_content(&content.bodies)?;
        let ground_stations: Vec<GroundStation> = content
            .ground_stations
            .iter()
            .map(GroundStation::from)
            .collect();
        let tle_propagators: Vec<TlePropagator> = content
            .tle_catalog
            .iter()
            .filter_map(|e| TlePropagator::from_entry(e).ok())
            .collect();
        Ok(Self {
            clock: SimClock::new(),
            company: crate::company::CompanyState::new(
                "Satellite Inc",
                STARTING_CASH,
                GameMode::Campaign,
                &content,
            ),
            spacecraft: None,
            bodies,
            pass_active: vec![false; ground_stations.len()],
            ground_stations,
            content,
            event_queue: Vec::new(),
            tle_propagators,
            ref_sat_cache: std::cell::RefCell::new(None),
        })
    }

    /// Central body of the current spacecraft (earth while in design or
    /// without a craft). `body_id` is validated on load/import.
    pub fn craft_body(&self) -> &CelestialBody {
        match self.spacecraft.as_ref() {
            Some(c) => self
                .bodies
                .get(&c.body_id)
                .expect("spacecraft body_id validated on load"),
            None => self.bodies.earth(),
        }
    }

    /// Whether a station can serve a craft at the given body: local dishes
    /// on the same body, or Earth DSN dishes for any craft beyond Earth.
    fn station_reachable(station: &GroundStation, craft_body_id: &str) -> bool {
        station.body_id == craft_body_id || (station.dsn && craft_body_id != "earth")
    }

    /// Link bandwidth for a station serving a craft at `body`: full for a
    /// local dish, scaled by the body's DSN factor over a deep-space link.
    fn station_bandwidth_for(
        station: &GroundStation,
        body: &CelestialBody,
        deep_space_comms_multiplier: f64,
    ) -> f64 {
        if station.body_id == body.id {
            station.bandwidth_mbps
        } else {
            station.bandwidth_mbps * body.dsn_bandwidth_factor * deep_space_comms_multiplier
        }
    }

    /// Unit vector from the craft's body toward Earth at `sim_seconds`.
    fn earth_direction_from(&self, body_id: &str, sim_seconds: f64) -> [f64; 3] {
        let body_idx = self.bodies.idx(body_id).expect("validated body id");
        let earth_idx = self.bodies.idx("earth").expect("registry holds earth");
        let e = self.bodies.heliocentric_position_km(earth_idx, sim_seconds);
        let b = self.bodies.heliocentric_position_km(body_idx, sim_seconds);
        let d = [e[0] - b[0], e[1] - b[1], e[2] - b[2]];
        let r = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if r < 1.0 {
            return [1.0, 0.0, 0.0];
        }
        [d[0] / r, d[1] / r, d[2] / r]
    }

    /// Seconds until the craft next has line of sight to Earth (DSN window),
    /// searching forward like `seconds_until_pass`.
    fn seconds_until_los(
        &self,
        body: &CelestialBody,
        elements: &crate::orbit::KeplerElements,
        from_sim_seconds: f64,
        max_search_seconds: f64,
        step_seconds: f64,
    ) -> Option<f64> {
        let earth_dir = self.earth_direction_from(&body.id, from_sim_seconds);
        #[allow(clippy::cast_sign_loss)] // positive durations
        let steps = (max_search_seconds / step_seconds).ceil() as u64;
        let mut was_visible = {
            let eci = eci_position_km(body.mu_km3_s2, elements, from_sim_seconds);
            in_sunlight(eci, earth_dir, body.radius_km)
        };
        for i in 1..=steps {
            let t = from_sim_seconds + i as f64 * step_seconds;
            let eci = eci_position_km(body.mu_km3_s2, elements, t);
            let visible = in_sunlight(eci, earth_dir, body.radius_km);
            if visible && !was_visible {
                return Some(t - from_sim_seconds);
            }
            was_visible = visible;
        }
        None
    }

    /// Validate and start an interplanetary transfer: cash pays the
    /// injection, propellant for the capture burn is reserved up front.
    fn begin_transfer(&mut self) -> Result<(), SimError> {
        let now = self.clock.sim_seconds;
        let craft = self
            .spacecraft
            .as_ref()
            .ok_or_else(|| SimError::InvalidAction("no spacecraft".into()))?;
        if craft.phase != CraftPhase::OnOrbit {
            return Err(SimError::WrongPhase);
        }
        let elements = craft
            .elements
            .ok_or_else(|| SimError::InvalidAction("no orbit".into()))?;
        let target_sel = craft
            .transfer_target
            .clone()
            .ok_or_else(|| SimError::InvalidAction("no transfer target set".into()))?;
        let origin_idx = self.bodies.idx(&craft.body_id).expect("validated body id");
        let target_idx = self
            .bodies
            .idx(&target_sel.body_id)
            .ok_or_else(|| SimError::Content(format!("unknown body: {}", target_sel.body_id)))?;

        if !crate::orbit::window_open(&self.bodies, origin_idx, target_idx, now)? {
            let next = crate::orbit::next_launch_window(&self.bodies, origin_idx, target_idx, now)?;
            return Err(SimError::InvalidAction(format!(
                "transfer window closed — next opens in {:.1} days",
                (next - now) / crate::constants::SECONDS_PER_DAY
            )));
        }

        let plan = crate::orbit::plan_transfer(
            &self.bodies,
            origin_idx,
            target_idx,
            elements.semi_major_axis_km,
            target_sel.capture_altitude_km,
            now,
        )?;

        let target_body = self.bodies.body(target_idx);
        let injection_cost = craft.bus.launch_cost_usd * target_body.injection_cost_multiplier;
        if !self.company.can_afford(injection_cost) {
            self.push_event(SimEvent::InsufficientFunds {
                required_usd: injection_cost,
            });
            return Err(SimError::InsufficientFunds(injection_cost));
        }

        // Aerobraking tech shaves the capture burn where there's air.
        let capture_dv = if target_body.has_atmosphere {
            plan.capture_dv_km_s * self.company.capture_dv_multiplier
        } else {
            plan.capture_dv_km_s
        };
        let isp = craft.bus.isp_s * self.company.isp_multiplier;
        let reserved_kg = crate::orbit::propellant_for_delta_v(
            capture_dv,
            craft.dry_mass_kg(),
            craft.propellant_kg,
            isp,
        );
        if reserved_kg > craft.propellant_kg + 1e-6 {
            return Err(SimError::InvalidAction(format!(
                "insufficient propellant for the capture burn: need {reserved_kg:.1} kg"
            )));
        }

        let arrival = now + plan.transfer_seconds;
        let frame_body_id = plan.frame_body_id.clone();
        let transfer = crate::spacecraft::ActiveTransfer {
            origin_body_id: plan.origin_body_id.clone(),
            target_body_id: plan.target_body_id.clone(),
            frame_body_id: frame_body_id.clone(),
            departure_sim_seconds: now,
            arrival_sim_seconds: arrival,
            capture_altitude_km: target_sel.capture_altitude_km,
            capture_inclination_deg: target_sel.capture_inclination_deg,
            capture_dv_km_s: capture_dv,
            capture_propellant_reserved_kg: reserved_kg,
        };

        self.company.cash_usd -= injection_cost;
        let craft = self.spacecraft.as_mut().expect("checked above");
        craft.propellant_kg -= reserved_kg;
        craft.phase = CraftPhase::Transfer;
        craft.body_id = frame_body_id;
        craft.elements = Some(plan.elements);
        craft.pending_hohmann = None;
        craft.downlink_active = false;
        craft.transfer = Some(transfer);
        self.pass_active.fill(false);
        self.push_event(SimEvent::TransferStarted {
            target_body_id: target_sel.body_id,
            arrival_sim_seconds: arrival,
            injection_cost_usd: injection_cost,
        });
        Ok(())
    }

    /// Arrival: automatic capture into the reserved parking orbit. The
    /// propellant was deducted at departure, so this can never strand.
    fn complete_arrival(&mut self) {
        let Some(craft) = self.spacecraft.as_mut() else {
            return;
        };
        let Some(transfer) = craft.transfer.take() else {
            return;
        };
        let target = self
            .bodies
            .get(&transfer.target_body_id)
            .expect("transfer target validated at departure");
        craft.body_id.clone_from(&target.id);
        craft.elements = Some(circular_orbit(
            target.radius_km,
            transfer.capture_altitude_km,
            transfer.capture_inclination_deg,
            transfer.arrival_sim_seconds,
        ));
        craft.phase = CraftPhase::OnOrbit;
        craft.total_delta_v_used_km_s += transfer.capture_dv_km_s;
        craft.transfer_target = None;
        let target_id = target.id.clone();
        let default_station = self
            .ground_stations
            .iter()
            .find(|st| Self::station_reachable(st, &target_id))
            .map(|st| st.id.clone());
        let craft = self.spacecraft.as_mut().expect("still here");
        craft.downlink_station_id = default_station;
        self.pass_active.fill(false);
        self.push_event(SimEvent::TransferArrived { body_id: target_id });
        self.push_event(SimEvent::CaptureExecuted {
            delta_v_km_s: transfer.capture_dv_km_s,
        });
    }

    pub fn apply_action(&mut self, action: PlayerAction) -> Result<(), SimError> {
        if matches!(
            self.company.outcome,
            Some(GameOutcome::Bankrupt | GameOutcome::ReputationLost)
        ) && !matches!(
            action,
            PlayerAction::LoadScenario { .. } | PlayerAction::SetGameMode { .. }
        ) {
            return Err(SimError::InvalidAction(
                "game over — load a scenario or switch mode".into(),
            ));
        }
        match action {
            PlayerAction::SelectBus { bus_id } => {
                if self
                    .spacecraft
                    .as_ref()
                    .is_some_and(|c| c.phase == CraftPhase::OnOrbit)
                {
                    return Err(SimError::InvalidAction(
                        "decommission the current satellite first".into(),
                    ));
                }
                if !self.company.is_bus_unlocked(&bus_id) {
                    return Err(SimError::InvalidAction(format!(
                        "bus not unlocked: {bus_id}"
                    )));
                }
                let bus = self
                    .content
                    .bus(&bus_id)
                    .ok_or_else(|| SimError::Content(format!("unknown bus: {bus_id}")))?
                    .clone();
                self.spacecraft = Some(Spacecraft::new_from_bus(&bus, &bus.name));
                self.push_event(SimEvent::ActionApplied {
                    message: format!("Selected bus: {}", bus.name),
                });
            }
            PlayerAction::SetOrbit {
                altitude_km,
                inclination_deg,
            } => {
                let (min_park, max_park) = {
                    let body = self.craft_body();
                    (body.min_parking_altitude_km, body.max_parking_altitude_km)
                };
                let craft = self.spacecraft_mut(CraftPhase::Design)?;
                craft.target_altitude_km = altitude_km.clamp(min_park, max_park);
                craft.target_inclination_deg = inclination_deg.clamp(0.0, 98.0);
            }
            PlayerAction::AcceptContract { contract_id } => {
                if self.company.active_contract.is_some() {
                    return Err(SimError::InvalidAction("contract already active".into()));
                }
                let offer = self
                    .company
                    .marketplace(&self.content)
                    .into_iter()
                    .find(|o| o.template_id == contract_id)
                    .ok_or_else(|| SimError::Content(format!("unknown contract: {contract_id}")))?;
                if !offer.available {
                    return Err(SimError::InvalidAction(
                        "contract not available (reputation or already completed)".into(),
                    ));
                }
                let template = self
                    .content
                    .contract(&contract_id)
                    .ok_or_else(|| SimError::Content(format!("unknown contract: {contract_id}")))?
                    .clone();
                let completions = if self.company.game_mode == GameMode::Campaign {
                    self.company.completions_of(&contract_id)
                } else {
                    0
                };
                let mut contract =
                    ActiveContract::from_template(&template, self.clock.sim_seconds, completions);
                // Interplanetary mobilization advance: cash up front, clawed
                // back with interest in reputation on failure.
                if completions == 0 && template.advance_fraction > 0.0 {
                    let advance = contract.payout_usd * template.advance_fraction;
                    contract.advance_paid_usd = advance;
                    self.company.cash_usd += advance;
                }
                let advance = contract.advance_paid_usd;
                self.company.active_contract = Some(contract);
                self.push_event(SimEvent::ActionApplied {
                    message: if advance > 0.0 {
                        format!(
                            "Accepted contract: {} (+${:.0} advance)",
                            template.name, advance
                        )
                    } else {
                        format!("Accepted contract: {}", template.name)
                    },
                });
            }
            PlayerAction::Launch => {
                let (cost, alt, inc) = {
                    let craft = self.spacecraft_mut(CraftPhase::Design)?;
                    (
                        craft.bus.launch_cost_usd,
                        craft.target_altitude_km,
                        craft.target_inclination_deg,
                    )
                };
                if !self.company.can_afford(cost) {
                    self.push_event(SimEvent::InsufficientFunds { required_usd: cost });
                    return Err(SimError::InsufficientFunds(cost));
                }
                let elements = circular_orbit(
                    self.craft_body().radius_km,
                    alt,
                    inc,
                    self.clock.sim_seconds,
                );
                let default_station = self.ground_stations.first().map(|s| s.id.clone());
                self.company.cash_usd -= cost;
                let craft = self.spacecraft_mut(CraftPhase::Design)?;
                craft.phase = CraftPhase::OnOrbit;
                craft.elements = Some(elements);
                craft.downlink_active = false;
                if craft.downlink_station_id.is_none() {
                    craft.downlink_station_id = default_station;
                }
                self.push_event(SimEvent::Launched {
                    altitude_km: alt,
                    inclination_deg: inc,
                });
            }
            PlayerAction::SetDownlink { active } => {
                let craft = self.spacecraft_mut(CraftPhase::OnOrbit)?;
                craft.downlink_active = active;
            }
            PlayerAction::SetTimeAccel { multiplier } => {
                // 0 pauses the simulation.
                self.clock.time_accel = multiplier.clamp(0.0, 100_000.0);
            }
            PlayerAction::ProgradeBurn { delta_v_km_s } => {
                let isp_mult = self.company.isp_multiplier;
                let body = self.craft_body().clone();
                let craft = self.spacecraft.as_mut().ok_or(SimError::WrongPhase)?;
                if craft.phase != CraftPhase::OnOrbit {
                    return Err(SimError::WrongPhase);
                }
                let result = execute_prograde_burn(
                    &body,
                    craft,
                    self.clock.sim_seconds,
                    delta_v_km_s.clamp(0.001, 2.0),
                    false,
                    isp_mult,
                )?;
                for ev in result.events {
                    self.push_event(ev);
                }
            }
            PlayerAction::RetrogradeBurn { delta_v_km_s } => {
                let isp_mult = self.company.isp_multiplier;
                let body = self.craft_body().clone();
                let craft = self.spacecraft.as_mut().ok_or(SimError::WrongPhase)?;
                if craft.phase != CraftPhase::OnOrbit {
                    return Err(SimError::WrongPhase);
                }
                let result = execute_prograde_burn(
                    &body,
                    craft,
                    self.clock.sim_seconds,
                    delta_v_km_s.clamp(0.001, 2.0),
                    true,
                    isp_mult,
                )?;
                for ev in result.events {
                    self.push_event(ev);
                }
            }
            PlayerAction::SetManeuverTarget { target_altitude_km } => {
                let (min_park, max_park) = {
                    let body = self.craft_body();
                    (body.min_parking_altitude_km, body.max_parking_altitude_km)
                };
                let craft = self.spacecraft_mut(CraftPhase::OnOrbit)?;
                craft.maneuver_target_altitude_km = target_altitude_km.clamp(min_park, max_park);
            }
            PlayerAction::StartHohmann => {
                let isp_mult = self.company.isp_multiplier;
                let body = self.craft_body().clone();
                let craft = self.spacecraft.as_mut().ok_or(SimError::WrongPhase)?;
                if craft.phase != CraftPhase::OnOrbit {
                    return Err(SimError::WrongPhase);
                }
                let target = craft.maneuver_target_altitude_km;
                let result = start_hohmann(&body, craft, self.clock.sim_seconds, target, isp_mult)?;
                for ev in result.events {
                    self.push_event(ev);
                }
            }
            PlayerAction::CompleteHohmann => {
                let isp_mult = self.company.isp_multiplier;
                let body = self.craft_body().clone();
                let craft = self.spacecraft.as_mut().ok_or(SimError::WrongPhase)?;
                if craft.phase != CraftPhase::OnOrbit {
                    return Err(SimError::WrongPhase);
                }
                let result = complete_hohmann(&body, craft, self.clock.sim_seconds, isp_mult)?;
                for ev in result.events {
                    self.push_event(ev);
                }
            }
            PlayerAction::Decommission => {
                let craft = self
                    .spacecraft
                    .as_ref()
                    .ok_or_else(|| SimError::InvalidAction("no spacecraft".into()))?;
                let salvage = match craft.phase {
                    CraftPhase::OnOrbit => craft.bus.launch_cost_usd * SALVAGE_FRACTION,
                    // Deep space has no salvage yard: abandoning a cruise
                    // forfeits the craft outright.
                    CraftPhase::Transfer => 0.0,
                    CraftPhase::Design => return Err(SimError::WrongPhase),
                };
                self.spacecraft = None;
                // Stale pass edges would suppress the next mission's PassStarted events.
                self.pass_active.fill(false);
                self.company.cash_usd += salvage;
                self.push_event(SimEvent::Decommissioned {
                    salvage_usd: salvage,
                });
            }
            PlayerAction::SetPayloadMode { mode } => {
                let craft = self.spacecraft_mut(CraftPhase::OnOrbit)?;
                craft.payload_mode = mode;
                // Manual mode change overrides any commanded safe-mode hold.
                craft.safe_mode_until = 0.0;
                craft.payload_mode_before_safe = None;
            }
            PlayerAction::SelectDownlinkStation { station_id } => {
                let craft_body_id = self.craft_body().id.clone();
                let station = self
                    .ground_stations
                    .iter()
                    .find(|s| s.id == station_id)
                    .ok_or_else(|| SimError::Content(format!("unknown station: {station_id}")))?;
                if !Self::station_reachable(station, &craft_body_id) {
                    return Err(SimError::InvalidAction(format!(
                        "station {station_id} cannot serve a craft at {craft_body_id}"
                    )));
                }
                let craft = self.spacecraft_mut(CraftPhase::OnOrbit)?;
                craft.downlink_station_id = Some(station_id);
            }
            PlayerAction::QueueCommand {
                command,
                station_id,
            } => {
                let craft_body_id = self.craft_body().id.clone();
                let station = self
                    .ground_stations
                    .iter()
                    .find(|s| s.id == station_id)
                    .ok_or_else(|| SimError::Content(format!("unknown station: {station_id}")))?;
                if !Self::station_reachable(station, &craft_body_id) {
                    return Err(SimError::InvalidAction(format!(
                        "station {station_id} cannot serve a craft at {craft_body_id}"
                    )));
                }
                let sim_seconds = self.clock.sim_seconds;
                let craft = self.spacecraft_mut(CraftPhase::OnOrbit)?;
                craft
                    .command_queue
                    .push(QueuedCommand::new(command, station_id, sim_seconds));
                self.push_event(SimEvent::ActionApplied {
                    message: "Command queued for next pass".into(),
                });
            }
            PlayerAction::PurchaseTech { tech_id } => {
                let tech = self
                    .content
                    .tech(&tech_id)
                    .ok_or_else(|| SimError::Content(format!("unknown tech: {tech_id}")))?
                    .clone();
                can_purchase_tech(&tech, &self.company.unlocked_tech, self.company.cash_usd)
                    .map_err(|e| SimError::InvalidAction(e.into()))?;
                self.company.cash_usd -= tech.cost_usd;
                let name = tech.name.clone();
                let id = tech.id.clone();
                self.company.apply_tech(&tech);
                self.push_event(SimEvent::TechPurchased { tech_id: id, name });
            }
            PlayerAction::SetGameMode { mode } => {
                self.company.game_mode = match mode.as_str() {
                    "campaign" => GameMode::Campaign,
                    _ => GameMode::Sandbox,
                };
                self.company.on_mode_changed(&self.content);
                self.push_event(SimEvent::ActionApplied {
                    message: format!("Game mode: {:?}", self.company.game_mode),
                });
            }
            PlayerAction::SetTransferTarget {
                body_id,
                capture_altitude_km,
                capture_inclination_deg,
            } => {
                let target = self
                    .bodies
                    .get(&body_id)
                    .ok_or_else(|| SimError::Content(format!("unknown body: {body_id}")))?;
                let altitude = capture_altitude_km.clamp(
                    target.min_parking_altitude_km,
                    target.max_parking_altitude_km,
                );
                let inclination = capture_inclination_deg.clamp(0.0, 98.0);
                let craft = self.spacecraft_mut(CraftPhase::OnOrbit)?;
                craft.transfer_target = Some(crate::spacecraft::TransferTarget {
                    body_id,
                    capture_altitude_km: altitude,
                    capture_inclination_deg: inclination,
                });
            }
            PlayerAction::BeginTransfer => {
                self.begin_transfer()?;
            }
            PlayerAction::UnlockProgram { program_id } => {
                let program = self
                    .content
                    .program(&program_id)
                    .ok_or_else(|| SimError::Content(format!("unknown program: {program_id}")))?
                    .clone();
                if self.company.unlocked_programs.contains(&program.id) {
                    return Err(SimError::InvalidAction("program already unlocked".into()));
                }
                if self.company.reputation < program.reputation_required {
                    return Err(SimError::InvalidAction(format!(
                        "requires reputation {:.0}",
                        program.reputation_required
                    )));
                }
                if let Some(keystone) = &program.requires_keystone
                    && !self.company.completed_contracts.contains(keystone)
                {
                    return Err(SimError::InvalidAction(format!(
                        "requires completing '{keystone}' first"
                    )));
                }
                if !self.company.can_afford(program.buyin_usd) {
                    self.push_event(SimEvent::InsufficientFunds {
                        required_usd: program.buyin_usd,
                    });
                    return Err(SimError::InsufficientFunds(program.buyin_usd));
                }
                self.company.cash_usd -= program.buyin_usd;
                self.company.unlocked_programs.push(program.id.clone());
                self.push_event(SimEvent::ProgramUnlocked {
                    program_id: program.id,
                });
            }
            PlayerAction::WarpTo { target_sim_seconds } => {
                if !target_sim_seconds.is_finite() || target_sim_seconds <= self.clock.sim_seconds {
                    return Err(SimError::InvalidAction(
                        "warp target must be a future sim time".into(),
                    ));
                }
                let stride = (target_sim_seconds - self.clock.sim_seconds).min(WARP_CHUNK_SECONDS);
                let mut advanced = 0.0;
                while advanced < stride {
                    let dt = (stride - advanced).min(1.0);
                    self.clock.sim_seconds += dt;
                    self.clock.tick += 1;
                    self.sim_tick(dt);
                    advanced += dt;
                    if matches!(
                        self.company.outcome,
                        Some(GameOutcome::Bankrupt | GameOutcome::ReputationLost)
                    ) {
                        break;
                    }
                }
                self.push_event(SimEvent::WarpCompleted { seconds: advanced });
            }
            PlayerAction::LoadScenario { scenario_id } => {
                self.apply_scenario(&scenario_id)?;
                self.push_event(SimEvent::ActionApplied {
                    message: format!("Loaded scenario: {scenario_id}"),
                });
            }
        }
        Ok(())
    }

    pub fn step(&mut self, real_dt_seconds: f64) {
        // The wasm boundary passes raw f64 from JS: NaN/Infinity or absurd
        // frame times must not hang the tick loop.
        if !real_dt_seconds.is_finite() {
            return;
        }
        let sim_dt = real_dt_seconds.clamp(0.0, 60.0) * self.clock.time_accel;
        let mut remaining = sim_dt;
        while remaining > 0.0 {
            let dt = remaining.min(STEP_DT_SECONDS);
            self.clock.sim_seconds += dt;
            self.clock.tick += 1;
            self.sim_tick(dt);
            remaining -= dt;
        }
    }

    fn sim_tick(&mut self, dt: f64) {
        // The world freezes after a game over; victory keeps playing.
        if matches!(
            self.company.outcome,
            Some(GameOutcome::Bankrupt | GameOutcome::ReputationLost)
        ) {
            return;
        }

        // Campaign recurring costs run even without a satellite, so a broke
        // company can't idle forever between $0 and the bankruptcy floor.
        if self.company.game_mode == GameMode::Campaign && self.company.outcome.is_none() {
            let burn = self.current_ops_cost_usd_per_day() * dt / SECONDS_PER_DAY;
            let before = self.company.cash_usd;
            self.company.cash_usd -= burn;
            if before >= 0.0 && self.company.cash_usd < 0.0 {
                self.push_event(SimEvent::CashNegative {
                    cash_usd: self.company.cash_usd,
                });
            }
            if self.company.cash_usd < BANKRUPTCY_FLOOR_USD {
                self.company.outcome = Some(GameOutcome::Bankrupt);
                self.push_event(SimEvent::GameOver {
                    reason: GameOutcome::Bankrupt,
                });
                return;
            }
        }

        let Some(craft_ref) = self.spacecraft.as_ref() else {
            return;
        };
        if craft_ref.phase == CraftPhase::Transfer {
            // Hibernated cruise: no power/imaging/pass/anomaly processing —
            // only the arrival check (inside the 1 s tick, so warp and
            // manual acceleration are bit-identical).
            if craft_ref
                .transfer
                .as_ref()
                .is_some_and(|t| self.clock.sim_seconds >= t.arrival_sim_seconds)
            {
                self.complete_arrival();
                self.check_contract();
            }
            return;
        }
        if craft_ref.phase != CraftPhase::OnOrbit {
            return;
        }
        let Some(elements) = craft_ref.elements else {
            return;
        };

        let sim_seconds = self.clock.sim_seconds;
        let craft_body_id = craft_ref.body_id.clone();
        let (mu, radius, rotation, body_idx) = {
            let body = self.craft_body();
            (
                body.mu_km3_s2,
                body.radius_km,
                body.rotation_rad_s,
                self.bodies.idx(&body.id).expect("validated body id"),
            )
        };
        let eci = eci_position_km(mu, &elements, sim_seconds);
        // Seasonal sun from the ephemerides: eclipse geometry drifts over
        // the year as the body moves around the sun.
        let sun_dir = self.bodies.sun_direction_from(body_idx, sim_seconds);
        let sunlight = in_sunlight(eci, sun_dir, radius);
        let ecef = eci_to_ecef_km(rotation, eci, sim_seconds);
        // Deep-space link: visible when the craft's own body doesn't block
        // the Earth direction (the same cylinder test as the shadow).
        let earth_los = if craft_body_id == "earth" {
            false
        } else {
            let earth_dir = self.earth_direction_from(&craft_body_id, sim_seconds);
            in_sunlight(eci, earth_dir, radius)
        };

        let comms_mult = self.company.comms_multiplier;
        let solar_mult = self.company.solar_multiplier;

        let mut pass_events = Vec::new();
        let mut station_passes = vec![false; self.ground_stations.len()];

        let mut pass_starts: Vec<(String, String)> = Vec::new();
        for (i, station) in self.ground_stations.iter().enumerate() {
            let in_pass = if station.body_id == craft_body_id {
                is_in_pass(
                    radius,
                    station.latitude_deg,
                    station.longitude_deg,
                    station.min_elevation_deg,
                    ecef,
                )
            } else if station.dsn {
                earth_los
            } else {
                false
            };
            station_passes[i] = in_pass;
            let was_active = self.pass_active[i];
            if in_pass && !was_active {
                pass_events.push(SimEvent::PassStarted {
                    station_id: station.id.clone(),
                    station_name: station.name.clone(),
                });
                pass_starts.push((station.id.clone(), station.name.clone()));
            } else if !in_pass && was_active {
                pass_events.push(SimEvent::PassEnded {
                    station_id: station.id.clone(),
                });
            }
            self.pass_active[i] = in_pass;
        }
        for (id, name) in pass_starts {
            self.try_execute_queued_command(&id, &name);
        }

        let mut anomaly_events = Vec::new();
        {
            let craft = self.spacecraft.as_mut().unwrap();
            if let Some(a) = craft.active_anomaly.clone()
                && a.is_expired(sim_seconds)
            {
                anomaly_events.push(SimEvent::AnomalyCleared { kind: a.kind });
                craft.active_anomaly = None;
                craft.anomaly_cooldown_until = sim_seconds + ANOMALY_COOLDOWN_SECONDS;
            }
            let flavor = self.bodies.body(body_idx).anomaly_flavor.clone();
            let body_rate = self.bodies.body(body_idx).anomaly_rate_multiplier;
            if craft.active_anomaly.is_none()
                && let Some(kind) = roll_anomaly(
                    sim_seconds,
                    dt,
                    craft.anomaly_cooldown_until,
                    self.company.anomaly_mtbf_multiplier / body_rate,
                )
            {
                let mut anomaly = ActiveAnomaly::new(kind, sim_seconds);
                if let Some(flavor) = &flavor {
                    anomaly.label_override = match kind {
                        AnomalyType::PowerGlitch => flavor.power_glitch_label.clone(),
                        AnomalyType::CommsDropout => flavor.comms_dropout_label.clone(),
                        AnomalyType::BufferCorruption => flavor.buffer_corruption_label.clone(),
                    };
                    if kind == AnomalyType::CommsDropout
                        && let Some(duration) = flavor.comms_dropout_duration_s
                    {
                        anomaly.duration_seconds = duration;
                    }
                }
                let label = anomaly.label().to_string();
                if kind == AnomalyType::BufferCorruption {
                    craft.buffer_mb *= 0.75;
                }
                craft.active_anomaly = Some(anomaly);
                craft.anomaly_cooldown_until = sim_seconds + ANOMALY_COOLDOWN_SECONDS;
                anomaly_events.push(SimEvent::AnomalyStarted { kind, label });
            }
        }

        let craft = self.spacecraft.as_mut().unwrap();

        if craft.safe_mode_until > 0.0 && sim_seconds >= craft.safe_mode_until {
            craft.safe_mode_until = 0.0;
            if craft.payload_mode == PayloadMode::Safe
                && let Some(mode) = craft.payload_mode_before_safe.take()
            {
                craft.payload_mode = mode;
            }
        }

        let downlink_pass = craft
            .downlink_station_id
            .as_ref()
            .and_then(|id| {
                self.ground_stations
                    .iter()
                    .position(|s| &s.id == id)
                    .map(|i| station_passes[i])
            })
            .unwrap_or_else(|| station_passes.iter().any(|&p| p));

        let anomaly_solar_penalty = self
            .bodies
            .body(body_idx)
            .anomaly_flavor
            .as_ref()
            .map_or(1.0, |f| f.solar_penalty_while_active);
        let balance =
            craft.power_balance_w(sunlight, solar_mult, sim_seconds, anomaly_solar_penalty);
        craft.power_wh = (craft.power_wh + balance * dt / 3600.0).clamp(0.0, craft.bus.battery_wh);

        // Imaging needs power, like downlink below.
        if craft.power_wh > 1.0 {
            let imaging = craft.imaging_rate_mb_s(sunlight, sim_seconds) * dt;
            craft.buffer_mb = (craft.buffer_mb + imaging).min(craft.max_buffer_mb());
        }

        let mut transferred = 0.0;
        if craft.downlink_active && downlink_pass && craft.buffer_mb > 0.0 && craft.power_wh > 1.0 {
            let body = self.bodies.body(body_idx);
            let station_bandwidth = craft
                .downlink_station_id
                .as_ref()
                .and_then(|id| self.ground_stations.iter().find(|s| &s.id == id))
                .map(|s| {
                    Self::station_bandwidth_for(s, body, self.company.deep_space_comms_multiplier)
                });
            let rate = craft.downlink_rate_mb_s(comms_mult, station_bandwidth, sim_seconds);
            transferred = (rate * dt).min(craft.buffer_mb);
            craft.buffer_mb -= transferred;
        }

        if transferred > 0.0
            && let Some(station_id) = craft.downlink_station_id.clone()
            && let Some(contract) = self.company.active_contract.as_mut()
        {
            // Data downlinked from a non-compliant orbit is not accepted.
            if contract.orbit_ok(&elements, radius) {
                contract.downlink_mb_done += transferred;
                if !contract.stations_used.contains(&station_id) {
                    contract.stations_used.push(station_id);
                }
            }
        }

        // Presence contracts: time in a compliant orbit counts on its own.
        if let Some(contract) = self.company.active_contract.as_mut()
            && contract.min_on_station_days > 0.0
            && contract.body_id == craft_body_id
            && contract.orbit_ok(&elements, radius)
        {
            contract.on_station_seconds += dt;
        }

        for event in pass_events.into_iter().chain(anomaly_events) {
            self.push_event(event);
        }
        self.check_contract();
    }

    fn try_execute_queued_command(&mut self, station_id: &str, station_name: &str) {
        let Some(craft) = self.spacecraft.as_mut() else {
            return;
        };
        let idx = craft
            .command_queue
            .iter()
            .position(|c| c.station_id == station_id);
        let Some(idx) = idx else { return };
        let cmd = craft.command_queue.remove(idx);
        let msg = execute_command(&cmd.command).to_string();
        match cmd.command {
            CommandType::RebootComms => {
                if let Some(a) = &craft.active_anomaly
                    && a.kind == AnomalyType::CommsDropout
                {
                    craft.clear_anomaly(self.clock.sim_seconds);
                    self.push_event(SimEvent::AnomalyCleared {
                        kind: AnomalyType::CommsDropout,
                    });
                }
            }
            CommandType::SafeMode => {
                if craft.payload_mode != PayloadMode::Safe {
                    craft.payload_mode_before_safe = Some(craft.payload_mode);
                }
                craft.safe_mode_until = self.clock.sim_seconds + 300.0;
                craft.payload_mode = PayloadMode::Safe;
                // Safing also halves the remaining anomaly time.
                if let Some(a) = craft.active_anomaly.as_mut() {
                    let elapsed = self.clock.sim_seconds - a.started_at_sim_seconds;
                    let remaining = (a.duration_seconds - elapsed).max(0.0);
                    a.duration_seconds = elapsed + remaining * 0.5;
                }
            }
            CommandType::ClearAnomaly => {
                if let Some(a) = craft.active_anomaly.clone() {
                    craft.clear_anomaly(self.clock.sim_seconds);
                    self.push_event(SimEvent::AnomalyCleared { kind: a.kind });
                }
            }
        }
        self.push_event(SimEvent::CommandExecuted {
            command: format!("{:?}", cmd.command).to_lowercase(),
            station_name: station_name.to_string(),
            message: msg,
        });
    }

    pub fn check_contract(&mut self) {
        let Some(contract) = self.company.active_contract.as_mut() else {
            return;
        };

        if contract.is_complete() {
            // The advance was cash on acceptance: pay out the remainder.
            let total_payout = contract.payout_usd;
            let payout = total_payout - contract.advance_paid_usd;
            let rep = contract.reputation_reward;
            let id = contract.template_id.clone();
            let body_id = contract.body_id.clone();
            self.company.cash_usd += payout;
            self.company.completed_contracts.push(id.clone());
            self.company.reputation = (self.company.reputation + rep).min(300.0);
            self.push_event(SimEvent::ContractCompleted {
                contract_id: id,
                payout_usd: total_payout,
            });
            self.push_event(SimEvent::ReputationChanged {
                delta: rep,
                new_value: self.company.reputation,
            });
            self.company.active_contract = None;

            // Program milestone: every contract of the body done once.
            if self.company.game_mode == GameMode::Campaign
                && let Some(program) = self.content.program_for_body(&body_id).cloned()
                && !self.company.completed_programs.contains(&program.id)
                && self
                    .company
                    .program_contracts_complete(&self.content, &body_id)
            {
                self.company.completed_programs.push(program.id.clone());
                self.company.cash_usd += program.completion_grant_usd;
                self.push_event(SimEvent::ProgramCompleted {
                    program_id: program.id,
                    grant_usd: program.completion_grant_usd,
                });
            }
            if self.company.game_mode == GameMode::Campaign
                && self.company.outcome.is_none()
                && self.company.all_contracts_completed(&self.content)
            {
                self.company.outcome = Some(GameOutcome::Victory);
                self.push_event(SimEvent::GameWon);
            }
            return;
        }

        if self.clock.sim_seconds > contract.deadline_sim_seconds {
            let id = contract.template_id.clone();
            // Clawback: the advance goes back with an extra reputation hit.
            let advance = contract.advance_paid_usd;
            let mut penalty = self.company.failure_rep_penalty();
            if advance > 0.0 {
                self.company.cash_usd -= advance;
                penalty += 5.0;
            }
            self.company.reputation = (self.company.reputation - penalty).max(0.0);
            self.push_event(SimEvent::ContractFailed { contract_id: id });
            self.push_event(SimEvent::ReputationChanged {
                delta: -penalty,
                new_value: self.company.reputation,
            });
            self.company.active_contract = None;
            if self.company.game_mode == GameMode::Campaign
                && self.company.outcome.is_none()
                && self.company.reputation <= 0.0
            {
                self.company.outcome = Some(GameOutcome::ReputationLost);
                self.push_event(SimEvent::GameOver {
                    reason: GameOutcome::ReputationLost,
                });
            }
        }
    }

    /// Current recurring burn rate ($/sim-day): base overhead plus the
    /// on-orbit bus ops cost. Zero in sandbox and after any outcome.
    pub fn current_ops_cost_usd_per_day(&self) -> f64 {
        if self.company.game_mode != GameMode::Campaign || self.company.outcome.is_some() {
            return 0.0;
        }
        let craft_cost = self.spacecraft.as_ref().map_or(0.0, |c| match c.phase {
            CraftPhase::OnOrbit => c.bus.ops_cost_usd_per_day,
            // Hibernated cruise: a skeleton crew on console.
            CraftPhase::Transfer => c.bus.ops_cost_usd_per_day * HIBERNATION_OPS_FRACTION,
            CraftPhase::Design => 0.0,
        });
        (BASE_OVERHEAD_USD_PER_DAY + craft_cost) * self.company.ops_cost_multiplier
    }

    /// Dropping the return value loses the events permanently.
    #[must_use]
    pub fn drain_events(&mut self) -> Vec<SimEvent> {
        std::mem::take(&mut self.event_queue)
    }

    fn reference_satellites(&self) -> Vec<ReferenceSatellite> {
        let mut cache = self.ref_sat_cache.borrow_mut();
        if let Some((tick, sats)) = cache.as_ref()
            && *tick == self.clock.tick
        {
            return sats.clone();
        }
        let sats: Vec<ReferenceSatellite> = self
            .tle_propagators
            .iter()
            .filter_map(|p| p.propagate(self.clock.sim_seconds).ok())
            .collect();
        *cache = Some((self.clock.tick, sats.clone()));
        sats
    }

    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        let game_phase = self.derive_game_phase();
        let marketplace = self.company.marketplace(&self.content);
        let craft_body = self.craft_body();
        let craft_body_idx = self.bodies.idx(&craft_body.id).expect("validated body id");
        let sun_dir = self
            .bodies
            .sun_direction_from(craft_body_idx, self.clock.sim_seconds);
        let earth_los_now = |elements: &crate::orbit::KeplerElements| {
            if craft_body.id == "earth" {
                false
            } else {
                let dir = self.earth_direction_from(&craft_body.id, self.clock.sim_seconds);
                let eci = eci_position_km(craft_body.mu_km3_s2, elements, self.clock.sim_seconds);
                in_sunlight(eci, dir, craft_body.radius_km)
            }
        };

        let spacecraft = self.spacecraft.as_ref().map(|craft| {
            let (position, velocity, trail, in_pass, sunlight) = if let Some(elements) =
                craft.elements
            {
                let mu = craft_body.mu_km3_s2;
                let eci = eci_position_km(mu, &elements, self.clock.sim_seconds);
                let vel = eci_velocity_km_s(mu, &elements, self.clock.sim_seconds);
                let trail = orbit_trail_samples(mu, &elements, self.clock.sim_seconds, 128);
                let ecef = eci_to_ecef_km(craft_body.rotation_rad_s, eci, self.clock.sim_seconds);
                let los = earth_los_now(&elements);
                let in_pass = self.ground_stations.iter().any(|s| {
                    if s.body_id == craft_body.id {
                        is_in_pass(
                            craft_body.radius_km,
                            s.latitude_deg,
                            s.longitude_deg,
                            s.min_elevation_deg,
                            ecef,
                        )
                    } else {
                        s.dsn && los
                    }
                });
                let sunlight = in_sunlight(eci, sun_dir, craft_body.radius_km);
                (eci, vel, trail, in_pass, sunlight)
            } else {
                ([0.0; 3], [0.0; 3], vec![], false, true)
            };

            let apsis = craft.elements.is_some_and(|_| at_apsis(position, velocity));
            let (seconds_to_perigee, seconds_to_apogee) = craft
                .elements
                .filter(|_| craft.phase == CraftPhase::OnOrbit)
                .map_or((None, None), |e| {
                    let (p, a) =
                        seconds_to_apsides(craft_body.mu_km3_s2, &e, self.clock.sim_seconds);
                    (Some(p), Some(a))
                });
            let hohmann_preview =
                if craft.phase == CraftPhase::OnOrbit && craft.pending_hohmann.is_none() {
                    preview_hohmann(craft_body, craft, craft.maneuver_target_altitude_km)
                        .ok()
                        .map(|p| HohmannPlanSnapshot::from(&p))
                } else {
                    None
                };

            let (anomaly_type, anomaly_label) =
                craft.active_anomaly.as_ref().map_or((None, None), |a| {
                    (Some(a.kind), Some(a.label().to_string()))
                });

            SpacecraftSnapshot {
                name: craft.name.clone(),
                bus_id: craft.bus_id.clone(),
                bus_name: craft.bus.name.clone(),
                phase: craft.phase,
                body_id: craft.body_id.clone(),
                position_eci_km: position,
                velocity_eci_km_s: velocity,
                elements: craft.elements,
                orbit_trail_km: trail,
                power_wh: craft.power_wh,
                battery_wh_capacity: craft.bus.battery_wh,
                buffer_mb: craft.buffer_mb,
                buffer_capacity_mb: craft.max_buffer_mb(),
                downlink_active: craft.downlink_active,
                in_sunlight: sunlight,
                in_pass,
                downlink_rate_mb_s: craft.downlink_rate_mb_s(
                    self.company.comms_multiplier,
                    craft
                        .downlink_station_id
                        .as_ref()
                        .and_then(|id| self.ground_stations.iter().find(|s| &s.id == id))
                        .map(|s| {
                            Self::station_bandwidth_for(
                                s,
                                craft_body,
                                self.company.deep_space_comms_multiplier,
                            )
                        }),
                    self.clock.sim_seconds,
                ),
                imaging_rate_mb_s: craft.imaging_rate_mb_s(sunlight, self.clock.sim_seconds),
                target_altitude_km: craft.target_altitude_km,
                target_inclination_deg: craft.target_inclination_deg,
                altitude_km: craft
                    .current_altitude_km(craft_body.radius_km)
                    .unwrap_or(0.0),
                propellant_kg: craft.propellant_kg,
                propellant_capacity_kg: craft.bus.propellant_kg,
                delta_v_remaining_km_s: craft.delta_v_remaining_km_s(self.company.isp_multiplier),
                total_delta_v_used_km_s: craft.total_delta_v_used_km_s,
                pending_hohmann: craft.pending_hohmann.clone(),
                at_apsis: apsis,
                seconds_to_perigee,
                seconds_to_apogee,
                hohmann_preview,
                maneuver_target_altitude_km: craft.maneuver_target_altitude_km,
                payload_mode: craft.payload_mode,
                downlink_station_id: craft.downlink_station_id.clone(),
                command_queue: craft.command_queue.clone(),
                active_anomaly: anomaly_type,
                anomaly_label,
                transfer: craft.transfer.as_ref().map(|t| {
                    let span = (t.arrival_sim_seconds - t.departure_sim_seconds).max(1.0);
                    TransferSnapshot {
                        origin_body_id: t.origin_body_id.clone(),
                        target_body_id: t.target_body_id.clone(),
                        frame_body_id: t.frame_body_id.clone(),
                        departure_sim_seconds: t.departure_sim_seconds,
                        arrival_sim_seconds: t.arrival_sim_seconds,
                        progress: ((self.clock.sim_seconds - t.departure_sim_seconds) / span)
                            .clamp(0.0, 1.0),
                        capture_dv_km_s: t.capture_dv_km_s,
                    }
                }),
                transfer_planner: self.transfer_planner_snapshot(craft),
                light_delay_seconds: {
                    const C_KM_S: f64 = 299_792.458;
                    let earth_idx = self.bodies.idx("earth").expect("registry holds earth");
                    let e = self
                        .bodies
                        .heliocentric_position_km(earth_idx, self.clock.sim_seconds);
                    let b = self
                        .bodies
                        .heliocentric_position_km(craft_body_idx, self.clock.sim_seconds);
                    let craft_helio = [b[0] + position[0], b[1] + position[1], b[2] + position[2]];
                    let d = [
                        craft_helio[0] - e[0],
                        craft_helio[1] - e[1],
                        craft_helio[2] - e[2],
                    ];
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() / C_KM_S
                },
            }
        });

        let ground_stations: Vec<StationSnapshot> = self
            .ground_stations
            .iter()
            .map(|station| {
                let local = station.body_id == craft_body.id;
                let deep_space = station.dsn && !local && craft_body.id != "earth";
                let elements = self.spacecraft.as_ref().and_then(|c| c.elements);
                let elevation = match (local, elements) {
                    (true, Some(elements)) => {
                        let eci = eci_position_km(
                            craft_body.mu_km3_s2,
                            &elements,
                            self.clock.sim_seconds,
                        );
                        let ecef =
                            eci_to_ecef_km(craft_body.rotation_rad_s, eci, self.clock.sim_seconds);
                        crate::ops::elevation_deg(
                            craft_body.radius_km,
                            station.latitude_deg,
                            station.longitude_deg,
                            ecef,
                        )
                    }
                    _ => -90.0,
                };
                let in_pass = if local {
                    elevation >= station.min_elevation_deg
                } else {
                    deep_space && elements.is_some_and(|e| earth_los_now(&e))
                };
                let next_pass_in_seconds = if in_pass {
                    None
                } else if local {
                    elements.and_then(|elements| {
                        crate::ops::seconds_until_pass(
                            craft_body,
                            &elements,
                            station.latitude_deg,
                            station.longitude_deg,
                            station.min_elevation_deg,
                            self.clock.sim_seconds,
                            7200.0,
                            15.0,
                        )
                    })
                } else if deep_space {
                    elements.and_then(|elements| {
                        self.seconds_until_los(
                            craft_body,
                            &elements,
                            self.clock.sim_seconds,
                            7200.0,
                            15.0,
                        )
                    })
                } else {
                    None
                };
                StationSnapshot {
                    id: station.id.clone(),
                    name: station.name.clone(),
                    latitude_deg: station.latitude_deg,
                    longitude_deg: station.longitude_deg,
                    elevation_deg: elevation,
                    in_pass,
                    bandwidth_mbps: Self::station_bandwidth_for(
                        station,
                        craft_body,
                        self.company.deep_space_comms_multiplier,
                    ),
                    next_pass_in_seconds,
                    body_id: station.body_id.clone(),
                    dsn: station.dsn,
                }
            })
            .collect();

        let active_contract_orbit_ok = self.company.active_contract.as_ref().and_then(|c| {
            self.spacecraft.as_ref().and_then(|craft| {
                craft
                    .elements
                    .map(|e| c.body_id == craft.body_id && c.orbit_ok(&e, craft_body.radius_km))
            })
        });

        let bodies: Vec<BodySnapshot> = (0..self.bodies.len())
            .map(|i| {
                let b = self.bodies.body(i);
                BodySnapshot {
                    id: b.id.clone(),
                    name: b.name.clone(),
                    parent_id: b.parent.map(|p| self.bodies.body(p).id.clone()),
                    radius_km: b.radius_km,
                    rotation_rad_s: b.rotation_rad_s,
                    position_helio_km: self
                        .bodies
                        .heliocentric_position_km(i, self.clock.sim_seconds),
                    position_parent_km: self
                        .bodies
                        .parent_relative_position_km(i, self.clock.sim_seconds),
                    sun_direction: self.bodies.sun_direction_from(i, self.clock.sim_seconds),
                    soi_radius_km: b.soi_radius_km.is_finite().then_some(b.soi_radius_km),
                    texture: b.texture.clone(),
                }
            })
            .collect();

        Snapshot {
            bodies,
            epoch_utc: self.content.epoch_utc.clone(),
            sim_seconds: self.clock.sim_seconds,
            sim_days: self.clock.sim_days(),
            time_accel: self.clock.time_accel,
            tick: self.clock.tick,
            company_name: self.company.name.clone(),
            cash_usd: self.company.cash_usd,
            reputation: self.company.reputation,
            game_mode: self.company.game_mode,
            unlocked_tech: self.company.unlocked_tech.clone(),
            unlocked_buses: self.company.unlocked_buses.clone(),
            marketplace,
            active_contract: self.company.active_contract.clone(),
            active_contract_orbit_ok,
            spacecraft,
            ground_stations,
            reference_satellites: self.reference_satellites(),
            phase: game_phase,
            outcome: self.company.outcome,
            ops_cost_usd_per_day: self.current_ops_cost_usd_per_day(),
            transfers_unlocked: self.transfers_unlocked(),
            programs: self
                .content
                .programs
                .iter()
                .map(|p| ProgramSnapshot {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    body_id: p.body_id.clone(),
                    buyin_usd: p.buyin_usd,
                    reputation_required: p.reputation_required,
                    requires_keystone: p.requires_keystone.clone(),
                    keystone_done: p
                        .requires_keystone
                        .as_ref()
                        .is_none_or(|k| self.company.completed_contracts.contains(k)),
                    unlocked: self.company.unlocked_programs.contains(&p.id),
                    completed: self.company.completed_programs.contains(&p.id),
                    completion_grant_usd: p.completion_grant_usd,
                })
                .collect(),
        }
    }

    /// Sandbox always; campaign once Deep Space Operations is researched.
    fn transfers_unlocked(&self) -> bool {
        self.company.game_mode == GameMode::Sandbox
            || self
                .company
                .unlocked_tech
                .iter()
                .any(|t| t == "deep_space_ops")
    }

    /// Planner readout for the craft's selected destination.
    fn transfer_planner_snapshot(&self, craft: &Spacecraft) -> Option<TransferPlannerSnapshot> {
        if craft.phase != CraftPhase::OnOrbit {
            return None;
        }
        let target_sel = craft.transfer_target.as_ref()?;
        let elements = craft.elements?;
        let origin_idx = self.bodies.idx(&craft.body_id)?;
        let target_idx = self.bodies.idx(&target_sel.body_id)?;
        let now = self.clock.sim_seconds;

        let plan = crate::orbit::plan_transfer(
            &self.bodies,
            origin_idx,
            target_idx,
            elements.semi_major_axis_km,
            target_sel.capture_altitude_km,
            now,
        )
        .ok()?;
        let window_open =
            crate::orbit::window_open(&self.bodies, origin_idx, target_idx, now).ok()?;
        let next_window =
            crate::orbit::next_launch_window(&self.bodies, origin_idx, target_idx, now).ok()?;
        let target_body = self.bodies.body(target_idx);

        // Sibling transfers expose the phase angles; moon-style ones don't
        // have a meaningful one (windows are effectively always open).
        let sibling = self.bodies.body(origin_idx).parent == target_body.parent;
        let (phase, required) = if sibling {
            (
                Some(crate::orbit::phase_angle_deg(
                    &self.bodies,
                    origin_idx,
                    target_idx,
                    now,
                )),
                crate::orbit::required_phase_angle_deg(&self.bodies, origin_idx, target_idx).ok(),
            )
        } else {
            (None, None)
        };

        let capture_dv = if target_body.has_atmosphere {
            plan.capture_dv_km_s * self.company.capture_dv_multiplier
        } else {
            plan.capture_dv_km_s
        };
        let isp = craft.bus.isp_s * self.company.isp_multiplier;
        let capture_propellant_kg = crate::orbit::propellant_for_delta_v(
            capture_dv,
            craft.dry_mass_kg(),
            craft.propellant_kg,
            isp,
        );
        let injection_cost_usd = craft.bus.launch_cost_usd * target_body.injection_cost_multiplier;

        Some(TransferPlannerSnapshot {
            target_body_id: target_sel.body_id.clone(),
            window_open,
            phase_angle_deg: phase,
            required_phase_angle_deg: required,
            next_window_sim_seconds: next_window,
            transfer_seconds: plan.transfer_seconds,
            departure_dv_km_s: plan.departure_dv_km_s,
            capture_dv_km_s: capture_dv,
            capture_propellant_kg,
            injection_cost_usd,
            affordable: self.company.can_afford(injection_cost_usd),
            propellant_ok: capture_propellant_kg <= craft.propellant_kg + 1e-6,
        })
    }

    fn derive_game_phase(&self) -> GamePhase {
        match self.company.outcome {
            Some(GameOutcome::Victory) => return GamePhase::Victory,
            Some(GameOutcome::Bankrupt | GameOutcome::ReputationLost) => {
                return GamePhase::GameOver;
            }
            None => {}
        }
        match self.spacecraft.as_ref().map(|s| &s.phase) {
            None => GamePhase::Design,
            Some(CraftPhase::Design) => {
                if self.company.active_contract.is_some() {
                    GamePhase::AwaitingLaunch
                } else {
                    GamePhase::Design
                }
            }
            Some(CraftPhase::Transfer) => GamePhase::InTransit,
            Some(CraftPhase::OnOrbit) => {
                if self.company.active_contract.is_none()
                    && !self.company.completed_contracts.is_empty()
                {
                    GamePhase::ContractComplete
                } else {
                    GamePhase::OnOrbit
                }
            }
        }
    }

    fn spacecraft_mut(&mut self, required: CraftPhase) -> Result<&mut Spacecraft, SimError> {
        let craft = self
            .spacecraft
            .as_mut()
            .ok_or_else(|| SimError::InvalidAction("no spacecraft".into()))?;
        if craft.phase != required {
            return Err(SimError::WrongPhase);
        }
        Ok(craft)
    }

    fn push_event(&mut self, event: SimEvent) {
        self.event_queue.push(event);
    }
}

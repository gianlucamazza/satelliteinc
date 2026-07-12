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
    ActiveContractSnapshot, BodySnapshot, CraftStationStatus, GamePhase, ProgramSnapshot, Snapshot,
    SpacecraftSnapshot, StationSnapshot, TransferPlannerSnapshot, TransferSnapshot,
};
use crate::spacecraft::{CraftId, CraftPhase, HohmannPlanSnapshot, PayloadMode, Spacecraft};

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
/// Mission Control baseline: concurrent flights before any growth.
pub const BASE_FLIGHT_SLOTS: usize = 2;
/// Daily staffing cost of each slot beyond the base two.
pub const SLOT_OVERHEAD_USD_PER_DAY: f64 = 2_000.0;
/// Sandbox flies free of the campaign's slot economics (soft cap).
pub const SANDBOX_FLIGHT_SLOTS: usize = 6;
/// Bankruptcy triggers this many days of burn below zero (min $50k).
pub const BANKRUPTCY_RUNWAY_DAYS: f64 = 3.0;

pub struct SimState {
    pub clock: SimClock,
    pub company: crate::company::CompanyState,
    /// The fleet, in creation order (deterministic iteration).
    pub fleet: Vec<Spacecraft>,
    /// Monotonic craft id source; never reused.
    pub next_craft_id: CraftId,
    pub bodies: BodyRegistry,
    pub ground_stations: Vec<GroundStation>,
    pub content: GameContent,
    pub event_queue: Vec<SimEvent>,
    pub tle_propagators: Vec<TlePropagator>,
    /// Pass edge state per craft per station: `pass_active[craft_idx][station_idx]`,
    /// rows kept in lockstep with `fleet` by `add_craft`/`remove_craft` only.
    pub pass_active: Vec<Vec<bool>>,
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
            fleet: Vec::new(),
            next_craft_id: 0,
            bodies,
            pass_active: Vec::new(),
            ground_stations,
            content,
            event_queue: Vec::new(),
            tle_propagators,
            ref_sat_cache: std::cell::RefCell::new(None),
        })
    }

    /// Central body of a craft. `body_id` is validated on load/import.
    pub fn craft_body_of(&self, craft: &Spacecraft) -> &CelestialBody {
        self.bodies
            .get(&craft.body_id)
            .expect("spacecraft body_id validated on load")
    }

    /// Craft by id.
    pub fn craft(&self, id: CraftId) -> Result<&Spacecraft, SimError> {
        self.fleet
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| SimError::InvalidAction(format!("no such craft: {id}")))
    }

    pub fn craft_mut(&mut self, id: CraftId) -> Result<&mut Spacecraft, SimError> {
        self.fleet
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| SimError::InvalidAction(format!("no such craft: {id}")))
    }

    fn craft_idx(&self, id: CraftId) -> Option<usize> {
        self.fleet.iter().position(|c| c.id == id)
    }

    fn craft_in_phase_mut(
        &mut self,
        id: CraftId,
        required: CraftPhase,
    ) -> Result<&mut Spacecraft, SimError> {
        let craft = self.craft_mut(id)?;
        if craft.phase != required {
            return Err(SimError::WrongPhase);
        }
        Ok(craft)
    }

    /// The ONLY way fleet/pass_active rows are added: keeps them in lockstep.
    pub(crate) fn add_craft(&mut self, craft: Spacecraft) -> CraftId {
        let id = craft.id;
        self.next_craft_id = self.next_craft_id.max(id + 1);
        self.fleet.push(craft);
        self.pass_active
            .push(vec![false; self.ground_stations.len()]);
        id
    }

    /// The ONLY way fleet/pass_active rows are removed. Clears contract
    /// assignments pointing at the removed craft.
    fn remove_craft(&mut self, idx: usize) -> Spacecraft {
        let craft = self.fleet.remove(idx);
        self.pass_active.remove(idx);
        for contract in self
            .company
            .active_contracts
            .iter_mut()
            .filter(|c| c.assigned_craft_id == Some(craft.id))
        {
            contract.assigned_craft_id = None;
            self.event_queue.push(SimEvent::ContractAssignmentChanged {
                contract_id: contract.template_id.clone(),
                craft_id: None,
            });
        }
        craft
    }

    /// Contract cap: M14a keeps the historical single slot; Mission
    /// Control slots (M15) will raise this.
    /// Concurrent flights allowed: 2 base, +1 per completed Program,
    /// +R&D annexes. You pay for each slot beyond the base (see ops cost).
    pub fn max_flights(&self) -> usize {
        if self.company.game_mode == GameMode::Sandbox {
            return SANDBOX_FLIGHT_SLOTS;
        }
        BASE_FLIGHT_SLOTS
            + self.company.completed_programs.len()
            + self.company.extra_flight_slots as usize
    }

    fn max_active_contracts(&self) -> usize {
        self.max_flights()
    }

    /// Craft currently flying (on orbit or cruising).
    fn active_flights(&self) -> usize {
        self.fleet
            .iter()
            .filter(|c| c.phase != CraftPhase::Design)
            .count()
    }

    /// The contract currently assigned to a craft, if any (1:1 binding).
    fn contract_assigned_to(&self, craft_id: CraftId) -> Option<&ActiveContract> {
        self.company
            .active_contracts
            .iter()
            .find(|c| c.assigned_craft_id == Some(craft_id))
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
    fn begin_transfer(&mut self, craft_id: CraftId) -> Result<(), SimError> {
        let now = self.clock.sim_seconds;
        let craft = self.craft(craft_id)?;
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
        let idx = self.craft_idx(craft_id).expect("checked above");
        let craft = &mut self.fleet[idx];
        craft.propellant_kg -= reserved_kg;
        craft.phase = CraftPhase::Transfer;
        craft.body_id = frame_body_id;
        craft.elements = Some(plan.elements);
        craft.pending_hohmann = None;
        craft.downlink_active = false;
        craft.transfer = Some(transfer);
        self.pass_active[idx].fill(false);
        self.push_event(SimEvent::TransferStarted {
            craft_id,
            target_body_id: target_sel.body_id,
            arrival_sim_seconds: arrival,
            injection_cost_usd: injection_cost,
        });
        Ok(())
    }

    /// Arrival: automatic capture into the reserved parking orbit. The
    /// propellant was deducted at departure, so this can never strand.
    fn complete_arrival(&mut self, idx: usize) {
        let craft = &mut self.fleet[idx];
        let craft_id = craft.id;
        let Some(transfer) = craft.transfer.take() else {
            return;
        };
        let target = self
            .bodies
            .get(&transfer.target_body_id)
            .expect("transfer target validated at departure");
        craft.body_id.clone_from(&target.id);
        let mut capture_elements = circular_orbit(
            target.radius_km,
            transfer.capture_altitude_km,
            transfer.capture_inclination_deg,
            transfer.arrival_sim_seconds,
        );
        // Giants capture into a high-e ellipse (periapsis at the chosen
        // altitude) — the burn priced by the planner assumed exactly this.
        if target.capture_eccentricity > 0.0 {
            let r_p = target.radius_km + transfer.capture_altitude_km;
            capture_elements.eccentricity = target.capture_eccentricity;
            capture_elements.semi_major_axis_km = r_p / (1.0 - target.capture_eccentricity);
        }
        craft.elements = Some(capture_elements);
        craft.phase = CraftPhase::OnOrbit;
        craft.total_delta_v_used_km_s += transfer.capture_dv_km_s;
        craft.transfer_target = None;
        let target_id = target.id.clone();
        let default_station = self
            .ground_stations
            .iter()
            .find(|st| Self::station_reachable(st, &target_id))
            .map(|st| st.id.clone());
        self.fleet[idx].downlink_station_id = default_station;
        self.pass_active[idx].fill(false);
        self.push_event(SimEvent::TransferArrived {
            craft_id,
            body_id: target_id,
        });
        self.push_event(SimEvent::CaptureExecuted {
            craft_id,
            delta_v_km_s: transfer.capture_dv_km_s,
        });
    }

    /// Deterministic themed default name: pool by bus class, Roman-suffixed
    /// on reuse ("Kestrel II"). Craft attachment is cheap and load-bearing.
    fn default_craft_name(bus_id: &str, id: CraftId) -> String {
        const CUBESAT: [&str; 4] = ["Sparrow", "Wren", "Finch", "Robin"];
        const MICROSAT: [&str; 4] = ["Merlin", "Osprey", "Harrier", "Kite"];
        const SMALLSAT: [&str; 4] = ["Falcon", "Condor", "Eagle", "Gyrfalcon"];
        const EXPLORER: [&str; 4] = ["Kestrel", "Albatross", "Petrel", "Skua"];
        const PIONEER: [&str; 4] = ["Voyager", "Ranger", "Surveyor", "Mariner"];
        const ROMAN: [&str; 8] = ["", " II", " III", " IV", " V", " VI", " VII", " VIII"];
        let pool: &[&str] = match bus_id {
            "microsat" => &MICROSAT,
            "smallsat" => &SMALLSAT,
            "explorer" => &EXPLORER,
            "pioneer" => &PIONEER,
            _ => &CUBESAT,
        };
        let idx = id as usize % pool.len();
        let round = id as usize / pool.len();
        format!("{}{}", pool[idx], ROMAN[round.min(ROMAN.len() - 1)])
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
                // Re-selecting while a craft is still on the drawing board
                // replaces it (same UX as before the fleet); flying craft
                // are untouched.
                if let Some(idx) = self
                    .fleet
                    .iter()
                    .position(|c| c.phase == CraftPhase::Design)
                {
                    self.remove_craft(idx);
                }
                let id = self.next_craft_id;
                let name = Self::default_craft_name(&bus.id, id);
                self.add_craft(Spacecraft::new_from_bus(&bus, &name, id));
                self.push_event(SimEvent::ActionApplied {
                    message: format!("Selected bus: {} ({name})", bus.name),
                });
            }
            PlayerAction::SetOrbit {
                craft_id,
                altitude_km,
                inclination_deg,
            } => {
                let (min_park, max_park) = {
                    let body = self.craft_body_of(self.craft(craft_id)?);
                    (body.min_parking_altitude_km, body.max_parking_altitude_km)
                };
                let craft = self.craft_in_phase_mut(craft_id, CraftPhase::Design)?;
                craft.target_altitude_km = altitude_km.clamp(min_park, max_park);
                craft.target_inclination_deg = inclination_deg.clamp(0.0, 98.0);
            }
            PlayerAction::SetDesignBody { craft_id, body_id } => {
                if self.company.game_mode != GameMode::Sandbox {
                    return Err(SimError::InvalidAction(
                        "staging at another body is a sandbox convenience — in \
                         campaign you fly there"
                            .into(),
                    ));
                }
                let body = self
                    .bodies
                    .get(&body_id)
                    .ok_or_else(|| SimError::Content(format!("unknown body: {body_id}")))?;
                if body.id == "sun" {
                    return Err(SimError::InvalidAction("cannot stage at the sun".into()));
                }
                let (min_park, max_park) =
                    (body.min_parking_altitude_km, body.max_parking_altitude_km);
                let craft = self.craft_in_phase_mut(craft_id, CraftPhase::Design)?;
                craft.body_id = body_id;
                craft.target_altitude_km = craft.target_altitude_km.clamp(min_park, max_park);
            }
            PlayerAction::AcceptContract {
                contract_id,
                assigned_craft_id,
            } => {
                if self.company.active_contracts.len() >= self.max_active_contracts() {
                    return Err(SimError::InvalidAction("contract slots full".into()));
                }
                if self.company.contract_by_template(&contract_id).is_some() {
                    return Err(SimError::InvalidAction("contract already active".into()));
                }
                if let Some(craft_id) = assigned_craft_id {
                    self.craft(craft_id)?;
                    if self.contract_assigned_to(craft_id).is_some() {
                        return Err(SimError::InvalidAction(
                            "craft already has an assigned contract".into(),
                        ));
                    }
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
                contract.assigned_craft_id = assigned_craft_id;
                let advance = contract.advance_paid_usd;
                self.company.active_contracts.push(contract);
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
            PlayerAction::Launch { craft_id } => {
                let max = self.max_flights();
                if self.active_flights() >= max {
                    return Err(SimError::InvalidAction(format!(
                        "mission control is saturated ({max} concurrent flights)"
                    )));
                }
                let (cost, alt, inc) = {
                    let craft = self.craft_in_phase_mut(craft_id, CraftPhase::Design)?;
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
                    self.craft_body_of(self.craft(craft_id)?).radius_km,
                    alt,
                    inc,
                    self.clock.sim_seconds,
                );
                let default_station = self.ground_stations.first().map(|s| s.id.clone());
                self.company.cash_usd -= cost;
                let now = self.clock.sim_seconds;
                let craft = self.craft_in_phase_mut(craft_id, CraftPhase::Design)?;
                craft.phase = CraftPhase::OnOrbit;
                craft.elements = Some(elements);
                craft.downlink_active = false;
                if craft.launched_at_sim_seconds.is_none() {
                    craft.launched_at_sim_seconds = Some(now);
                }
                if craft.downlink_station_id.is_none() {
                    craft.downlink_station_id = default_station;
                }
                let craft_name = craft.name.clone();
                self.push_event(SimEvent::Launched {
                    craft_id,
                    craft_name,
                    altitude_km: alt,
                    inclination_deg: inc,
                });
            }
            PlayerAction::SetDownlink { craft_id, active } => {
                let craft = self.craft_in_phase_mut(craft_id, CraftPhase::OnOrbit)?;
                craft.downlink_active = active;
            }
            PlayerAction::SetTimeAccel { multiplier } => {
                // 0 pauses the simulation.
                self.clock.time_accel = multiplier.clamp(0.0, 100_000.0);
            }
            PlayerAction::ProgradeBurn {
                craft_id,
                delta_v_km_s,
            } => {
                let isp_mult = self.company.isp_multiplier;
                let body = self.craft_body_of(self.craft(craft_id)?).clone();
                let now = self.clock.sim_seconds;
                let craft = self.craft_in_phase_mut(craft_id, CraftPhase::OnOrbit)?;
                let result = execute_prograde_burn(
                    &body,
                    craft,
                    now,
                    delta_v_km_s.clamp(0.001, 2.0),
                    false,
                    isp_mult,
                )?;
                for ev in result.events {
                    self.push_event(ev);
                }
            }
            PlayerAction::RetrogradeBurn {
                craft_id,
                delta_v_km_s,
            } => {
                let isp_mult = self.company.isp_multiplier;
                let body = self.craft_body_of(self.craft(craft_id)?).clone();
                let now = self.clock.sim_seconds;
                let craft = self.craft_in_phase_mut(craft_id, CraftPhase::OnOrbit)?;
                let result = execute_prograde_burn(
                    &body,
                    craft,
                    now,
                    delta_v_km_s.clamp(0.001, 2.0),
                    true,
                    isp_mult,
                )?;
                for ev in result.events {
                    self.push_event(ev);
                }
            }
            PlayerAction::SetManeuverTarget {
                craft_id,
                target_altitude_km,
            } => {
                let (min_park, max_park) = {
                    let body = self.craft_body_of(self.craft(craft_id)?);
                    (body.min_parking_altitude_km, body.max_parking_altitude_km)
                };
                let craft = self.craft_in_phase_mut(craft_id, CraftPhase::OnOrbit)?;
                craft.maneuver_target_altitude_km = target_altitude_km.clamp(min_park, max_park);
            }
            PlayerAction::StartHohmann { craft_id } => {
                let isp_mult = self.company.isp_multiplier;
                let body = self.craft_body_of(self.craft(craft_id)?).clone();
                let now = self.clock.sim_seconds;
                let craft = self.craft_in_phase_mut(craft_id, CraftPhase::OnOrbit)?;
                let target = craft.maneuver_target_altitude_km;
                let result = start_hohmann(&body, craft, now, target, isp_mult)?;
                for ev in result.events {
                    self.push_event(ev);
                }
            }
            PlayerAction::CompleteHohmann { craft_id } => {
                let isp_mult = self.company.isp_multiplier;
                let body = self.craft_body_of(self.craft(craft_id)?).clone();
                let now = self.clock.sim_seconds;
                let craft = self.craft_in_phase_mut(craft_id, CraftPhase::OnOrbit)?;
                let result = complete_hohmann(&body, craft, now, isp_mult)?;
                for ev in result.events {
                    self.push_event(ev);
                }
            }
            PlayerAction::Decommission { craft_id } => {
                let craft = self.craft(craft_id)?;
                let salvage = match craft.phase {
                    CraftPhase::OnOrbit => craft.bus.launch_cost_usd * SALVAGE_FRACTION,
                    // Deep space has no salvage yard: abandoning a cruise
                    // forfeits the craft outright.
                    CraftPhase::Transfer => 0.0,
                    CraftPhase::Design => return Err(SimError::WrongPhase),
                };
                let idx = self.craft_idx(craft_id).expect("checked above");
                let craft = self.remove_craft(idx);
                self.company.cash_usd += salvage;
                self.push_event(SimEvent::Decommissioned {
                    craft_id,
                    craft_name: craft.name,
                    salvage_usd: salvage,
                });
            }
            PlayerAction::SetPayloadMode { craft_id, mode } => {
                let craft = self.craft_in_phase_mut(craft_id, CraftPhase::OnOrbit)?;
                craft.payload_mode = mode;
                // Manual mode change overrides any commanded safe-mode hold.
                craft.safe_mode_until = 0.0;
                craft.payload_mode_before_safe = None;
            }
            PlayerAction::SelectDownlinkStation {
                craft_id,
                station_id,
            } => {
                let craft_body_id = self.craft_body_of(self.craft(craft_id)?).id.clone();
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
                let craft = self.craft_in_phase_mut(craft_id, CraftPhase::OnOrbit)?;
                craft.downlink_station_id = Some(station_id);
            }
            PlayerAction::QueueCommand {
                craft_id,
                command,
                station_id,
            } => {
                let craft_body_id = self.craft_body_of(self.craft(craft_id)?).id.clone();
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
                let craft = self.craft_in_phase_mut(craft_id, CraftPhase::OnOrbit)?;
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
                craft_id,
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
                let craft = self.craft_in_phase_mut(craft_id, CraftPhase::OnOrbit)?;
                craft.transfer_target = Some(crate::spacecraft::TransferTarget {
                    body_id,
                    capture_altitude_km: altitude,
                    capture_inclination_deg: inclination,
                });
            }
            PlayerAction::BeginTransfer { craft_id } => {
                self.begin_transfer(craft_id)?;
            }
            PlayerAction::RenameCraft { craft_id, name } => {
                let trimmed = name.trim();
                if trimmed.is_empty() || trimmed.len() > 40 {
                    return Err(SimError::InvalidAction(
                        "craft names are 1-40 characters".into(),
                    ));
                }
                let trimmed = trimmed.to_string();
                let craft = self.craft_mut(craft_id)?;
                craft.name = trimmed;
            }
            PlayerAction::AssignContract {
                contract_id,
                craft_id,
            } => {
                if let Some(id) = craft_id {
                    self.craft(id)?;
                    if self
                        .contract_assigned_to(id)
                        .is_some_and(|c| c.template_id != contract_id)
                    {
                        return Err(SimError::InvalidAction(
                            "craft already has an assigned contract".into(),
                        ));
                    }
                }
                let contract = self
                    .company
                    .active_contracts
                    .iter_mut()
                    .find(|c| c.template_id == contract_id)
                    .ok_or_else(|| {
                        SimError::InvalidAction(format!("contract not active: {contract_id}"))
                    })?;
                if contract.assigned_craft_id != craft_id {
                    // Anti tag-team: presence time restarts with a new crew.
                    contract.on_station_seconds = 0.0;
                }
                contract.assigned_craft_id = craft_id;
                self.push_event(SimEvent::ContractAssignmentChanged {
                    contract_id,
                    craft_id,
                });
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
                if let Some(tech_id) = &program.requires_tech
                    && !self.company.unlocked_tech.contains(tech_id)
                {
                    return Err(SimError::InvalidAction(format!(
                        "requires researching '{tech_id}' first"
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
            // The floor scales with the fleet's burn: a flat -50k would be
            // less than a day of margin for three active craft.
            let floor = BANKRUPTCY_FLOOR_USD
                .min(-self.current_ops_cost_usd_per_day() * BANKRUPTCY_RUNWAY_DAYS);
            if self.company.cash_usd < floor {
                self.company.outcome = Some(GameOutcome::Bankrupt);
                self.push_event(SimEvent::GameOver {
                    reason: GameOutcome::Bankrupt,
                });
                return;
            }
        }

        debug_assert_eq!(
            self.pass_active.len(),
            self.fleet.len(),
            "pass_active rows must track the fleet"
        );
        for idx in 0..self.fleet.len() {
            self.tick_craft(idx, dt);
        }
        self.check_contracts();
    }

    /// One craft's share of the tick: mechanical transcription of the old
    /// single-craft body, scoped to `fleet[idx]`.
    fn tick_craft(&mut self, idx: usize, dt: f64) {
        // Service record: this craft's share of the daily burn.
        if self.company.game_mode == GameMode::Campaign {
            let daily = Self::craft_ops_raw_usd_per_day(&self.fleet[idx])
                * self.company.ops_cost_multiplier;
            self.fleet[idx].lifetime_ops_usd += daily * dt / SECONDS_PER_DAY;
        }
        let craft_ref = &self.fleet[idx];
        let craft_id = craft_ref.id;
        if craft_ref.phase == CraftPhase::Transfer {
            // Hibernated cruise: no power/imaging/pass/anomaly processing —
            // only the arrival check (inside the 1 s tick, so warp and
            // manual acceleration are bit-identical).
            if craft_ref
                .transfer
                .as_ref()
                .is_some_and(|t| self.clock.sim_seconds >= t.arrival_sim_seconds)
            {
                self.complete_arrival(idx);
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
            let body = self.craft_body_of(craft_ref);
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
            let was_active = self.pass_active[idx][i];
            if in_pass && !was_active {
                pass_events.push(SimEvent::PassStarted {
                    craft_id,
                    station_id: station.id.clone(),
                    station_name: station.name.clone(),
                });
                pass_starts.push((station.id.clone(), station.name.clone()));
            } else if !in_pass && was_active {
                pass_events.push(SimEvent::PassEnded {
                    craft_id,
                    station_id: station.id.clone(),
                });
            }
            self.pass_active[idx][i] = in_pass;
        }
        for (id, name) in pass_starts {
            self.try_execute_queued_command(idx, &id, &name);
        }

        let mut anomaly_events = Vec::new();
        {
            let flavor = self.bodies.body(body_idx).anomaly_flavor.clone();
            let body_rate = self.bodies.body(body_idx).anomaly_rate_multiplier;
            let craft = &mut self.fleet[idx];
            if let Some(a) = craft.active_anomaly.clone()
                && a.is_expired(sim_seconds)
            {
                anomaly_events.push(SimEvent::AnomalyCleared {
                    craft_id,
                    kind: a.kind,
                });
                craft.active_anomaly = None;
                craft.anomaly_cooldown_until = sim_seconds + ANOMALY_COOLDOWN_SECONDS;
            }
            if craft.active_anomaly.is_none()
                && let Some(kind) = roll_anomaly(
                    sim_seconds,
                    dt,
                    craft.anomaly_cooldown_until,
                    self.company.anomaly_mtbf_multiplier / body_rate,
                    f64::from(craft_id) * crate::ops::ANOMALY_PHASE_STRIDE_SECONDS,
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
                    let fraction = flavor
                        .as_ref()
                        .map_or(0.25, |f| f.buffer_corruption_fraction);
                    craft.buffer_mb *= 1.0 - fraction;
                }
                craft.active_anomaly = Some(anomaly);
                craft.anomaly_cooldown_until = sim_seconds + ANOMALY_COOLDOWN_SECONDS;
                anomaly_events.push(SimEvent::AnomalyStarted {
                    craft_id,
                    kind,
                    label,
                });
            }
        }

        let anomaly_solar_penalty = self
            .bodies
            .body(body_idx)
            .anomaly_flavor
            .as_ref()
            .map_or(1.0, |f| f.solar_penalty_while_active);
        let deep_space_mult = self.company.deep_space_comms_multiplier;
        let body_dsn_factor = self.bodies.body(body_idx).dsn_bandwidth_factor;
        let _ = body_dsn_factor;
        let craft = &mut self.fleet[idx];

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
                .map(|s| Self::station_bandwidth_for(s, body, deep_space_mult));
            let rate = craft.downlink_rate_mb_s(comms_mult, station_bandwidth, sim_seconds);
            transferred = (rate * dt).min(craft.buffer_mb);
            craft.buffer_mb -= transferred;
        }
        let downlink_station_id = craft.downlink_station_id.clone();

        // Accrual routes ONLY through the assigned craft (1:1 binding).
        if transferred > 0.0
            && let Some(station_id) = downlink_station_id
            && let Some(contract) = self
                .company
                .active_contracts
                .iter_mut()
                .find(|c| c.assigned_craft_id == Some(craft_id))
        {
            // Data from the wrong body or a non-compliant orbit is not accepted.
            if contract.body_id == craft_body_id && contract.orbit_ok(&elements, radius) {
                contract.downlink_mb_done += transferred;
                if !contract.stations_used.contains(&station_id) {
                    contract.stations_used.push(station_id);
                }
            }
        }

        // Presence contracts: time in a compliant orbit counts on its own.
        if let Some(contract) = self
            .company
            .active_contracts
            .iter_mut()
            .find(|c| c.assigned_craft_id == Some(craft_id))
            && contract.min_on_station_days > 0.0
            && contract.body_id == craft_body_id
            && contract.orbit_ok(&elements, radius)
        {
            contract.on_station_seconds += dt;
        }

        for event in pass_events.into_iter().chain(anomaly_events) {
            self.push_event(event);
        }
    }

    fn try_execute_queued_command(
        &mut self,
        craft_idx: usize,
        station_id: &str,
        station_name: &str,
    ) {
        let craft = &mut self.fleet[craft_idx];
        let craft_id = craft.id;
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
                        craft_id,
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
                    self.push_event(SimEvent::AnomalyCleared {
                        craft_id,
                        kind: a.kind,
                    });
                }
            }
        }
        self.push_event(SimEvent::CommandExecuted {
            craft_id,
            command: format!("{:?}", cmd.command).to_lowercase(),
            station_name: station_name.to_string(),
            message: msg,
        });
    }

    pub fn check_contracts(&mut self) {
        let mut i = 0;
        while i < self.company.active_contracts.len() {
            let contract = &self.company.active_contracts[i];

            if contract.is_complete() {
                let contract = self.company.active_contracts.remove(i);
                // The advance was cash on acceptance: pay out the remainder.
                let total_payout = contract.payout_usd;
                let payout = total_payout - contract.advance_paid_usd;
                let rep = contract.reputation_reward;
                let id = contract.template_id.clone();
                let body_id = contract.body_id.clone();
                self.company.cash_usd += payout;
                self.company.completed_contracts.push(id.clone());
                self.company.reputation = (self.company.reputation + rep).min(300.0);
                if let Some(craft_id) = contract.assigned_craft_id
                    && let Some(craft) = self.fleet.iter_mut().find(|c| c.id == craft_id)
                {
                    craft.contracts_completed += 1;
                    craft.lifetime_revenue_usd += total_payout;
                }
                self.push_event(SimEvent::ContractCompleted {
                    contract_id: id,
                    payout_usd: total_payout,
                });
                self.push_event(SimEvent::ReputationChanged {
                    delta: rep,
                    new_value: self.company.reputation,
                });

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
                continue; // slot i now holds the next contract
            }

            if self.clock.sim_seconds > contract.deadline_sim_seconds {
                let contract = self.company.active_contracts.remove(i);
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
                if self.company.game_mode == GameMode::Campaign
                    && self.company.outcome.is_none()
                    && self.company.reputation <= 0.0
                {
                    self.company.outcome = Some(GameOutcome::ReputationLost);
                    self.push_event(SimEvent::GameOver {
                        reason: GameOutcome::ReputationLost,
                    });
                }
                continue;
            }
            i += 1;
        }
    }

    /// Current recurring burn rate ($/sim-day): base overhead plus the
    /// on-orbit bus ops cost. Zero in sandbox and after any outcome.
    pub fn current_ops_cost_usd_per_day(&self) -> f64 {
        if self.company.game_mode != GameMode::Campaign || self.company.outcome.is_some() {
            return 0.0;
        }
        let fleet_cost: f64 = self.fleet.iter().map(Self::craft_ops_raw_usd_per_day).sum();
        // Slots beyond the base two bill their staffing whether flown or not.
        let slot_overhead = SLOT_OVERHEAD_USD_PER_DAY
            * (self.max_flights().saturating_sub(BASE_FLIGHT_SLOTS)) as f64;
        (BASE_OVERHEAD_USD_PER_DAY + slot_overhead + fleet_cost) * self.company.ops_cost_multiplier
    }

    /// One craft's raw daily ops (before the company multiplier).
    fn craft_ops_raw_usd_per_day(craft: &Spacecraft) -> f64 {
        match craft.phase {
            CraftPhase::OnOrbit => craft.bus.ops_cost_usd_per_day,
            // Hibernated cruise: a skeleton crew on console.
            CraftPhase::Transfer => craft.bus.ops_cost_usd_per_day * HIBERNATION_OPS_FRACTION,
            CraftPhase::Design => 0.0,
        }
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
        let now = self.clock.sim_seconds;

        let spacecraft: Vec<SpacecraftSnapshot> = self
            .fleet
            .iter()
            .map(|craft| self.craft_snapshot(craft))
            .collect();

        // Static station facts only: craft-relative geometry lives in each
        // SpacecraftSnapshot's station_status.
        let ground_stations: Vec<StationSnapshot> = self
            .ground_stations
            .iter()
            .map(|station| StationSnapshot {
                id: station.id.clone(),
                name: station.name.clone(),
                latitude_deg: station.latitude_deg,
                longitude_deg: station.longitude_deg,
                bandwidth_mbps: station.bandwidth_mbps,
                body_id: station.body_id.clone(),
                dsn: station.dsn,
            })
            .collect();

        let active_contracts: Vec<ActiveContractSnapshot> = self
            .company
            .active_contracts
            .iter()
            .map(|c| {
                let orbit_ok = c.assigned_craft_id.and_then(|id| {
                    let craft = self.fleet.iter().find(|k| k.id == id)?;
                    let radius = self.craft_body_of(craft).radius_km;
                    craft
                        .elements
                        .map(|e| c.body_id == craft.body_id && c.orbit_ok(&e, radius))
                });
                ActiveContractSnapshot {
                    contract: c.clone(),
                    orbit_ok,
                }
            })
            .collect();
        let _ = now;

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
            active_contracts,
            spacecraft,
            ground_stations,
            reference_satellites: self.reference_satellites(),
            phase: game_phase,
            outcome: self.company.outcome,
            ops_cost_usd_per_day: self.current_ops_cost_usd_per_day(),
            transfers_unlocked: self.transfers_unlocked(),
            next_event_sim_seconds: self.next_fleet_event_sim_seconds(),
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
                    requires_tech: p.requires_tech.clone(),
                    tech_done: p
                        .requires_tech
                        .as_ref()
                        .is_none_or(|t| self.company.unlocked_tech.contains(t)),
                    unlocked: self.company.unlocked_programs.contains(&p.id),
                    completed: self.company.completed_programs.contains(&p.id),
                    completion_grant_usd: p.completion_grant_usd,
                })
                .collect(),
        }
    }

    /// One craft's snapshot: mechanical transcription of the old
    /// single-craft builder, with per-craft station geometry attached.
    fn craft_snapshot(&self, craft: &Spacecraft) -> SpacecraftSnapshot {
        let craft_body = self.craft_body_of(craft);
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

        let (position, velocity, trail, in_pass, sunlight) = if let Some(elements) = craft.elements
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

        // Per-craft station geometry (reachable stations only).
        let station_status: Vec<CraftStationStatus> = self
            .ground_stations
            .iter()
            .filter(|st| Self::station_reachable(st, &craft_body.id))
            .map(|station| {
                let local = station.body_id == craft_body.id;
                let deep_space = station.dsn && !local && craft_body.id != "earth";
                let elements = craft.elements;
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
                CraftStationStatus {
                    station_id: station.id.clone(),
                    elevation_deg: elevation,
                    in_pass,
                    next_pass_in_seconds,
                    bandwidth_mbps: Self::station_bandwidth_for(
                        station,
                        craft_body,
                        self.company.deep_space_comms_multiplier,
                    ),
                }
            })
            .collect();

        let apsis = craft.elements.is_some_and(|_| at_apsis(position, velocity));
        let (seconds_to_perigee, seconds_to_apogee) = craft
            .elements
            .filter(|_| craft.phase == CraftPhase::OnOrbit)
            .map_or((None, None), |e| {
                let (p, a) = seconds_to_apsides(craft_body.mu_km3_s2, &e, self.clock.sim_seconds);
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
            id: craft.id,
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
            ops_cost_usd_per_day: Self::craft_ops_raw_usd_per_day(craft)
                * self.company.ops_cost_multiplier,
            contracts_completed: craft.contracts_completed,
            lifetime_revenue_usd: craft.lifetime_revenue_usd,
            lifetime_ops_usd: craft.lifetime_ops_usd,
            launched_at_sim_seconds: craft.launched_at_sim_seconds,
            station_status,
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
    }

    /// Earliest upcoming fleet event: the "warp to next event" target.
    /// Considers transfer arrivals, planned-transfer windows, contract
    /// deadlines, and each craft's next station pass/LOS window.
    pub fn next_fleet_event_sim_seconds(&self) -> Option<f64> {
        let now = self.clock.sim_seconds;
        let mut next: Option<f64> = None;
        let mut consider = |t: f64| {
            if t > now + 1.0 {
                next = Some(next.map_or(t, |n: f64| n.min(t)));
            }
        };

        for contract in &self.company.active_contracts {
            consider(contract.deadline_sim_seconds);
        }
        for craft in &self.fleet {
            if let Some(t) = &craft.transfer {
                consider(t.arrival_sim_seconds);
                continue;
            }
            if craft.phase != CraftPhase::OnOrbit {
                continue;
            }
            // Planned transfer waiting on a window.
            if let Some(target) = &craft.transfer_target
                && let (Some(origin), Some(dest)) = (
                    self.bodies.idx(&craft.body_id),
                    self.bodies.idx(&target.body_id),
                )
                && let Ok(window) =
                    crate::orbit::next_launch_window(&self.bodies, origin, dest, now)
            {
                consider(window + 60.0);
            }
            // Next pass or DSN window for the craft's own downlink station.
            if let Some(elements) = craft.elements {
                let body = self.craft_body_of(craft);
                let station = craft
                    .downlink_station_id
                    .as_ref()
                    .and_then(|id| self.ground_stations.iter().find(|s| &s.id == id));
                if let Some(station) = station {
                    let eta = if station.body_id == body.id {
                        crate::ops::seconds_until_pass(
                            body,
                            &elements,
                            station.latitude_deg,
                            station.longitude_deg,
                            station.min_elevation_deg,
                            now,
                            7200.0,
                            15.0,
                        )
                    } else if station.dsn {
                        self.seconds_until_los(body, &elements, now, 7200.0, 15.0)
                    } else {
                        None
                    };
                    if let Some(eta) = eta {
                        consider(now + eta);
                    }
                }
            }
        }
        next
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
        // Coarse company-level phase for HUD routing only; prefer the
        // per-craft phases in the fleet snapshot.
        if self.fleet.is_empty() {
            return GamePhase::Design;
        }
        if self.fleet.iter().any(|c| c.phase == CraftPhase::Design) {
            return if self.company.active_contracts.is_empty() {
                GamePhase::Design
            } else {
                GamePhase::AwaitingLaunch
            };
        }
        if self.fleet.iter().any(|c| c.phase == CraftPhase::OnOrbit) {
            return if self.company.active_contracts.is_empty()
                && !self.company.completed_contracts.is_empty()
            {
                GamePhase::ContractComplete
            } else {
                GamePhase::OnOrbit
            };
        }
        GamePhase::InTransit
    }

    fn push_event(&mut self, event: SimEvent) {
        self.event_queue.push(event);
    }
}

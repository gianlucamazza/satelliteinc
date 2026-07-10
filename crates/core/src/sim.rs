use crate::action::PlayerAction;
use crate::clock::SimClock;
use crate::company::{can_purchase_tech, ActiveContract, GameMode};
use crate::content::GameContent;
use crate::error::SimError;
use crate::event::SimEvent;
use crate::maneuver_ops::{
    complete_hohmann, execute_prograde_burn, preview_hohmann, start_hohmann,
};
use crate::ops::{eci_to_ecef_km, in_sunlight, is_in_pass, GroundStation};
use crate::ops::{
    execute_command, roll_anomaly, ActiveAnomaly, AnomalyType, CommandType, QueuedCommand,
    ANOMALY_COOLDOWN_SECONDS,
};
use crate::orbit::{
    at_apsis, circular_leo, eci_position_km, eci_velocity_km_s, orbit_trail_samples,
    ReferenceSatellite, TlePropagator,
};
use crate::snapshot::{GamePhase, Snapshot, SpacecraftSnapshot, StationSnapshot};
use crate::spacecraft::{CraftPhase, HohmannPlanSnapshot, PayloadMode, Spacecraft};

const STEP_DT_SECONDS: f64 = 1.0;
const STARTING_CASH: f64 = 450_000.0;

pub struct SimState {
    pub clock: SimClock,
    pub company: crate::company::CompanyState,
    pub spacecraft: Option<Spacecraft>,
    pub ground_stations: Vec<GroundStation>,
    pub content: GameContent,
    pub event_queue: Vec<SimEvent>,
    pub tle_propagators: Vec<TlePropagator>,
    pub pass_active: std::collections::HashMap<String, bool>,
    /// Per-tick cache for SGP4 reference-satellite states (snapshot runs per frame).
    pub(crate) ref_sat_cache: std::cell::RefCell<Option<(u64, Vec<ReferenceSatellite>)>>,
}

impl SimState {
    pub fn new(content: GameContent) -> Self {
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
        Self {
            clock: SimClock::new(),
            company: crate::company::CompanyState::new(
                "Satellite Inc",
                STARTING_CASH,
                GameMode::Campaign,
                &content,
            ),
            spacecraft: None,
            ground_stations,
            content,
            event_queue: Vec::new(),
            tle_propagators,
            pass_active: std::collections::HashMap::new(),
            ref_sat_cache: std::cell::RefCell::new(None),
        }
    }

    pub fn apply_action(&mut self, action: PlayerAction) -> Result<(), SimError> {
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
                self.spacecraft = Some(Spacecraft::new_from_bus(&bus, &bus.name));
                self.push_event(SimEvent::ActionApplied {
                    message: format!("Selected bus: {}", bus.name),
                });
            }
            PlayerAction::SetOrbit {
                altitude_km,
                inclination_deg,
            } => {
                let craft = self.spacecraft_mut(CraftPhase::Design)?;
                craft.target_altitude_km = altitude_km.clamp(200.0, 2_000.0);
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
                let contract = ActiveContract::from_template(&template, self.clock.sim_seconds);
                self.company.active_contract = Some(contract);
                self.push_event(SimEvent::ActionApplied {
                    message: format!("Accepted contract: {}", template.name),
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
                let elements = circular_leo(alt, inc, self.clock.sim_seconds);
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
                self.clock.time_accel = multiplier.clamp(1.0, 10_000.0);
            }
            PlayerAction::ProgradeBurn { delta_v_km_s } => {
                let craft = self.spacecraft.as_mut().ok_or(SimError::WrongPhase)?;
                if craft.phase != CraftPhase::OnOrbit {
                    return Err(SimError::WrongPhase);
                }
                let result = execute_prograde_burn(
                    craft,
                    self.clock.sim_seconds,
                    delta_v_km_s.clamp(0.001, 2.0),
                    false,
                )?;
                for ev in result.events {
                    self.push_event(ev);
                }
            }
            PlayerAction::RetrogradeBurn { delta_v_km_s } => {
                let craft = self.spacecraft.as_mut().ok_or(SimError::WrongPhase)?;
                if craft.phase != CraftPhase::OnOrbit {
                    return Err(SimError::WrongPhase);
                }
                let result = execute_prograde_burn(
                    craft,
                    self.clock.sim_seconds,
                    delta_v_km_s.clamp(0.001, 2.0),
                    true,
                )?;
                for ev in result.events {
                    self.push_event(ev);
                }
            }
            PlayerAction::SetManeuverTarget { target_altitude_km } => {
                let craft = self.spacecraft_mut(CraftPhase::OnOrbit)?;
                craft.maneuver_target_altitude_km = target_altitude_km.clamp(200.0, 2_000.0);
            }
            PlayerAction::StartHohmann => {
                let craft = self.spacecraft.as_mut().ok_or(SimError::WrongPhase)?;
                if craft.phase != CraftPhase::OnOrbit {
                    return Err(SimError::WrongPhase);
                }
                let target = craft.maneuver_target_altitude_km;
                let result = start_hohmann(craft, self.clock.sim_seconds, target)?;
                for ev in result.events {
                    self.push_event(ev);
                }
            }
            PlayerAction::CompleteHohmann => {
                let craft = self.spacecraft.as_mut().ok_or(SimError::WrongPhase)?;
                if craft.phase != CraftPhase::OnOrbit {
                    return Err(SimError::WrongPhase);
                }
                let result = complete_hohmann(craft, self.clock.sim_seconds)?;
                for ev in result.events {
                    self.push_event(ev);
                }
            }
            PlayerAction::SetPayloadMode { mode } => {
                let craft = self.spacecraft_mut(CraftPhase::OnOrbit)?;
                craft.payload_mode = mode;
                // Manual mode change overrides any commanded safe-mode hold.
                craft.safe_mode_until = 0.0;
                craft.payload_mode_before_safe = None;
            }
            PlayerAction::SelectDownlinkStation { station_id } => {
                if !self.ground_stations.iter().any(|s| s.id == station_id) {
                    return Err(SimError::Content(format!("unknown station: {station_id}")));
                }
                let craft = self.spacecraft_mut(CraftPhase::OnOrbit)?;
                craft.downlink_station_id = Some(station_id);
            }
            PlayerAction::QueueCommand {
                command,
                station_id,
            } => {
                if !self.ground_stations.iter().any(|s| s.id == station_id) {
                    return Err(SimError::Content(format!("unknown station: {station_id}")));
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
        let sim_dt = real_dt_seconds * self.clock.time_accel;
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
        let on_orbit = self
            .spacecraft
            .as_ref()
            .map(|c| c.phase == CraftPhase::OnOrbit && c.elements.is_some())
            .unwrap_or(false);
        if !on_orbit {
            return;
        }

        let sim_seconds = self.clock.sim_seconds;
        let elements = self.spacecraft.as_ref().unwrap().elements.unwrap();
        let eci = eci_position_km(&elements, sim_seconds);
        let sunlight = in_sunlight(eci);
        let ecef = eci_to_ecef_km(eci, sim_seconds);

        let comms_mult = self.company.comms_multiplier;
        let solar_mult = self.company.solar_multiplier;

        let mut pass_events = Vec::new();
        let mut station_passes: std::collections::HashMap<String, bool> =
            std::collections::HashMap::new();

        let mut pass_starts: Vec<(String, String)> = Vec::new();
        for station in &self.ground_stations {
            let in_pass = is_in_pass(
                station.latitude_deg,
                station.longitude_deg,
                station.min_elevation_deg,
                ecef,
            );
            station_passes.insert(station.id.clone(), in_pass);
            let was_active = *self.pass_active.get(&station.id).unwrap_or(&false);
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
            self.pass_active.insert(station.id.clone(), in_pass);
        }
        for (id, name) in pass_starts {
            self.try_execute_queued_command(&id, &name);
        }

        let mut anomaly_events = Vec::new();
        {
            let craft = self.spacecraft.as_mut().unwrap();
            if let Some(a) = craft.active_anomaly.clone() {
                if a.is_expired(sim_seconds) {
                    anomaly_events.push(SimEvent::AnomalyCleared { kind: a.kind });
                    craft.active_anomaly = None;
                    craft.anomaly_cooldown_until = sim_seconds + ANOMALY_COOLDOWN_SECONDS;
                }
            }
            if craft.active_anomaly.is_none() {
                if let Some(kind) = roll_anomaly(sim_seconds, dt, craft.anomaly_cooldown_until) {
                    let anomaly = ActiveAnomaly::new(kind, sim_seconds);
                    let label = anomaly.label().to_string();
                    if kind == AnomalyType::BufferCorruption {
                        craft.buffer_mb *= 0.8;
                    }
                    craft.active_anomaly = Some(anomaly);
                    craft.anomaly_cooldown_until = sim_seconds + ANOMALY_COOLDOWN_SECONDS;
                    anomaly_events.push(SimEvent::AnomalyStarted { kind, label });
                }
            }
        }

        let craft = self.spacecraft.as_mut().unwrap();

        if craft.safe_mode_until > 0.0 && sim_seconds >= craft.safe_mode_until {
            craft.safe_mode_until = 0.0;
            if craft.payload_mode == PayloadMode::Safe {
                if let Some(mode) = craft.payload_mode_before_safe.take() {
                    craft.payload_mode = mode;
                }
            }
        }

        let downlink_pass = craft
            .downlink_station_id
            .as_ref()
            .and_then(|id| station_passes.get(id).copied())
            .unwrap_or_else(|| station_passes.values().any(|&p| p));

        let balance = craft.power_balance_w(sunlight, solar_mult, sim_seconds);
        craft.power_wh = (craft.power_wh + balance * dt / 3600.0).clamp(0.0, craft.bus.battery_wh);

        // Imaging needs power, like downlink below.
        if craft.power_wh > 1.0 {
            let imaging = craft.imaging_rate_mb_s(sunlight, sim_seconds) * dt;
            craft.buffer_mb = (craft.buffer_mb + imaging).min(craft.max_buffer_mb());
        }

        let mut transferred = 0.0;
        if craft.downlink_active && downlink_pass && craft.buffer_mb > 0.0 && craft.power_wh > 1.0 {
            let rate = craft.downlink_rate_mb_s(comms_mult, sim_seconds);
            transferred = (rate * dt).min(craft.buffer_mb);
            craft.buffer_mb -= transferred;
        }

        if transferred > 0.0 {
            if let Some(station_id) = craft.downlink_station_id.clone() {
                if let Some(contract) = self.company.active_contract.as_mut() {
                    contract.downlink_mb_done += transferred;
                    if !contract.stations_used.contains(&station_id) {
                        contract.stations_used.push(station_id);
                    }
                }
            }
        }

        for event in pass_events.into_iter().chain(anomaly_events) {
            self.push_event(event);
        }
        self.check_contract();
    }

    fn try_execute_queued_command(&mut self, station_id: &str, station_name: &str) {
        let craft = match self.spacecraft.as_mut() {
            Some(c) => c,
            None => return,
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
                if let Some(a) = &craft.active_anomaly {
                    if a.kind == AnomalyType::CommsDropout {
                        craft.clear_anomaly(self.clock.sim_seconds);
                        self.push_event(SimEvent::AnomalyCleared {
                            kind: AnomalyType::CommsDropout,
                        });
                    }
                }
            }
            CommandType::SafeMode => {
                if craft.payload_mode != PayloadMode::Safe {
                    craft.payload_mode_before_safe = Some(craft.payload_mode);
                }
                craft.safe_mode_until = self.clock.sim_seconds + 300.0;
                craft.payload_mode = PayloadMode::Safe;
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
            let payout = contract.payout_usd;
            let rep = contract.reputation_reward;
            let id = contract.template_id.clone();
            self.company.cash_usd += payout;
            self.company.completed_contracts.push(id.clone());
            self.company.reputation = (self.company.reputation + rep).min(100.0);
            self.push_event(SimEvent::ContractCompleted {
                contract_id: id,
                payout_usd: payout,
            });
            self.push_event(SimEvent::ReputationChanged {
                delta: rep,
                new_value: self.company.reputation,
            });
            self.company.active_contract = None;
            return;
        }

        if self.clock.sim_seconds > contract.deadline_sim_seconds {
            let id = contract.template_id.clone();
            let penalty = self.company.failure_rep_penalty();
            self.company.reputation = (self.company.reputation - penalty).max(0.0);
            self.push_event(SimEvent::ContractFailed { contract_id: id });
            self.push_event(SimEvent::ReputationChanged {
                delta: -penalty,
                new_value: self.company.reputation,
            });
            self.company.active_contract = None;
        }
    }

    pub fn drain_events(&mut self) -> Vec<SimEvent> {
        std::mem::take(&mut self.event_queue)
    }

    fn reference_satellites(&self) -> Vec<ReferenceSatellite> {
        let mut cache = self.ref_sat_cache.borrow_mut();
        if let Some((tick, sats)) = cache.as_ref() {
            if *tick == self.clock.tick {
                return sats.clone();
            }
        }
        let sats: Vec<ReferenceSatellite> = self
            .tle_propagators
            .iter()
            .filter_map(|p| p.propagate(self.clock.sim_seconds).ok())
            .collect();
        *cache = Some((self.clock.tick, sats.clone()));
        sats
    }

    pub fn snapshot(&self) -> Snapshot {
        let game_phase = self.derive_game_phase();
        let marketplace = self.company.marketplace(&self.content);

        let spacecraft = self.spacecraft.as_ref().map(|craft| {
            let (position, velocity, trail, in_pass, sunlight) =
                if let Some(elements) = craft.elements {
                    let eci = eci_position_km(&elements, self.clock.sim_seconds);
                    let vel = eci_velocity_km_s(&elements, self.clock.sim_seconds);
                    let trail = orbit_trail_samples(&elements, self.clock.sim_seconds, 128);
                    let ecef = eci_to_ecef_km(eci, self.clock.sim_seconds);
                    let in_pass = self.ground_stations.iter().any(|s| {
                        is_in_pass(s.latitude_deg, s.longitude_deg, s.min_elevation_deg, ecef)
                    });
                    let sunlight = in_sunlight(eci);
                    (eci, vel, trail, in_pass, sunlight)
                } else {
                    ([0.0; 3], [0.0; 3], vec![], false, true)
                };

            let apsis = craft.elements.is_some_and(|_| at_apsis(position, velocity));
            let hohmann_preview =
                if craft.phase == CraftPhase::OnOrbit && craft.pending_hohmann.is_none() {
                    preview_hohmann(craft, craft.maneuver_target_altitude_km)
                        .ok()
                        .map(|p| HohmannPlanSnapshot::from(&p))
                } else {
                    None
                };

            let (anomaly_type, anomaly_label) = craft
                .active_anomaly
                .as_ref()
                .map(|a| (Some(a.kind), Some(a.label().to_string())))
                .unwrap_or((None, None));

            SpacecraftSnapshot {
                name: craft.name.clone(),
                bus_id: craft.bus_id.clone(),
                bus_name: craft.bus.name.clone(),
                phase: craft.phase.clone(),
                position_eci_km: position,
                velocity_eci_km_s: velocity,
                elements: craft.elements,
                orbit_trail_km: trail,
                power_wh: craft.power_wh,
                buffer_mb: craft.buffer_mb,
                downlink_active: craft.downlink_active,
                in_sunlight: sunlight,
                in_pass,
                downlink_rate_mb_s: craft
                    .downlink_rate_mb_s(self.company.comms_multiplier, self.clock.sim_seconds),
                imaging_rate_mb_s: craft.imaging_rate_mb_s(sunlight, self.clock.sim_seconds),
                target_altitude_km: craft.target_altitude_km,
                target_inclination_deg: craft.target_inclination_deg,
                altitude_km: craft.current_altitude_km().unwrap_or(0.0),
                propellant_kg: craft.propellant_kg,
                delta_v_remaining_km_s: craft.delta_v_remaining_km_s(),
                total_delta_v_used_km_s: craft.total_delta_v_used_km_s,
                pending_hohmann: craft.pending_hohmann.clone(),
                at_apsis: apsis,
                hohmann_preview,
                maneuver_target_altitude_km: craft.maneuver_target_altitude_km,
                payload_mode: craft.payload_mode,
                downlink_station_id: craft.downlink_station_id.clone(),
                command_queue: craft.command_queue.clone(),
                active_anomaly: anomaly_type,
                anomaly_label,
            }
        });

        let ground_stations: Vec<StationSnapshot> = self
            .ground_stations
            .iter()
            .map(|station| {
                let def = self
                    .content
                    .ground_stations
                    .iter()
                    .find(|d| d.id == station.id);
                let bandwidth = def.map(|d| d.bandwidth_mbps).unwrap_or(50.0);
                let elevation = if let Some(craft) = self.spacecraft.as_ref() {
                    if let Some(elements) = craft.elements {
                        let eci = eci_position_km(&elements, self.clock.sim_seconds);
                        let ecef = eci_to_ecef_km(eci, self.clock.sim_seconds);
                        crate::ops::elevation_deg(station.latitude_deg, station.longitude_deg, ecef)
                    } else {
                        -90.0
                    }
                } else {
                    -90.0
                };
                let in_pass = elevation >= station.min_elevation_deg;
                let next_pass_in_seconds = if !in_pass {
                    self.spacecraft.as_ref().and_then(|craft| {
                        craft.elements.and_then(|elements| {
                            crate::ops::seconds_until_pass(
                                &elements,
                                station.latitude_deg,
                                station.longitude_deg,
                                station.min_elevation_deg,
                                self.clock.sim_seconds,
                                7200.0,
                                15.0,
                            )
                        })
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
                    bandwidth_mbps: bandwidth,
                    next_pass_in_seconds,
                }
            })
            .collect();

        Snapshot {
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
            spacecraft,
            ground_stations,
            reference_satellites: self.reference_satellites(),
            phase: game_phase,
        }
    }

    fn derive_game_phase(&self) -> GamePhase {
        match self.spacecraft.as_ref().map(|s| &s.phase) {
            None => GamePhase::Design,
            Some(CraftPhase::Design) => {
                if self.company.active_contract.is_some() {
                    GamePhase::AwaitingLaunch
                } else {
                    GamePhase::Design
                }
            }
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

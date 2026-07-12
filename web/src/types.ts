export type GamePhase =
  | "design"
  | "awaiting_launch"
  | "on_orbit"
  | "in_transit"
  | "contract_complete"
  | "victory"
  | "game_over";

export type GameOutcome = "victory" | "bankrupt" | "reputation_lost";

export type CraftPhase = "Design" | "OnOrbit" | "Transfer";
export type GameMode = "sandbox" | "campaign";
export type PayloadMode = "imaging" | "safe" | "downlink_priority";

export interface KeplerElements {
  semi_major_axis_km: number;
  eccentricity: number;
  inclination_deg: number;
  raan_deg: number;
  arg_perigee_deg: number;
  true_anomaly_deg: number;
  epoch_sim_seconds: number;
}

export interface ActiveContract {
  template_id: string;
  name: string;
  downlink_mb_required: number;
  downlink_mb_done: number;
  deadline_sim_seconds: number;
  payout_usd: number;
  reputation_reward: number;
  min_distinct_stations: number;
  stations_used: string[];
  /** Body whose orbit fulfills this contract. */
  body_id: string;
  /** Craft serving this contract (1:1); null = unassigned, no accrual. */
  assigned_craft_id: number | null;
  advance_paid_usd: number;
  min_on_station_days: number;
  on_station_seconds: number;
  min_altitude_km: number | null;
  max_altitude_km: number | null;
  min_inclination_deg: number | null;
}

export interface ContractOffer {
  template_id: string;
  name: string;
  description: string;
  downlink_mb: number;
  deadline_days: number;
  payout_usd: number;
  reputation_required: number;
  min_distinct_stations: number;
  min_altitude_km: number | null;
  max_altitude_km: number | null;
  min_inclination_deg: number | null;
  body_id: string;
  available: boolean;
  repeat: boolean;
}

export interface HohmannPlanSnapshot {
  burn1_dv_km_s: number;
  burn2_dv_km_s: number;
  total_dv_km_s: number;
  target_altitude_km: number;
  raising: boolean;
}

export interface PendingHohmann {
  plan: HohmannPlanSnapshot;
}

export interface QueuedCommand {
  command: string;
  station_id: string;
  queued_at_sim_seconds: number;
}

export interface ReferenceSatellite {
  norad_id: number;
  name: string;
  position_eci_km: [number, number, number];
  velocity_eci_km_s: [number, number, number];
  altitude_km: number;
}

/** A celestial body's state at snapshot time (positions in km). */
export interface BodySnapshot {
  id: string;
  name: string;
  parent_id: string | null;
  radius_km: number;
  rotation_rad_s: number;
  position_helio_km: [number, number, number];
  position_parent_km: [number, number, number];
  /** Unit vector from the body toward the sun (body-inertial frame). */
  sun_direction: [number, number, number];
  /** null for the sun (serialized infinity). */
  soi_radius_km: number | null;
  texture: string | null;
}

/** Active contract flattened with its assigned craft's orbit compliance. */
export type ActiveContractSnapshot = ActiveContract & {
  orbit_ok: boolean | null;
};

/** Craft-relative station geometry (reachable stations only). */
export interface CraftStationStatus {
  station_id: string;
  elevation_deg: number;
  in_pass: boolean;
  next_pass_in_seconds: number | null;
  /** Effective link bandwidth for this craft (DSN-scaled). */
  bandwidth_mbps: number;
}

export interface SpacecraftSnapshot {
  /** Stable fleet id. */
  id: number;
  name: string;
  bus_id: string;
  bus_name: string;
  phase: CraftPhase;
  /** Central body of position_eci_km / elements (body-relative frame). */
  body_id: string;
  position_eci_km: [number, number, number];
  velocity_eci_km_s: [number, number, number];
  elements: KeplerElements | null;
  orbit_trail_km: [number, number, number][];
  power_wh: number;
  battery_wh_capacity: number;
  buffer_mb: number;
  buffer_capacity_mb: number;
  downlink_active: boolean;
  in_sunlight: boolean;
  in_pass: boolean;
  downlink_rate_mb_s: number;
  imaging_rate_mb_s: number;
  target_altitude_km: number;
  target_inclination_deg: number;
  altitude_km: number;
  propellant_kg: number;
  propellant_capacity_kg: number;
  delta_v_remaining_km_s: number;
  total_delta_v_used_km_s: number;
  pending_hohmann: PendingHohmann | null;
  at_apsis: boolean;
  seconds_to_perigee: number | null;
  seconds_to_apogee: number | null;
  hohmann_preview: HohmannPlanSnapshot | null;
  maneuver_target_altitude_km: number;
  payload_mode: PayloadMode;
  downlink_station_id: string | null;
  command_queue: QueuedCommand[];
  active_anomaly: string | null;
  anomaly_label: string | null;
  /** One-way light time to Earth (flavor; ~0 in Earth orbit). */
  light_delay_seconds: number;
  /** This craft's daily ops line (company multiplier applied). */
  ops_cost_usd_per_day: number;
  /** Service record. */
  contracts_completed: number;
  lifetime_revenue_usd: number;
  lifetime_ops_usd: number;
  launched_at_sim_seconds: number | null;
  station_status: CraftStationStatus[];
  /** Cruise in progress (null on orbit / in design). */
  transfer: TransferSnapshot | null;
  /** Planner readout for the selected destination (OnOrbit only). */
  transfer_planner: TransferPlannerSnapshot | null;
}

export interface TransferSnapshot {
  origin_body_id: string;
  target_body_id: string;
  frame_body_id: string;
  departure_sim_seconds: number;
  arrival_sim_seconds: number;
  /** 0..1 fraction of the cruise elapsed. */
  progress: number;
  capture_dv_km_s: number;
}

export interface TransferPlannerSnapshot {
  target_body_id: string;
  window_open: boolean;
  phase_angle_deg: number | null;
  required_phase_angle_deg: number | null;
  next_window_sim_seconds: number;
  transfer_seconds: number;
  departure_dv_km_s: number;
  capture_dv_km_s: number;
  capture_propellant_kg: number;
  injection_cost_usd: number;
  affordable: boolean;
  propellant_ok: boolean;
}

export interface StationSnapshot {
  id: string;
  name: string;
  latitude_deg: number;
  longitude_deg: number;
  /** Base dish bandwidth (craft-effective rates live in station_status). */
  bandwidth_mbps: number;
  body_id: string;
  dsn: boolean;
}

export interface Snapshot {
  bodies: BodySnapshot[];
  /** UTC instant that sim_seconds = 0 represents. */
  epoch_utc: string;
  sim_seconds: number;
  sim_days: number;
  time_accel: number;
  tick: number;
  company_name: string;
  cash_usd: number;
  reputation: number;
  game_mode: GameMode;
  unlocked_tech: string[];
  unlocked_buses: string[];
  marketplace: ContractOffer[];
  active_contracts: ActiveContractSnapshot[];
  spacecraft: SpacecraftSnapshot[];
  ground_stations: StationSnapshot[];
  reference_satellites: ReferenceSatellite[];
  phase: GamePhase;
  outcome: GameOutcome | null;
  ops_cost_usd_per_day: number;
  transfers_unlocked: boolean;
  /** Earliest upcoming fleet event (warp-to-next-event target). */
  next_event_sim_seconds: number | null;
  programs: ProgramSnapshot[];
}

export interface ProgramSnapshot {
  id: string;
  name: string;
  body_id: string;
  buyin_usd: number;
  reputation_required: number;
  requires_keystone: string | null;
  keystone_done: boolean;
  requires_tech: string | null;
  tech_done: boolean;
  unlocked: boolean;
  completed: boolean;
  completion_grant_usd: number;
}

export interface BusTemplate {
  id: string;
  name: string;
  mass_kg: number;
  solar_power_w: number;
  battery_wh: number;
  comms_mbps: number;
  comms_power_w: number;
  payload_power_w: number;
  launch_cost_usd: number;
  propellant_kg: number;
  isp_s: number;
  /** Constant RTG watts, indifferent to sunlight (0 = solar only). */
  rtg_power_w?: number;
  ops_cost_usd_per_day: number;
  unlocked_by_default?: boolean;
}

export interface TechNode {
  id: string;
  name: string;
  description: string;
  cost_usd: number;
  requires: string[];
  unlocks_bus?: string | null;
  comms_multiplier: number;
  solar_multiplier: number;
  isp_multiplier?: number;
  anomaly_mtbf_multiplier?: number;
}

export interface ContractTemplate {
  id: string;
  name: string;
  description: string;
  downlink_mb: number;
  deadline_days: number;
  payout_usd: number;
  reputation_required?: number;
  reputation_reward?: number;
  body_id?: string;
}

export interface GroundStationDef {
  id: string;
  name: string;
  latitude_deg: number;
  longitude_deg: number;
  min_elevation_deg: number;
  bandwidth_mbps?: number;
  body_id?: string;
  dsn?: boolean;
}

export interface TleEntry {
  norad_id: number;
  name: string;
  line1: string;
  line2: string;
}

export interface ScenarioTemplate {
  id: string;
  name: string;
  description: string;
  game_mode: GameMode;
  starting_cash: number;
  starting_reputation: number;
  bus_id: string;
  body_id?: string;
  contract_id?: string | null;
  altitude_km: number;
  inclination_deg: number;
}

export interface GameContent {
  buses: BusTemplate[];
  ground_stations: GroundStationDef[];
  contracts: ContractTemplate[];
  tech_tree: TechNode[];
  tle_catalog: TleEntry[];
  scenarios?: ScenarioTemplate[];
}

/** Mirrors the Rust `SimEvent` enum (`#[serde(tag = "type", rename_all = "snake_case")]`). */
export type SimEvent =
  | {
      type: "pass_started";
      craft_id: number;
      station_id: string;
      station_name: string;
    }
  | { type: "pass_ended"; craft_id: number; station_id: string }
  | { type: "contract_completed"; contract_id: string; payout_usd: number }
  | { type: "contract_failed"; contract_id: string }
  | {
      type: "launched";
      craft_id: number;
      craft_name: string;
      altitude_km: number;
      inclination_deg: number;
    }
  | { type: "insufficient_funds"; required_usd: number }
  | { type: "action_applied"; message: string }
  | {
      type: "maneuver_executed";
      craft_id: number;
      delta_v_km_s: number;
      propellant_used_kg: number;
      new_altitude_km: number | null;
      label: string;
    }
  | {
      type: "hohmann_burn1_complete";
      craft_id: number;
      delta_v_km_s: number;
      target_altitude_km: number;
    }
  | {
      type: "hohmann_transfer_complete";
      craft_id: number;
      target_altitude_km: number;
    }
  | {
      type: "transfer_started";
      craft_id: number;
      target_body_id: string;
      arrival_sim_seconds: number;
      injection_cost_usd: number;
    }
  | { type: "transfer_arrived"; craft_id: number; body_id: string }
  | { type: "capture_executed"; craft_id: number; delta_v_km_s: number }
  | { type: "warp_completed"; seconds: number }
  | { type: "program_unlocked"; program_id: string }
  | { type: "program_completed"; program_id: string; grant_usd: number }
  | {
      type: "contract_assignment_changed";
      contract_id: string;
      craft_id: number | null;
    }
  | { type: "anomaly_started"; craft_id: number; kind: string; label: string }
  | { type: "anomaly_cleared"; craft_id: number; kind: string }
  | {
      type: "command_executed";
      craft_id: number;
      command: string;
      station_name: string;
      message: string;
    }
  | { type: "tech_purchased"; tech_id: string; name: string }
  | { type: "reputation_changed"; delta: number; new_value: number }
  | {
      type: "decommissioned";
      craft_id: number;
      craft_name: string;
      salvage_usd: number;
    }
  | { type: "game_won" }
  | { type: "game_over"; reason: GameOutcome }
  | { type: "cash_negative"; cash_usd: number };

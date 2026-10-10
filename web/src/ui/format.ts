import type {
  ActiveContract,
  ActiveContractSnapshot,
  Snapshot,
  SpacecraftSnapshot,
} from "../types.ts";

const SECONDS_PER_DAY = 86_400;

export function formatSimDays(seconds: number): string {
  return (seconds / SECONDS_PER_DAY).toFixed(2);
}

export function contractDaysLeft(
  contract: ActiveContract,
  simSeconds: number,
): number {
  return Math.max(
    0,
    (contract.deadline_sim_seconds - simSeconds) / SECONDS_PER_DAY,
  );
}

export function contractProgress(contract: ActiveContract): number {
  if (contract.downlink_mb_required <= 0) return 100;
  return Math.min(
    100,
    (contract.downlink_mb_done / contract.downlink_mb_required) * 100,
  );
}

/**
 * Whether the craft's *selected* downlink station is currently in link
 * (elevation/LOS). Falls back to craft-wide `in_pass` if no station chosen.
 * Matches sim tick: transfer only accrues when the selected dish sees the craft.
 */
export function isDownlinkLinkLive(craft: SpacecraftSnapshot): boolean {
  const id = craft.downlink_station_id;
  if (id) {
    const st = craft.station_status.find((x) => x.station_id === id);
    if (st) return st.in_pass;
  }
  return craft.in_pass;
}

/** Soonest upcoming pass for a craft (by `next_pass_in_seconds`, not elevation). */
export function nextPassStation(
  s: Snapshot,
  craft?: SpacecraftSnapshot | null,
): { name: string; elevation: number; etaSeconds: number | null } | null {
  const c = craft ?? s.spacecraft[0] ?? null;
  if (!c) return null;

  const withEta = c.station_status
    .filter(
      (st) =>
        !st.in_pass &&
        st.next_pass_in_seconds != null &&
        Number.isFinite(st.next_pass_in_seconds),
    )
    .sort(
      (a, b) => (a.next_pass_in_seconds ?? 1e12) - (b.next_pass_in_seconds ?? 1e12),
    );
  const best = withEta[0];
  if (!best) return null;
  const name =
    s.ground_stations.find((g) => g.id === best.station_id)?.name ??
    best.station_id;
  return {
    name,
    elevation: best.elevation_deg,
    etaSeconds: best.next_pass_in_seconds,
  };
}

export function payloadModeLabel(mode: string): string {
  switch (mode) {
    case "imaging":
      return "Imaging";
    case "safe":
      return "Safe";
    case "downlink_priority":
      return "Downlink priority";
    default:
      return mode.replace(/_/g, " ");
  }
}

export function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds.toFixed(0)}s`;
  if (seconds < 3600) return `${(seconds / 60).toFixed(1)} min`;
  return `${(seconds / 3600).toFixed(1)} h`;
}

/** Instantaneous downlink readout: Off / Armed (no link) / live KB/s. */
export function downlinkRateLabel(craft: SpacecraftSnapshot): string {
  if (!craft.downlink_active) return "Off";
  const kbps = craft.downlink_rate_mb_s * 1000;
  if (!isDownlinkLinkLive(craft)) {
    return kbps > 0 ? "Armed" : "On";
  }
  if (craft.power_wh <= 1) return "No power";
  if (craft.buffer_mb < 0.01) return "0 KB/s";
  if (craft.active_anomaly === "comms_dropout") return "Comms out";
  return `${kbps.toFixed(0)} KB/s`;
}

export interface OrbitRequirements {
  min_altitude_km: number | null;
  max_altitude_km: number | null;
  min_inclination_deg: number | null;
}

/** Human-readable orbit requirement summary, or "" when unconstrained. */
export function orbitRequirementText(req: OrbitRequirements): string {
  const parts: string[] = [];
  if (req.min_altitude_km != null)
    parts.push(`alt ≥ ${req.min_altitude_km} km`);
  if (req.max_altitude_km != null)
    parts.push(`alt ≤ ${req.max_altitude_km} km`);
  if (req.min_inclination_deg != null)
    parts.push(`inc ≥ ${req.min_inclination_deg}°`);
  return parts.join(" · ");
}

export interface DataFlowStatus {
  text: string;
  kind: "ok" | "warn" | "bad";
}

/**
 * One-line diagnosis of why contract data is (or isn't) flowing for a craft.
 * Prefer the selected craft + its assigned contract (not fleet index 0).
 */
export function dataFlowStatus(
  s: Snapshot,
  craft?: SpacecraftSnapshot | null,
  contract?: ActiveContractSnapshot | null,
): DataFlowStatus | null {
  const c = craft ?? s.spacecraft[0] ?? null;
  if (c?.phase !== "OnOrbit") return null;

  const assigned =
    contract ??
    s.active_contracts.find((ac) => ac.assigned_craft_id === c.id) ??
    null;

  if (assigned?.orbit_ok === false) {
    return { text: "Orbit out of spec — data not accepted", kind: "bad" };
  }
  if (assigned?.body_id && assigned.body_id !== c.body_id) {
    return {
      text: `Wrong body for contract (need ${assigned.body_id})`,
      kind: "bad",
    };
  }

  if (!c.downlink_active) {
    if (c.buffer_mb < 1 && c.imaging_rate_mb_s === 0) {
      return {
        text: c.in_sunlight
          ? "Not imaging (safe mode or anomaly)"
          : "Not imaging (eclipse) — waiting for sunlight",
        kind: "warn",
      };
    }
    return { text: "Downlink off — buffer filling", kind: "warn" };
  }

  if (!isDownlinkLinkLive(c)) {
    // ETA for the selected dish first, else any soonest window.
    const selected = c.downlink_station_id
      ? c.station_status.find((st) => st.station_id === c.downlink_station_id)
      : null;
    const next =
      selected?.next_pass_in_seconds ??
      c.station_status
        .map((st) => st.next_pass_in_seconds)
        .filter((v): v is number => v != null)
        .sort((a, b) => a - b)[0];
    const dish =
      selected &&
      s.ground_stations.find((g) => g.id === selected.station_id)?.name;
    return {
      text: `Waiting for ${dish ? `${dish} ` : ""}pass${next != null ? ` (next in ${formatDuration(next)})` : ""}`,
      kind: "warn",
    };
  }

  if (c.power_wh <= 1) {
    return { text: "No power for downlink", kind: "bad" };
  }
  if (c.active_anomaly === "comms_dropout") {
    return { text: "Comms dropout — queue reboot at station", kind: "bad" };
  }
  if (c.buffer_mb < 1) {
    return { text: "Buffer empty — nothing to downlink", kind: "warn" };
  }

  const kbps = (c.downlink_rate_mb_s * 1000).toFixed(0);
  return { text: `Downlinking ✓ · ${kbps} KB/s`, kind: "ok" };
}

/** Sim-days until bankruptcy (−50k floor); null when not burning cash. */
export function bankruptcyRunwayDays(s: Snapshot): number | null {
  if (s.ops_cost_usd_per_day <= 0) return null;
  return (s.cash_usd + 50_000) / s.ops_cost_usd_per_day;
}

export function commandLabel(cmd: string): string {
  switch (cmd) {
    case "clear_anomaly":
      return "EPS clear";
    case "safe_mode":
      return "Safe mode";
    case "reboot_comms":
      return "Reboot comms";
    default:
      return cmd.replace(/_/g, " ");
  }
}

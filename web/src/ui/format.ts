import type { ActiveContract, Snapshot } from "../types.ts";

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

export function nextPassStation(
  s: Snapshot,
): { name: string; elevation: number } | null {
  const craft = s.spacecraft[0] ?? null;
  if (!craft) return null;
  const best = craft.station_status
    .filter((st) => !st.in_pass && st.elevation_deg >= 0)
    .sort((a, b) => b.elevation_deg - a.elevation_deg)[0];
  if (!best) return null;
  const name =
    s.ground_stations.find((g) => g.id === best.station_id)?.name ??
    best.station_id;
  return { name, elevation: best.elevation_deg };
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

/** One-line diagnosis of why contract data is (or isn't) flowing. */
export function dataFlowStatus(s: Snapshot): DataFlowStatus | null {
  const craft = s.spacecraft[0] ?? null;
  if (craft?.phase !== "OnOrbit") return null;
  if (s.active_contracts[0]?.orbit_ok === false) {
    return { text: "Orbit out of spec — data not accepted", kind: "bad" };
  }
  if (!craft.downlink_active) {
    if (craft.buffer_mb < 1 && craft.imaging_rate_mb_s === 0) {
      return {
        text: craft.in_sunlight
          ? "Not imaging (safe mode or anomaly)"
          : "Not imaging (eclipse) — waiting for sunlight",
        kind: "warn",
      };
    }
    return { text: "Downlink off — buffer filling", kind: "warn" };
  }
  if (!craft.in_pass) {
    const next = craft.station_status
      .map((st) => st.next_pass_in_seconds)
      .filter((v): v is number => v != null)
      .sort((a, b) => a - b)[0];
    return {
      text: `Waiting for pass${next != null ? ` (next in ${formatDuration(next)})` : ""}`,
      kind: "warn",
    };
  }
  if (craft.buffer_mb < 1) {
    return { text: "Buffer empty — nothing to downlink", kind: "warn" };
  }
  return { text: "Downlinking ✓", kind: "ok" };
}

/** Sim-days until bankruptcy (−50k floor); null when not burning cash. */
export function bankruptcyRunwayDays(s: Snapshot): number | null {
  if (s.ops_cost_usd_per_day <= 0) return null;
  return (s.cash_usd + 50_000) / s.ops_cost_usd_per_day;
}

export function commandLabel(cmd: string): string {
  switch (cmd) {
    case "clear_anomaly":
      return "Clear anomaly";
    case "safe_mode":
      return "Safe mode";
    case "reboot_comms":
      return "Reboot comms";
    default:
      return cmd.replace(/_/g, " ");
  }
}

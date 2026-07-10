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
  const best = s.ground_stations
    .filter((st) => !st.in_pass && st.elevation_deg >= 0)
    .sort((a, b) => b.elevation_deg - a.elevation_deg)[0];
  if (!best) return null;
  return { name: best.name, elevation: best.elevation_deg };
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

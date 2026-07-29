import type { Snapshot, SpacecraftSnapshot } from "../../types.ts";
import { esc } from "../html.ts";

/** Ops tab: downlink station, payload mode, command queue. */
export function renderOpsTab(
  s: Snapshot,
  craft: SpacecraftSnapshot | null,
): string[] {
  const parts: string[] = [];
  if (craft?.phase !== "OnOrbit") {
    parts.push(`<p class="hint">Launch first to access ground ops.</p>`);
    return parts;
  }

  const reachable = s.ground_stations.filter(
    (st) =>
      st.unlocked &&
      (st.body_id === craft.body_id || (st.dsn && craft.body_id !== "earth")),
  );
  const lockedLocal = s.ground_stations.filter(
    (st) =>
      !st.unlocked && st.body_id === craft.body_id && st.requires_tech,
  );
  const hasLocal = reachable.some((st) => st.body_id === craft.body_id);
  const dsnOnly = craft.body_id !== "earth" && !hasLocal;
  const linkHint = dsnOnly
    ? `<p class="hint warn-hint">DSN only at ${esc(craft.body_id)} — light delay ${craft.light_delay_seconds < 120 ? `${craft.light_delay_seconds.toFixed(0)} s` : `${(craft.light_delay_seconds / 60).toFixed(1)} min`}; farside blackouts apply. Research a local relay for full bandwidth.</p>`
    : craft.body_id !== "earth"
      ? `<p class="hint">Local dishes on ${esc(craft.body_id)} ignore light-time and Earth blackout; DSN still available for deep-space fallback.</p>`
      : `<p class="hint" title="Click a station marker on the globe to select">Tip: click station on globe</p>`;
  const lockedHint =
    lockedLocal.length > 0
      ? `<p class="hint">Locked: ${lockedLocal
          .map(
            (st) =>
              `${esc(st.name)} (research ${esc(st.requires_tech ?? "tech")})`,
          )
          .join(" · ")}</p>`
      : "";

  const stationOptions = reachable
    .map((st) => {
      const stStatus = craft.station_status.find((x) => x.station_id === st.id);
      const eff = stStatus?.bandwidth_mbps ?? st.bandwidth_mbps;
      const tag =
        st.body_id === craft.body_id
          ? st.body_id
          : st.dsn
            ? "DSN"
            : st.body_id;
      const link =
        stStatus?.in_pass
          ? st.dsn && st.body_id !== craft.body_id
            ? " · LINK"
            : " · PASS"
          : stStatus?.next_pass_in_seconds != null
            ? ` · next ${(stStatus.next_pass_in_seconds / 60).toFixed(0)}m`
            : "";
      return `<option value="${esc(st.id)}" ${craft.downlink_station_id === st.id ? "selected" : ""}>${esc(st.name)} · ${tag} · ${eff.toFixed(1)} Mbps${link}</option>`;
    })
    .join("");

  const selectedStatus = craft.downlink_station_id
    ? craft.station_status.find((x) => x.station_id === craft.downlink_station_id)
    : null;
  const liveHint =
    craft.downlink_active && selectedStatus
      ? selectedStatus.in_pass
        ? `<p class="hint pass-hint">Link live on selected dish · ${selectedStatus.bandwidth_mbps.toFixed(1)} Mbps cap</p>`
        : `<p class="hint warn-hint">Downlink armed — selected dish not in view yet${selectedStatus.next_pass_in_seconds != null ? ` (next ~${(selectedStatus.next_pass_in_seconds / 60).toFixed(0)} min)` : ""}</p>`
      : "";

  const cmdLightNote =
    craft.light_delay_seconds > 1.0
      ? ` Commands cannot execute until light arrives (~${craft.light_delay_seconds < 120 ? `${craft.light_delay_seconds.toFixed(0)} s` : `${(craft.light_delay_seconds / 60).toFixed(1)} min`} OWLT).`
      : "";

  parts.push(`
    <div class="field"><label>Downlink station</label>
      <select id="downlink-station">
        ${stationOptions || `<option value="">No reachable stations</option>`}
      </select>
      ${linkHint}
      ${lockedHint}
      ${liveHint}
    </div>
    <div class="field"><label>Payload mode</label>
      <select id="payload-mode" title="Imaging fills buffer in sun; Downlink priority boosts transfer rate during passes; Safe reduces risk">
        <option value="imaging" ${craft.payload_mode === "imaging" ? "selected" : ""}>Imaging — collect data in sunlight</option>
        <option value="safe" ${craft.payload_mode === "safe" ? "selected" : ""}>Safe — low power, reduced ops</option>
        <option value="downlink_priority" ${craft.payload_mode === "downlink_priority" ? "selected" : ""}>Downlink priority — faster passes</option>
      </select>
    </div>
    <button id="btn-downlink" class="${craft.downlink_active ? "danger" : "primary"}">
      ${craft.downlink_active ? "Stop Downlink" : "Start Downlink"}
    </button>
    <div class="field"><label>FDIR commands (execute at pass + OWLT)</label>
      <p class="hint">Procedures must match the fault: <strong>EPS clear</strong> → power glitch · <strong>Reboot</strong> → comms dropout · <strong>Safe</strong> → load-shed (halves remaining). SEU buffer hits are one-shot data loss.${cmdLightNote}</p>
    </div>
    <button id="btn-queue-clear" title="EPS recovery — clears Power / EPS glitch only">EPS clear @ station</button>
    <button id="btn-queue-reboot" title="Reboot RF/modem — clears Comms dropout only">Reboot comms @ station</button>
    <button id="btn-queue-safe" title="Safe mode 5 min — sheds load, halves remaining fault time">Safe mode @ station</button>
  `);
  return parts;
}

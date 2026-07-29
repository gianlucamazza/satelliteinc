import type { Snapshot, SpacecraftSnapshot } from "../../types.ts";
import { formatDuration } from "../format.ts";
import { esc } from "../html.ts";

/** Transfer tab: cruise progress or injection planner. */
export function renderTransferTab(
  s: Snapshot,
  craft: SpacecraftSnapshot | null,
): string[] {
  const parts: string[] = [];

  if (craft?.phase === "Transfer" && craft.transfer) {
    const t = craft.transfer;
    const eta = Math.max(0, t.arrival_sim_seconds - s.sim_seconds);
    const targetName =
      s.bodies.find((b) => b.id === t.target_body_id)?.name ?? t.target_body_id;
    parts.push(`
      <div class="field"><label>Cruise</label>
        <div class="stat"><label>Destination</label><strong>${esc(targetName)}</strong></div>
        <div class="stat gauge"><label>Progress</label>
          <div class="progress-bar thin"><div class="progress-fill" style="width:${(t.progress * 100).toFixed(1)}%"></div></div>
          <strong>${(t.progress * 100).toFixed(1)}%</strong>
        </div>
        <div class="stat"><label>Arrival in</label><strong>${formatDuration(eta)}</strong></div>
        <div class="stat"><label>Capture burn</label><strong>${t.capture_dv_km_s.toFixed(2)} km/s</strong><span class="stat-sub">propellant reserved</span></div>
      </div>
      <button id="btn-warp-arrival" class="primary" data-warp="${t.arrival_sim_seconds + 1}">Warp to arrival</button>
      <p class="hint">The craft is hibernated: ops cost is cut to 10% until capture.</p>
    `);
    return parts;
  }

  if (craft?.phase !== "OnOrbit") {
    parts.push(
      `<p class="hint">Launch to a parking orbit first, then plan the injection here.</p>`,
    );
    return parts;
  }

  const destinations = s.bodies.filter(
    (b) => b.id !== "sun" && b.id !== craft.body_id,
  );
  const selected = craft.transfer_planner?.target_body_id ?? "";
  parts.push(`
    <div class="field"><label>Destination</label>
      <select id="transfer-dest">
        <option value="" ${selected === "" ? "selected" : ""}>— choose —</option>
        ${destinations
          .map(
            (b) =>
              `<option value="${esc(b.id)}" ${selected === b.id ? "selected" : ""}>${esc(b.name)}</option>`,
          )
          .join("")}
      </select>
    </div>
    <div class="field"><label>Capture orbit</label>
      <div class="row">
        <input type="number" id="transfer-alt" value="400" min="30" step="50" title="Capture altitude (km)" />
        <span class="unit">km ×</span>
        <input type="number" id="transfer-inc" value="30" min="0" max="98" step="1" title="Capture inclination (deg)" />
        <span class="unit">°</span>
      </div>
      <p class="hint">Set before departure — the capture burn is reserved from your tanks.</p>
    </div>
  `);

  const planner = craft.transfer_planner;
  if (planner) {
    const now = s.sim_seconds;
    const windowEta = Math.max(0, planner.next_window_sim_seconds - now);
    const phase =
      planner.phase_angle_deg != null && planner.required_phase_angle_deg != null
        ? `<div class="stat"><label>Phase angle</label><strong>${planner.phase_angle_deg.toFixed(1)}°</strong><span class="stat-sub">need ${planner.required_phase_angle_deg.toFixed(1)}°</span></div>`
        : "";
    parts.push(`
      <div class="field"><label>Window</label>
        <div class="stat"><label>Status</label><strong class="${planner.window_open ? "pass-active" : ""}">${planner.window_open ? "OPEN" : `opens in ${formatDuration(windowEta)}`}</strong></div>
        ${phase}
        <div class="stat"><label>Cruise time</label><strong>${formatDuration(planner.transfer_seconds)}</strong></div>
      </div>
      <div class="field"><label>Costs</label>
        <div class="stat"><label>Injection</label><strong class="${planner.affordable ? "" : "negative"}">$${planner.injection_cost_usd.toLocaleString()}</strong><span class="stat-sub">launch provider (cash)</span></div>
        <div class="stat"><label>Capture Δv</label><strong class="${planner.propellant_ok ? "" : "negative"}">${planner.capture_dv_km_s.toFixed(2)} km/s</strong><span class="stat-sub">${planner.capture_propellant_kg.toFixed(1)} kg from your tanks</span></div>
      </div>
      ${planner.window_open ? "" : `<button id="btn-warp-window" data-warp="${planner.next_window_sim_seconds + 60}">Warp to window</button>`}
      <button id="btn-begin-transfer" class="primary" ${planner.window_open && planner.affordable && planner.propellant_ok ? "" : "disabled"}
        title="${planner.window_open ? (planner.affordable ? (planner.propellant_ok ? "Buy the injection and depart" : "Not enough propellant for the capture burn") : "Insufficient funds for the injection") : "Transfer window closed"}">
        Begin transfer — $${planner.injection_cost_usd.toLocaleString()}
      </button>
    `);
  } else if (selected === "") {
    parts.push(
      `<p class="hint">Pick a destination to see windows and costs.</p>`,
    );
  }
  return parts;
}

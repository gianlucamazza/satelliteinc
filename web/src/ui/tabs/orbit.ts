import type { Snapshot, SpacecraftSnapshot } from "../../types.ts";

/** Orbit tab: burns, Hohmann, plane change, elements readout. */
export function renderOrbitTab(
  _s: Snapshot,
  craft: SpacecraftSnapshot | null,
): string[] {
  const parts: string[] = [];
  if (craft?.phase !== "OnOrbit") {
    parts.push(`<p class="hint">Launch first to access maneuvers.</p>`);
    return parts;
  }

  parts.push(`<button id="btn-prograde">Prograde +10 m/s</button>`);
  parts.push(`<button id="btn-retrograde">Retrograde −10 m/s</button>`);

  if (craft.pending_hohmann) {
    const p = craft.pending_hohmann.plan;
    const apsisNeeded = p.raising ? "apogee" : "perigee";
    parts.push(`
      <div class="alert warn">Hohmann → ${p.target_altitude_km} km. Burn 2 at <strong>${apsisNeeded}</strong> when Apsis shows READY. <span id="apsis-eta" class="stat-sub"></span></div>
      <button id="btn-hohmann-2" class="primary" ${!craft.at_apsis ? "disabled" : ""}>Circularize (Burn 2)</button>
    `);
  } else if (craft.hohmann_preview) {
    const preview = craft.hohmann_preview;
    const apsisNeeded = preview.raising ? "perigee" : "apogee";
    parts.push(`
      <div class="field"><label>Hohmann target altitude (km)</label>
        <input type="range" id="maneuver-alt" min="250" max="1200" value="${craft.maneuver_target_altitude_km}" />
        <span id="maneuver-alt-val">${craft.maneuver_target_altitude_km} km</span>
      </div>
      <p class="hint">Δv ${(preview.total_dv_km_s * 1000).toFixed(0)} m/s total · burn 1 at ${apsisNeeded} when Apsis READY <span id="apsis-eta" class="stat-sub"></span></p>
      <button id="btn-hohmann-1" class="primary" ${!craft.at_apsis ? "disabled" : ""}>Start Hohmann (Burn 1)</button>
    `);
  }

  const el = craft.elements;
  if (el) {
    parts.push(`
      <div class="stat-grid">
        <div class="stat"><label>Inclination</label><strong>${el.inclination_deg.toFixed(1)}°</strong></div>
        <div class="stat"><label>RAAN</label><strong>${el.raan_deg.toFixed(1)}°</strong><span class="stat-sub">J2 precesses node</span></div>
        <div class="stat"><label>Eccentricity</label><strong>${el.eccentricity.toFixed(4)}</strong></div>
        <div class="stat"><label>Solar flux</label><strong>${(craft.solar_flux * 100).toFixed(0)}%</strong><span class="stat-sub">1 AU = 100%</span></div>
      </div>
    `);
  }

  parts.push(`
    <div class="field"><label>Plane change (Δ inclination)</label>
      <input type="range" id="plane-di" min="-30" max="30" step="0.5" value="0" />
      <span id="plane-di-val">0.0°</span>
    </div>
    <p class="hint">Burns only at a node (equator crossing) — Node <strong id="plane-node" class="${craft.at_node ? "pass-active" : ""}">${craft.at_node ? "READY" : "—"}</strong> · Δv <span id="plane-dv">0 m/s</span></p>
    <button id="btn-plane-change" ${!craft.at_node ? "disabled" : ""}>Plane change burn</button>
  `);
  return parts;
}

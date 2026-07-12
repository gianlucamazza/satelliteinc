import type { GameContent, Snapshot } from "../types.ts";
import {
  bankruptcyRunwayDays,
  commandLabel,
  contractDaysLeft,
  contractProgress,
  dataFlowStatus,
  formatDuration,
  nextPassStation,
  orbitRequirementText,
  payloadModeLabel,
} from "./format.ts";

export type HudTab = "mission" | "ops" | "orbit" | "transfer" | "company";

/** Discrete time-acceleration presets (0 = paused). */
export const ACCEL_PRESETS = [0, 1, 10, 100, 1000, 10000] as const;

/** Escape content-derived strings interpolated into innerHTML. */
function esc(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** Stat cell with a fill bar showing value against capacity. */
function gauge(
  label: string,
  value: number,
  capacity: number,
  unit: string,
  opts: { lowIsBad?: boolean; highIsWarn?: boolean } = {},
): string {
  const pct =
    capacity > 0 ? Math.max(0, Math.min(100, (value / capacity) * 100)) : 0;
  const level =
    opts.lowIsBad && pct < 20
      ? " danger"
      : opts.highIsWarn && pct > 80
        ? " warn"
        : "";
  return `<div class="stat gauge">
    <label>${label}</label>
    <strong>${value.toFixed(value < 10 ? 1 : 0)} ${unit} <span class="stat-sub">(${pct.toFixed(0)}%)</span></strong>
    <div class="progress-bar thin"><div class="progress-fill${level}" style="width:${pct}%"></div></div>
  </div>`;
}

export interface PanelCallbacks {
  onSelectBus: (busId: string) => void;
  onSetOrbit: (altitude: number, inclination: number) => void;
  onAcceptContract: (contractId: string) => void;
  onLaunch: () => void;
  onToggleDownlink: (active: boolean) => void;
  onTimeAccel: (multiplier: number) => void;
  onProgradeBurn: (deltaV: number) => void;
  onRetrogradeBurn: (deltaV: number) => void;
  onSetManeuverTarget: (altitude: number) => void;
  onStartHohmann: () => void;
  onCompleteHohmann: () => void;
  onSelectDownlinkStation: (stationId: string) => void;
  onSetPayloadMode: (mode: string) => void;
  onQueueCommand: (command: string, stationId: string) => void;
  onPurchaseTech: (techId: string) => void;
  onSetGameMode: (mode: string) => void;
  onDecommission: (salvageUsd: number) => void;
  onSetTransferTarget: (
    bodyId: string,
    captureAltitudeKm: number,
    captureInclinationDeg: number,
  ) => void;
  onBeginTransfer: () => void;
  onUnlockProgram: (programId: string) => void;
  onWarpTo: (targetSimSeconds: number) => void;
  onRestartCampaign: () => void;
  onShowHelp: () => void;
  onAutoDownlinkChange: (active: boolean) => void;
  onShowTleRefsChange: (active: boolean) => void;
  onShowLegendChange: (active: boolean) => void;
  getShowLegend: () => boolean;
  onLoadScenario: (scenarioId: string) => void;
  onExportSave: () => void;
  onImportSave: () => void;
  getAutoDownlink: () => boolean;
  getShowTleRefs: () => boolean;
}

export class HudPanel {
  private readonly statusEl: HTMLElement;
  private readonly actionEl: HTMLElement;
  private readonly eventEl: HTMLElement;
  private readonly phaseEl: HTMLElement;
  private readonly tabBar: HTMLElement;
  private readonly toastEl: HTMLElement;
  private currentSnapshot: Snapshot | null = null;
  private actionStructureKey = "";
  private statusFingerprint = "";
  private activeTab: HudTab = "mission";
  private transfersUnlocked = false;
  private lastContractToast = "";
  private toastTimer: number | undefined;
  private victoryDismissed = false;
  private lastEndScreenKey = "";
  private readonly tabAttention = new Set<HudTab>();

  constructor(
    private readonly content: GameContent,
    private readonly callbacks: PanelCallbacks,
  ) {
    this.statusEl = document.getElementById("status-panel")!;
    this.actionEl = document.getElementById("action-panel")!;
    this.eventEl = document.getElementById("event-log")!;
    this.phaseEl = document.getElementById("phase-label")!;
    this.tabBar = document.getElementById("tab-bar")!;
    this.toastEl = document.getElementById("toast")!;
    this.bindActionDelegation();
    this.bindTabs();
    this.bindEndScreen();
    this.renderTabs();
  }

  setTab(tab: HudTab): void {
    this.activeTab = tab;
    this.tabAttention.delete(tab);
    this.renderTabs();
    if (this.currentSnapshot) {
      this.renderActions(this.currentSnapshot);
    }
  }

  /** Mark a tab with an attention dot instead of yanking the player to it. */
  markTabAttention(tab: HudTab): void {
    if (tab === this.activeTab) return;
    if (!this.tabAttention.has(tab)) {
      this.tabAttention.add(tab);
      this.renderTabs();
    }
  }

  update(snapshot: Snapshot): void {
    this.currentSnapshot = snapshot;
    if (snapshot.transfers_unlocked !== this.transfersUnlocked) {
      this.transfersUnlocked = snapshot.transfers_unlocked;
      if (!this.transfersUnlocked && this.activeTab === "transfer") {
        this.activeTab = "mission";
      }
      this.renderTabs();
    }
    const bodyName =
      snapshot.spacecraft && snapshot.spacecraft.body_id !== "earth"
        ? snapshot.bodies.find((b) => b.id === snapshot.spacecraft?.body_id)
            ?.name
        : null;
    this.phaseEl.textContent = `${snapshot.phase.replace(/_/g, " ")} · ${snapshot.game_mode}${bodyName ? ` · ${bodyName}` : ""}`;
    this.renderStatus(snapshot);
    this.renderActions(snapshot);
    this.updateActionDynamicState(snapshot);
    this.maybeToastContract(snapshot);
    this.renderEndScreen(snapshot);
  }

  logEvent(message: string, className = ""): void {
    const p = document.createElement("p");
    const ts = this.currentSnapshot
      ? `D${this.currentSnapshot.sim_days.toFixed(1)} `
      : "";
    p.textContent = `${ts}${message}`;
    if (className) p.className = className;
    this.eventEl.prepend(p);
    while (this.eventEl.children.length > 30) {
      this.eventEl.lastChild?.remove();
    }
  }

  logError(message: string): void {
    this.logEvent(message, "error");
    this.showToast(message, "error");
  }

  showToast(
    message: string,
    kind: "info" | "error" | "success" = "info",
  ): void {
    this.toastEl.textContent = message;
    this.toastEl.className = `toast visible ${kind}`;
    window.clearTimeout(this.toastTimer);
    this.toastTimer = window.setTimeout(() => {
      this.toastEl.classList.remove("visible");
    }, 4000);
  }

  private maybeToastContract(s: Snapshot): void {
    const c = s.active_contract;
    if (!c) return;
    const days = contractDaysLeft(c, s.sim_seconds);
    // Key the guard on the contract so each new contract can warn again.
    const key = `deadline:${c.template_id}`;
    if (days < 1 && this.lastContractToast !== key) {
      this.lastContractToast = key;
      this.showToast("Contract deadline in less than 1 sim-day!", "error");
    }
  }

  private bindEndScreen(): void {
    document
      .getElementById("endscreen-restart")
      ?.addEventListener("click", () => {
        this.victoryDismissed = false;
        this.callbacks.onRestartCampaign();
      });
    document
      .getElementById("endscreen-sandbox")
      ?.addEventListener("click", () => {
        this.victoryDismissed = false;
        this.callbacks.onSetGameMode("sandbox");
      });
    document
      .getElementById("endscreen-continue")
      ?.addEventListener("click", () => {
        this.victoryDismissed = true;
        document.getElementById("endscreen")?.classList.add("hidden");
      });
  }

  private renderEndScreen(s: Snapshot): void {
    const el = document.getElementById("endscreen");
    if (!el) return;
    const isEnd = s.phase === "victory" || s.phase === "game_over";
    const key = `${s.phase}|${s.outcome ?? ""}|${this.victoryDismissed}`;
    if (key === this.lastEndScreenKey) return;
    this.lastEndScreenKey = key;

    if (!isEnd) {
      this.victoryDismissed = false;
      el.classList.add("hidden");
      return;
    }
    if (s.phase === "victory" && this.victoryDismissed) {
      el.classList.add("hidden");
      return;
    }

    const title = document.getElementById("endscreen-title")!;
    const body = document.getElementById("endscreen-body")!;
    const continueBtn = document.getElementById("endscreen-continue")!;
    if (s.phase === "victory") {
      title.textContent = "Victory!";
      body.textContent =
        "All contracts completed — Satellite Inc is the market leader. Keep flying or start over.";
    } else if (s.outcome === "bankrupt") {
      title.textContent = "Bankrupt";
      body.textContent =
        "Cash fell below −$50k. Operating costs never sleep — next time keep a contract in the pipeline.";
    } else {
      title.textContent = "Reputation lost";
      body.textContent =
        "Reputation hit zero after too many failed contracts. No one will sign with Satellite Inc anymore.";
    }
    continueBtn.style.display = s.phase === "victory" ? "" : "none";
    el.classList.toggle("victory", s.phase === "victory");
    el.classList.toggle("game-over", s.phase === "game_over");
    el.classList.remove("hidden");
  }

  private bindTabs(): void {
    this.tabBar.addEventListener("click", (e) => {
      const btn = (e.target as HTMLElement).closest<HTMLButtonElement>(
        "[data-tab]",
      );
      if (!btn) return;
      this.setTab(btn.dataset.tab as HudTab);
    });
  }

  private renderTabs(): void {
    const tabs: { id: HudTab; label: string }[] = [
      { id: "mission", label: "Mission" },
      { id: "ops", label: "Ops" },
      { id: "orbit", label: "Orbit" },
      ...(this.transfersUnlocked
        ? [{ id: "transfer" as HudTab, label: "Transfer" }]
        : []),
      { id: "company", label: "Company" },
    ];
    this.tabBar.innerHTML = tabs
      .map(
        (t) =>
          `<button type="button" class="tab${this.activeTab === t.id ? " active" : ""}${this.tabAttention.has(t.id) ? " attention" : ""}" data-tab="${t.id}">${t.label}</button>`,
      )
      .join("");
  }

  private computeStatusFingerprint(s: Snapshot): string {
    const c = s.spacecraft;
    const contract = s.active_contract;
    return [
      s.cash_usd.toFixed(0),
      s.reputation.toFixed(0),
      s.sim_days.toFixed(1),
      s.time_accel.toFixed(0),
      c?.buffer_mb.toFixed(1) ?? "",
      c?.altitude_km.toFixed(0) ?? "",
      c?.in_sunlight ?? "",
      c?.in_pass ?? "",
      c?.downlink_rate_mb_s.toFixed(2) ?? "",
      c?.power_wh.toFixed(0) ?? "",
      c?.anomaly_label ?? "",
      c?.command_queue.length ?? "",
      contract?.downlink_mb_done.toFixed(0) ?? "",
      s.active_contract_orbit_ok ?? "",
      s.outcome ?? "",
      s.ops_cost_usd_per_day.toFixed(0),
      c ? Math.round((c.power_wh / c.battery_wh_capacity) * 50) : "",
      c ? Math.round((c.buffer_mb / c.buffer_capacity_mb) * 50) : "",
      c?.propellant_kg.toFixed(2) ?? "",
      dataFlowStatus(s)?.text ?? "",
      Math.round(bankruptcyRunwayDays(s) ?? -1),
      s.ground_stations
        .map(
          (st) =>
            // Coarse ETA bucket so the "next pass" countdown refreshes.
            `${st.id}:${st.in_pass}:${st.elevation_deg.toFixed(0)}:${Math.round((st.next_pass_in_seconds ?? -1) / 15)}`,
        )
        .join(","),
    ].join("|");
  }

  private renderStatus(s: Snapshot): void {
    const fp = this.computeStatusFingerprint(s);
    if (fp === this.statusFingerprint) return;
    this.statusFingerprint = fp;

    const craft = s.spacecraft;
    const contract = s.active_contract;
    const nextPass = nextPassStation(s);
    const craftBodyId = craft?.body_id ?? "earth";

    const stationRows = s.ground_stations
      .filter(
        (st) =>
          st.body_id === craftBodyId ||
          (st.dsn && craftBodyId !== "earth"),
      )
      .map((st) => {
        const passEta =
          st.in_pass || st.next_pass_in_seconds == null
            ? ""
            : `<span class="stat-sub">next ${formatDuration(st.next_pass_in_seconds)}</span>`;
        const idle =
          st.dsn && st.body_id !== craftBodyId
            ? "NO LINK"
            : `${st.elevation_deg.toFixed(0)}°`;
        const active = st.dsn && st.body_id !== craftBodyId ? "LINK" : "PASS";
        return `<div class="stat"><label>${esc(st.name)}</label><strong class="${st.in_pass ? "pass-active" : ""}">${st.in_pass ? active : idle}</strong><span class="stat-sub">${st.bandwidth_mbps.toFixed(1)} Mbps</span>${passEta}</div>`;
      })
      .join("");

    const lightDelayLine =
      craft && craft.light_delay_seconds > 1.0
        ? `<div class="stat"><label>Light delay</label><strong>${craft.light_delay_seconds < 120 ? `${craft.light_delay_seconds.toFixed(0)} s` : `${(craft.light_delay_seconds / 60).toFixed(1)} min`}</strong><span class="stat-sub">one-way to Earth</span></div>`
        : "";

    const orbitReq = contract ? orbitRequirementText(contract) : "";
    const orbitSpecLine =
      contract && orbitReq
        ? s.active_contract_orbit_ok === false
          ? `<div class="orbit-spec bad">Orbit OUT OF SPEC — needs ${orbitReq}</div>`
          : `<div class="orbit-spec ok">Orbit OK (${orbitReq})</div>`
        : "";

    const contractBlock = contract
      ? `<div class="contract-card">
          <div class="contract-head">
            <span>${esc(contract.name)}</span>
            <span class="deadline${contractDaysLeft(contract, s.sim_seconds) < 2 ? " urgent" : ""}">${contractDaysLeft(contract, s.sim_seconds).toFixed(1)} d left</span>
          </div>
          ${orbitSpecLine}
          <div class="progress-bar"><div class="progress-fill" style="width:${contractProgress(contract)}%"></div></div>
          <div class="progress-label">${contract.downlink_mb_done.toFixed(0)} / ${contract.downlink_mb_required.toFixed(0)} MB · +$${contract.payout_usd.toLocaleString()} · +${contract.reputation_reward} rep</div>
          ${contract.min_distinct_stations > 0 ? `<div class="progress-label">Stations: ${contract.stations_used.length} / ${contract.min_distinct_stations} (${esc(contract.stations_used.join(", ")) || "none yet"})</div>` : ""}
        </div>`
      : `<p class="hint">No active contract — pick one in Mission tab.</p>`;

    const flow = dataFlowStatus(s);
    const flowLine =
      flow && craft?.phase === "OnOrbit"
        ? `<div class="orbit-spec ${flow.kind === "bad" ? "bad" : flow.kind === "ok" ? "ok" : "warn"}">${flow.text}</div>`
        : "";

    const craftBlock = craft
      ? `<div class="stat-grid">
          <div class="stat"><label>Bus</label><strong>${esc(craft.bus_name)}</strong></div>
          <div class="stat"><label>Altitude</label><strong>${craft.altitude_km.toFixed(0)} km</strong></div>
          ${gauge("Battery", craft.power_wh, craft.battery_wh_capacity, "Wh", { lowIsBad: true })}
          ${gauge("Buffer", craft.buffer_mb, craft.buffer_capacity_mb, "MB", { highIsWarn: true })}
          ${gauge("Propellant", craft.propellant_kg, craft.propellant_capacity_kg, "kg", { lowIsBad: true })}
          <div class="stat"><label>Δv left</label><strong>${(craft.delta_v_remaining_km_s * 1000).toFixed(0)} m/s</strong></div>
          <div class="stat"><label>Sunlight</label><strong class="${craft.in_sunlight ? "pass-active" : ""}">${craft.in_sunlight ? "YES" : "Eclipse"}</strong></div>
          <div class="stat"><label>Downlink</label><strong>${craft.downlink_active ? `${(craft.downlink_rate_mb_s * 1000).toFixed(0)} KB/s` : "Off"}</strong></div>
          <div class="stat"><label>Imaging</label><strong>${craft.imaging_rate_mb_s.toFixed(1)} MB/s</strong></div>
          <div class="stat"><label>Payload</label><strong>${payloadModeLabel(craft.payload_mode)}</strong></div>
          <div class="stat"><label>Apsis</label><strong class="${craft.at_apsis ? "pass-active" : ""}">${craft.at_apsis ? "READY" : "—"}</strong></div>
        </div>
        ${flowLine}
        ${craft.anomaly_label ? `<div class="alert warn">⚠ ${esc(craft.anomaly_label)} — queue Clear anomaly or Safe mode at a station</div>` : ""}
        ${craft.command_queue.length ? `<ul class="queue-list">${craft.command_queue.map((q) => `<li>${esc(commandLabel(q.command))} → ${esc(q.station_id)}</li>`).join("")}</ul>` : ""}`
      : "";

    const nextPassBlock = nextPass
      ? `<p class="hint pass-hint">Next pass candidate: <strong>${esc(nextPass.name)}</strong> (${nextPass.elevation.toFixed(0)}°)</p>`
      : craft?.phase === "OnOrbit"
        ? `<p class="hint">No station above horizon — increase time accel or wait.</p>`
        : "";

    const runway = bankruptcyRunwayDays(s);
    const runwayBanner =
      runway != null && (runway < 30 || s.cash_usd < 0)
        ? `<div class="alert danger">💸 Bankrupt in ~${Math.max(0, runway).toFixed(0)} d at current burn — complete a contract or decommission</div>`
        : "";

    this.statusEl.innerHTML = `
      <div class="stat-grid">
        <div class="stat"><label>Cash</label><strong class="${s.cash_usd < 0 ? "negative" : ""}">$${s.cash_usd.toLocaleString(undefined, { maximumFractionDigits: 0 })}</strong></div>
        ${gauge("Reputation", s.reputation, 100, "", {})}
        <div class="stat"><label>Sim Time</label><strong>Day ${s.sim_days.toFixed(2)}</strong></div>
        <div class="stat"><label>Time Accel</label><strong>${s.time_accel === 0 ? "PAUSED" : `${s.time_accel.toFixed(0)}×`}</strong></div>
        ${s.ops_cost_usd_per_day > 0 ? `<div class="stat"><label>Ops cost</label><strong>$${s.ops_cost_usd_per_day.toLocaleString(undefined, { maximumFractionDigits: 0 })}/day</strong></div>` : ""}
      </div>
      ${runwayBanner}
      ${contractBlock}
      ${craftBlock}
      ${nextPassBlock}
      <div class="stat-grid stations">${lightDelayLine}${stationRows}</div>
    `;
  }

  private renderActions(s: Snapshot): void {
    // Bail before building any HTML: this runs every frame.
    const structureKey = `${this.activeTab}|${this.computeActionStructureKey(s)}`;
    if (structureKey === this.actionStructureKey) return;
    this.actionStructureKey = structureKey;

    const parts: string[] = [];

    parts.push(`
      <div class="toolbar">
        <div class="field">
          <label>Time</label>
          <div class="accel-row">
            ${ACCEL_PRESETS.map(
              (p) =>
                `<button type="button" class="accel-btn" data-accel="${p}" title="${p === 0 ? "Pause (Space)" : `${p}× speed`}">${p === 0 ? "⏸" : `${p}×`}</button>`,
            ).join("")}
          </div>
        </div>
        <label class="toggle">
          <input type="checkbox" id="auto-downlink" ${this.callbacks.getAutoDownlink() ? "checked" : ""} />
          Auto-downlink on pass
        </label>
      </div>
    `);

    switch (this.activeTab) {
      case "mission":
        parts.push(...this.renderMissionTab(s));
        break;
      case "ops":
        parts.push(...this.renderOpsTab(s));
        break;
      case "orbit":
        parts.push(...this.renderOrbitTab(s));
        break;
      case "transfer":
        parts.push(...this.renderTransferTab(s));
        break;
      case "company":
        parts.push(...this.renderCompanyTab(s));
        break;
    }

    this.actionEl.innerHTML = parts.join("");
  }

  private renderMissionTab(s: Snapshot): string[] {
    const parts: string[] = [];
    const craft = s.spacecraft;

    const scenarios = this.content.scenarios ?? [];
    if (scenarios.length) {
      parts.push(`<div class="field"><label>Scenarios</label><select id="scenario-select">
        ${scenarios.map((sc) => `<option value="${esc(sc.id)}">${esc(sc.name)}${sc.body_id && sc.body_id !== "earth" ? ` [${esc(sc.body_id)}]` : ""}</option>`).join("")}
      </select></div>`);
      parts.push(
        `<button id="btn-load-scenario" class="primary">Load scenario</button>`,
      );
      parts.push(
        `<p class="hint" id="scenario-desc">${esc(scenarios[0]?.description ?? "")}</p>`,
      );
    }

    if (!craft || s.phase === "design" || craft.phase === "Design") {
      const buses = this.content.buses.filter((b) =>
        s.unlocked_buses.includes(b.id),
      );
      const selected = craft?.bus_id ?? buses[0]?.id ?? "";
      parts.push(`<div class="field"><label>Select bus</label>
        <select id="bus-select">
          ${buses
            .map(
              (b) =>
                `<option value="${esc(b.id)}" ${b.id === selected ? "selected" : ""}>${esc(b.name)} — $${b.launch_cost_usd.toLocaleString()}</option>`,
            )
            .join("")}
        </select>
        <p class="bus-spec" id="bus-spec"></p>
      </div>`);

      if (craft) {
        parts.push(`
          <div class="field"><label>Altitude (km)</label>
            <input type="range" id="altitude" min="200" max="1200" value="${craft.target_altitude_km}" />
            <span id="altitude-val">${craft.target_altitude_km} km</span>
          </div>
          <div class="field"><label>Inclination (°)</label>
            <input type="range" id="inclination" min="28" max="98" value="${craft.target_inclination_deg}" />
            <span id="inclination-val">${craft.target_inclination_deg}°</span>
          </div>
        `);
      }

      if (!s.active_contract) {
        parts.push(
          `<div class="field"><label>Contract marketplace</label></div>`,
        );
        for (const offer of s.marketplace) {
          const repPct = Math.min(
            100,
            (s.reputation / Math.max(offer.reputation_required, 1)) * 100,
          );
          const orbitReq = orbitRequirementText(offer);
          parts.push(`
            <div class="offer-card ${offer.available ? "" : "locked"}">
              <button class="btn-contract" data-contract="${esc(offer.template_id)}" ${offer.available ? "" : "disabled"}>
                ${esc(offer.name)}
              </button>
              ${offer.repeat ? `<span class="chip repeat">REPEAT — ×0.6 payout, no rep</span>` : ""}
              <p class="offer-meta">$${offer.payout_usd.toLocaleString()} · ${offer.downlink_mb} MB · ${offer.deadline_days} d · rep ${offer.reputation_required}${offer.min_distinct_stations > 0 ? ` · ${offer.min_distinct_stations} stations` : ""}${orbitReq ? ` · ${orbitReq}` : ""}</p>
              <p class="offer-desc">${esc(offer.description)}</p>
              ${!offer.available ? `<div class="progress-bar thin"><div class="progress-fill" style="width:${repPct}%"></div></div>` : ""}
            </div>
          `);
        }
      } else {
        const cost =
          this.content.buses.find((b) => b.id === craft?.bus_id)
            ?.launch_cost_usd ?? 0;
        parts.push(`
          <button id="btn-launch" class="primary" ${s.cash_usd < cost ? "disabled" : ""}>
            Launch — $${cost.toLocaleString()}
          </button>
          <p class="hint">Checklist: bus configured · contract accepted · funds available</p>
        `);
      }
    } else if (!s.active_contract) {
      parts.push(
        `<div class="success-banner">Contract complete! Choose your next mission.</div>`,
      );
      for (const offer of s.marketplace) {
        const orbitReq = orbitRequirementText(offer);
        parts.push(`
          <button class="btn-contract" data-contract="${esc(offer.template_id)}" ${offer.available ? "" : "disabled"}>
            ${esc(offer.name)} — $${offer.payout_usd.toLocaleString()}${offer.repeat ? " (repeat)" : ""}
          </button>
          ${orbitReq ? `<p class="hint">${orbitReq}</p>` : ""}
        `);
      }
      parts.push(this.decommissionButton(craft.bus_id));
    } else {
      parts.push(
        `<p class="hint">Satellite on orbit. Use Ops and Orbit tabs during the mission.</p>`,
      );
      parts.push(this.decommissionButton(craft.bus_id));
    }
    return parts;
  }

  private decommissionButton(busId: string): string {
    const salvage = this.salvageUsd(busId);
    return `<button id="btn-decommission" class="danger" title="Retire the satellite and return to the design phase">Decommission — salvage $${salvage.toLocaleString()}</button>`;
  }

  private salvageUsd(busId: string): number {
    const cost =
      this.content.buses.find((b) => b.id === busId)?.launch_cost_usd ?? 0;
    return Math.round(cost * 0.15);
  }

  private renderOpsTab(s: Snapshot): string[] {
    const parts: string[] = [];
    const craft = s.spacecraft;
    if (craft?.phase !== "OnOrbit") {
      parts.push(`<p class="hint">Launch first to access ground ops.</p>`);
      return parts;
    }

    parts.push(`
      <div class="field"><label>Downlink station</label>
        <select id="downlink-station">
          ${s.ground_stations
            .filter(
              (st) =>
                st.body_id === craft.body_id ||
                (st.dsn && craft.body_id !== "earth"),
            )
            .map((st) => `<option value="${esc(st.id)}" ${craft.downlink_station_id === st.id ? "selected" : ""}>${esc(st.name)} (${st.bandwidth_mbps.toFixed(1)} Mbps)</option>`)
            .join("")}
        </select>
        <p class="hint" title="Click a station marker on the globe to select">Tip: click station on globe</p>
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
      <div class="field"><label>Command queue (runs at pass start)</label></div>
      <button id="btn-queue-clear" title="Clears active anomaly">Clear anomaly @ station</button>
      <button id="btn-queue-safe" title="Engages safe mode for 5 sim-minutes">Safe mode @ station</button>
      <button id="btn-queue-reboot" title="Reboots comms subsystem">Reboot comms @ station</button>
    `);
    return parts;
  }

  private renderTransferTab(s: Snapshot): string[] {
    const parts: string[] = [];
    const craft = s.spacecraft;

    if (craft?.phase === "Transfer" && craft.transfer) {
      const t = craft.transfer;
      const eta = Math.max(0, t.arrival_sim_seconds - s.sim_seconds);
      const targetName =
        s.bodies.find((b) => b.id === t.target_body_id)?.name ??
        t.target_body_id;
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
      parts.push(`<p class="hint">Pick a destination to see windows and costs.</p>`);
    }
    return parts;
  }

  private renderOrbitTab(s: Snapshot): string[] {
    const parts: string[] = [];
    const craft = s.spacecraft;
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
    return parts;
  }

  private renderCompanyTab(s: Snapshot): string[] {
    const parts: string[] = [];
    if (s.programs.length > 0 && s.game_mode === "campaign") {
      const rows = s.programs
        .map((p) => {
          const status = p.completed
            ? `<strong class="pass-active">COMPLETE</strong>`
            : p.unlocked
              ? `<strong>ACTIVE</strong>`
              : `<button class="btn-unlock-program" data-program="${esc(p.id)}"
                  ${s.reputation >= p.reputation_required && p.keystone_done && s.cash_usd >= p.buyin_usd ? "" : "disabled"}
                  title="${p.keystone_done ? (s.reputation >= p.reputation_required ? (s.cash_usd >= p.buyin_usd ? "Buy into the program" : "Insufficient funds") : `Requires reputation ${p.reputation_required}`) : `Complete ${p.requires_keystone ?? ""} first`}">
                  Unlock — $${p.buyin_usd.toLocaleString()}
                </button>`;
          return `<div class="stat"><label>${esc(p.name)}</label>${status}<span class="stat-sub">rep ${p.reputation_required}${p.completion_grant_usd > 0 ? ` · grant $${(p.completion_grant_usd / 1000).toFixed(0)}k` : ""}</span></div>`;
        })
        .join("");
      parts.push(
        `<div class="field"><label>Programs</label>${rows}<p class="hint">Each program opens a body's contracts; its keystone unlocks the next gate.</p></div>`,
      );
    }
    parts.push(`
      <div class="field"><label>Game mode</label>
        <select id="game-mode" title="Campaign gates contracts by reputation; Sandbox is relaxed">
          <option value="sandbox" ${s.game_mode === "sandbox" ? "selected" : ""}>Sandbox</option>
          <option value="campaign" ${s.game_mode === "campaign" ? "selected" : ""}>Campaign</option>
        </select>
        <p class="hint">${s.game_mode === "sandbox" ? "Sandbox: all buses, all contracts available, −5 rep on failure." : "Campaign: rep gates, buses unlock via R&D, −10 rep on failure."}</p>
      </div>
      <button id="btn-export-save">Save game</button>
      <button id="btn-import-save">Load game</button>
      <button id="btn-help">Help / Tutorial</button>
      <label class="toggle">
        <input type="checkbox" id="show-tle" ${this.callbacks.getShowTleRefs() ? "checked" : ""} />
        Show TLE reference satellites (ISS)
      </label>
      <label class="toggle">
        <input type="checkbox" id="show-legend" ${this.callbacks.getShowLegend() ? "checked" : ""} />
        Show globe legend
      </label>
      <p class="hint">Shortcuts: Space pause · +/− speed · 1-4 tabs · D downlink · ? help</p>
    `);

    if (s.unlocked_tech.length) {
      parts.push(`<div class="field"><label>Unlocked R&amp;D</label><div class="chip-row">
        ${s.unlocked_tech
          .map((id) => {
            const t = this.content.tech_tree.find((n) => n.id === id);
            return `<span class="chip">${esc(t?.name ?? id)}</span>`;
          })
          .join("")}
      </div></div>`);
    }

    parts.push(`<div class="field"><label>Research tree</label></div>`);
    parts.push(`<div class="tech-tree">`);
    for (const tech of this.content.tech_tree) {
      const owned = s.unlocked_tech.includes(tech.id);
      const locked = tech.requires.some((r) => !s.unlocked_tech.includes(r));
      const state = owned ? "owned" : locked ? "locked" : "available";
      parts.push(`
        <div class="tech-node ${state}">
          <div class="tech-name">${esc(tech.name)}</div>
          <div class="tech-desc">${esc(tech.description)}</div>
          ${owned ? `<span class="chip owned">Owned</span>` : `<button class="btn-tech" data-tech="${esc(tech.id)}" ${locked || s.cash_usd < tech.cost_usd ? "disabled" : ""}>$${tech.cost_usd.toLocaleString()}</button>`}
          ${locked && !owned ? `<span class="tech-req">Requires: ${esc(tech.requires.join(", "))}</span>` : ""}
        </div>
      `);
    }
    parts.push(`</div>`);
    return parts;
  }

  private computeActionStructureKey(s: Snapshot): string {
    const craft = s.spacecraft;
    return [
      s.phase,
      s.game_mode,
      craft?.phase ?? "",
      craft?.bus_id ?? "",
      s.active_contract?.template_id ?? "none",
      s.transfers_unlocked,
      s.programs
        .map((p) => `${p.id}:${p.unlocked}:${p.completed}:${p.keystone_done}`)
        .join(","),
      craft?.transfer_planner?.target_body_id ?? "",
      craft?.transfer_planner?.window_open ?? "",
      craft?.transfer_planner
        ? Math.round(
            (craft.transfer_planner.next_window_sim_seconds - s.sim_seconds) /
              900,
          )
        : "",
      craft?.transfer ? Math.round(craft.transfer.progress * 200) : "",
      craft?.transfer
        ? Math.round(
            (craft.transfer.arrival_sim_seconds - s.sim_seconds) / 900,
          )
        : "",
      s.unlocked_tech.join(","),
      s.unlocked_buses.join(","),
      craft?.pending_hohmann
        ? "hohmann2"
        : craft?.hohmann_preview
          ? "hohmann1"
          : "nohohmann",
      s.outcome ?? "",
      s.marketplace
        .map((m) => `${m.template_id}:${m.available}:${m.repeat}`)
        .join(","),
    ].join("|");
  }

  private updateActionDynamicState(s: Snapshot): void {
    const craft = s.spacecraft;

    for (const btn of document.querySelectorAll<HTMLButtonElement>(
      ".accel-btn",
    )) {
      btn.classList.toggle(
        "active",
        Number(btn.dataset.accel) === s.time_accel,
      );
    }

    const busSel = document.getElementById(
      "bus-select",
    ) as HTMLSelectElement | null;
    const busSpec = document.getElementById("bus-spec");
    if (busSel && busSpec) {
      const bus = this.content.buses.find((b) => b.id === busSel.value);
      if (bus) {
        busSpec.textContent = `${bus.mass_kg} kg · ${bus.solar_power_w} W solar · ${bus.comms_mbps} Mbps · ${bus.propellant_kg} kg prop`;
      }
    }

    const launchBtn = document.getElementById(
      "btn-launch",
    ) as HTMLButtonElement | null;
    if (launchBtn && craft) {
      const cost =
        this.content.buses.find((b) => b.id === craft.bus_id)
          ?.launch_cost_usd ?? 0;
      launchBtn.disabled = s.cash_usd < cost;
    }

    for (const btn of document.querySelectorAll<HTMLButtonElement>(
      ".btn-tech",
    )) {
      const tech = this.content.tech_tree.find(
        (t) => t.id === btn.dataset.tech,
      );
      if (!tech) continue;
      const locked = tech.requires.some((r) => !s.unlocked_tech.includes(r));
      btn.disabled = locked || s.cash_usd < tech.cost_usd;
    }

    const downlinkBtn = document.getElementById(
      "btn-downlink",
    ) as HTMLButtonElement | null;
    if (downlinkBtn && craft) {
      downlinkBtn.textContent = craft.downlink_active
        ? "Stop Downlink"
        : "Start Downlink";
      downlinkBtn.className = craft.downlink_active ? "danger" : "primary";
    }

    const hohmann1 = document.getElementById(
      "btn-hohmann-1",
    ) as HTMLButtonElement | null;
    if (hohmann1) hohmann1.disabled = !craft?.at_apsis;
    const hohmann2 = document.getElementById(
      "btn-hohmann-2",
    ) as HTMLButtonElement | null;
    if (hohmann2) hohmann2.disabled = !craft?.at_apsis;

    // Live countdown to the burn apsis (rendered HTML is structure-keyed,
    // so this span is refreshed here every frame instead).
    const apsisEta = document.getElementById("apsis-eta");
    if (apsisEta && craft) {
      const raising = craft.pending_hohmann
        ? craft.pending_hohmann.plan.raising
        : (craft.hohmann_preview?.raising ?? true);
      // Burn 1 fires at perigee when raising; burn 2 at apogee (and vice versa).
      const burn2 = craft.pending_hohmann != null;
      const wantPerigee = raising !== burn2;
      const eta = wantPerigee
        ? craft.seconds_to_perigee
        : craft.seconds_to_apogee;
      apsisEta.textContent =
        craft.at_apsis || eta == null
          ? ""
          : `· apsis in ~${formatDuration(eta)}`;
    }
  }

  private bindActionDelegation(): void {
    this.actionEl.addEventListener("click", (e) => {
      const target = (e.target as HTMLElement).closest("button");
      if (!target || target.disabled) return;
      const s = this.currentSnapshot;
      if (!s) return;

      if (target.classList.contains("btn-tech")) {
        this.callbacks.onPurchaseTech(target.dataset.tech!);
        return;
      }
      if (target.classList.contains("btn-contract")) {
        this.callbacks.onAcceptContract(target.dataset.contract!);
        return;
      }
      if (target.classList.contains("accel-btn")) {
        this.callbacks.onTimeAccel(Number(target.dataset.accel));
        return;
      }
      if (target.id === "btn-launch") {
        this.callbacks.onLaunch();
        return;
      }
      if (target.id === "btn-decommission") {
        const busId = s.spacecraft?.bus_id;
        if (busId) this.callbacks.onDecommission(this.salvageUsd(busId));
        return;
      }
      if (target.id === "btn-downlink") {
        this.callbacks.onToggleDownlink(!s.spacecraft?.downlink_active);
        return;
      }
      if (
        target.id === "btn-queue-clear" ||
        target.id === "btn-queue-safe" ||
        target.id === "btn-queue-reboot"
      ) {
        const stationSel = document.getElementById(
          "downlink-station",
        ) as HTMLSelectElement | null;
        const station =
          stationSel?.value ??
          s.spacecraft?.downlink_station_id ??
          s.ground_stations[0]?.id;
        if (!station) return;
        const cmd =
          target.id === "btn-queue-clear"
            ? "clear_anomaly"
            : target.id === "btn-queue-safe"
              ? "safe_mode"
              : "reboot_comms";
        this.callbacks.onQueueCommand(cmd, station);
        return;
      }
      if (target.id === "btn-prograde") {
        this.callbacks.onProgradeBurn(0.01);
        return;
      }
      if (target.id === "btn-retrograde") {
        this.callbacks.onRetrogradeBurn(0.01);
        return;
      }
      if (target.id === "btn-hohmann-1") {
        this.callbacks.onStartHohmann();
        return;
      }
      if (target.id === "btn-hohmann-2") {
        this.callbacks.onCompleteHohmann();
        return;
      }
      if (target.classList.contains("btn-unlock-program")) {
        const programId = target.dataset.program;
        if (programId) {
          this.callbacks.onUnlockProgram(programId);
        }
        return;
      }
      if (target.id === "btn-begin-transfer") {
        this.callbacks.onBeginTransfer();
        return;
      }
      if (target.id === "btn-warp-window" || target.id === "btn-warp-arrival") {
        const warpTo = Number(target.dataset.warp);
        if (Number.isFinite(warpTo)) {
          this.callbacks.onWarpTo(warpTo);
        }
        return;
      }
      if (target.id === "btn-load-scenario") {
        const sel = document.getElementById(
          "scenario-select",
        ) as HTMLSelectElement;
        this.callbacks.onLoadScenario(sel.value);
        return;
      }
      if (target.id === "btn-help") {
        this.callbacks.onShowHelp();
        return;
      }
      if (target.id === "btn-export-save") {
        this.callbacks.onExportSave();
        return;
      }
      if (target.id === "btn-import-save") {
        this.callbacks.onImportSave();
      }
    });

    this.actionEl.addEventListener("input", (e) => {
      const target = e.target as HTMLElement;
      if (target.id === "altitude" || target.id === "inclination") {
        const alt = document.getElementById(
          "altitude",
        ) as HTMLInputElement | null;
        const inc = document.getElementById(
          "inclination",
        ) as HTMLInputElement | null;
        if (alt && inc) {
          document.getElementById("altitude-val")!.textContent =
            `${alt.value} km`;
          document.getElementById("inclination-val")!.textContent =
            `${inc.value}°`;
          this.callbacks.onSetOrbit(Number(alt.value), Number(inc.value));
        }
        return;
      }
      if (target.id === "maneuver-alt") {
        const val = Number((target as HTMLInputElement).value);
        document.getElementById("maneuver-alt-val")!.textContent = `${val} km`;
        this.callbacks.onSetManeuverTarget(val);
      }
    });

    this.actionEl.addEventListener("change", (e) => {
      const target = e.target as HTMLElement;
      if (target.id === "bus-select") {
        this.callbacks.onSelectBus((target as HTMLSelectElement).value);
        return;
      }
      if (target.id === "game-mode") {
        this.callbacks.onSetGameMode((target as HTMLSelectElement).value);
        return;
      }
      if (target.id === "downlink-station") {
        this.callbacks.onSelectDownlinkStation(
          (target as HTMLSelectElement).value,
        );
        return;
      }
      if (target.id === "payload-mode") {
        this.callbacks.onSetPayloadMode((target as HTMLSelectElement).value);
        return;
      }
      if (target.id === "auto-downlink") {
        this.callbacks.onAutoDownlinkChange(
          (target as HTMLInputElement).checked,
        );
        return;
      }
      if (target.id === "show-tle") {
        this.callbacks.onShowTleRefsChange(
          (target as HTMLInputElement).checked,
        );
        return;
      }
      if (target.id === "show-legend") {
        this.callbacks.onShowLegendChange((target as HTMLInputElement).checked);
        return;
      }
      if (target.id === "transfer-dest") {
        const dest = (target as unknown as HTMLSelectElement).value;
        if (dest) {
          const alt = Number(
            (document.getElementById("transfer-alt") as HTMLInputElement)
              .value,
          );
          const inc = Number(
            (document.getElementById("transfer-inc") as HTMLInputElement)
              .value,
          );
          this.callbacks.onSetTransferTarget(dest, alt, inc);
        }
        return;
      }
      if (target.id === "transfer-alt" || target.id === "transfer-inc") {
        const destSel = document.getElementById(
          "transfer-dest",
        ) as HTMLSelectElement | null;
        if (destSel?.value) {
          const alt = Number(
            (document.getElementById("transfer-alt") as HTMLInputElement)
              .value,
          );
          const inc = Number(
            (document.getElementById("transfer-inc") as HTMLInputElement)
              .value,
          );
          this.callbacks.onSetTransferTarget(destSel.value, alt, inc);
        }
        return;
      }
      if (target.id === "scenario-select") {
        const sc = this.content.scenarios?.find(
          (s) => s.id === (target as HTMLSelectElement).value,
        );
        const desc = document.getElementById("scenario-desc");
        if (desc && sc) desc.textContent = sc.description;
      }
    });
  }
}

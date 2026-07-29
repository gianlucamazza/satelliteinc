import type {
  ActiveContractSnapshot,
  GameContent,
  Snapshot,
  SpacecraftSnapshot,
} from "../types.ts";
import {
  bankruptcyRunwayDays,
  commandLabel,
  contractDaysLeft,
  contractProgress,
  dataFlowStatus,
  downlinkRateLabel,
  formatDuration,
  isDownlinkLinkLive,
  nextPassStation,
  orbitRequirementText,
  payloadModeLabel,
} from "./format.ts";
import { releaseFocus, trapFocus } from "./focus-trap.ts";
import { esc, gauge } from "./html.ts";
import { ToastQueue, type ToastKind } from "./toast.ts";
import { renderCompanyTab } from "./tabs/company.ts";
import { renderOpsTab } from "./tabs/ops.ts";
import { renderOrbitTab } from "./tabs/orbit.ts";
import { renderTransferTab } from "./tabs/transfer.ts";

export type HudTab = "mission" | "ops" | "orbit" | "transfer" | "company";

const TAB_KBD: Record<HudTab, string> = {
  mission: "1",
  ops: "2",
  orbit: "3",
  company: "4",
  transfer: "5",
};

/** Discrete time-acceleration presets (0 = paused). */
export const ACCEL_PRESETS = [0, 1, 10, 100, 1000, 10000] as const;

export interface PanelCallbacks {
  onSelectBus: (busId: string) => void;
  onSetOrbit: (altitude: number, inclination: number) => void;
  onAcceptContract: (contractId: string, craftId?: number) => void;
  onRushBid: (contractId: string, craftId?: number) => void;
  onAssignContract: (contractId: string, craftId: number | null) => void;
  onLaunch: () => void;
  onToggleDownlink: (active: boolean) => void;
  onTimeAccel: (multiplier: number) => void;
  onProgradeBurn: (deltaV: number) => void;
  onRetrogradeBurn: (deltaV: number) => void;
  onSetManeuverTarget: (altitude: number) => void;
  onStartHohmann: () => void;
  onCompleteHohmann: () => void;
  onPlaneChange: (deltaInclinationDeg: number) => void;
  onSelectDownlinkStation: (stationId: string) => void;
  onSetPayloadMode: (mode: string) => void;
  onQueueCommand: (command: string, stationId: string) => void;
  onPurchaseTech: (techId: string) => void;
  onSetGameMode: (mode: string) => void;
  onDecommission: (salvageUsd: number) => void;
  onRenameCraft: (craftId: number, name: string) => void;
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
  onAutoPauseArrivalChange: (active: boolean) => void;
  onAutoPauseAnomalyChange: (active: boolean) => void;
  onShowTleRefsChange: (active: boolean) => void;
  onShowLegendChange: (active: boolean) => void;
  getShowLegend: () => boolean;
  getAutoPauseArrival: () => boolean;
  getAutoPauseAnomaly: () => boolean;
  onLoadScenario: (scenarioId: string) => void;
  onExportSave: () => void;
  onImportSave: () => void;
  getAutoDownlink: () => boolean;
  getShowTleRefs: () => boolean;
}

export class HudPanel {
  private readonly statusEl: HTMLElement;
  private readonly priorityEl: HTMLElement;
  private readonly timeToolbarEl: HTMLElement;
  private readonly actionEl: HTMLElement;
  private readonly eventEl: HTMLElement;
  private readonly phaseEl: HTMLElement;
  private readonly fleetEl: HTMLElement;
  private readonly tabBar: HTMLElement;
  private timeToolbarKey = "";
  private readonly toasts: ToastQueue;
  private currentSnapshot: Snapshot | null = null;
  private actionStructureKey = "";
  private statusFingerprint = "";
  private activeTab: HudTab = "mission";
  private transfersUnlocked = false;
  /** Last craft phase used for tab set — refresh tabs when it changes. */
  private tabsPhaseKey = "";
  private selectedCraftId = 0;
  private lastContractToast = "";
  private victoryDismissed = false;
  private lastEndScreenKey = "";
  private endscreenUntrap: (() => void) | null = null;
  private readonly tabAttention = new Set<HudTab>();
  /** Marketplace filter: all | available | contested | body id */
  private marketFilter = "all";

  constructor(
    private readonly content: GameContent,
    private readonly callbacks: PanelCallbacks,
  ) {
    this.statusEl = document.getElementById("status-panel")!;
    this.priorityEl = document.getElementById("priority-strip")!;
    this.timeToolbarEl = document.getElementById("time-toolbar")!;
    this.actionEl = document.getElementById("action-panel")!;
    this.eventEl = document.getElementById("event-log")!;
    this.phaseEl = document.getElementById("phase-label")!;
    this.fleetEl = document.getElementById("fleet-strip")!;
    this.fleetEl.addEventListener("click", (e) => {
      const chip = (e.target as HTMLElement).closest<HTMLElement>(
        "[data-craft]",
      );
      if (!chip?.dataset.craft) return;
      const id = Number(chip.dataset.craft);
      if (Number.isFinite(id) && id !== this.selectedCraftId) {
        this.selectedCraftId = id;
        this.statusFingerprint = "";
        this.actionStructureKey = "";
        if (this.currentSnapshot) this.update(this.currentSnapshot);
      }
    });
    this.tabBar = document.getElementById("tab-bar")!;
    this.toasts = new ToastQueue(document.getElementById("toast")!);
    this.bindActionDelegation();
    this.bindTimeToolbar();
    this.bindTabs();
    this.bindEndScreen();
    this.actionEl.addEventListener("change", (e) => {
      const t = e.target as HTMLElement;
      if (t.id === "market-filter" && t instanceof HTMLSelectElement) {
        this.marketFilter = t.value;
        this.actionStructureKey = "";
        if (this.currentSnapshot) this.renderActions(this.currentSnapshot);
      }
    });
    this.renderTabs();
  }

  /** Placeholder shimmer until the first snapshot arrives. */
  showSkeleton(): void {
    this.priorityEl.innerHTML = Array(5)
      .fill('<span class="ps-item skeleton-ps"><label>…</label><strong>—</strong></span>')
      .join("");
    this.statusEl.innerHTML = `<div class="skeleton-grid">${Array(4)
      .fill('<div class="skeleton-stat"></div>')
      .join("")}</div>`;
    this.actionEl.innerHTML =
      '<div class="skeleton-block"></div><div class="skeleton-block short"></div>';
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

  /** Craft the HUD is focused on (falls back to the first). */
  selectedCraft(s: Snapshot): SpacecraftSnapshot | null {
    return (
      s.spacecraft.find((c) => c.id === this.selectedCraftId) ??
      s.spacecraft[0] ??
      null
    );
  }

  getSelectedCraftId(): number {
    return this.currentSnapshot
      ? (this.selectedCraft(this.currentSnapshot)?.id ?? 0)
      : 0;
  }

  /** Focus a fleet craft (e.g. from system-map “Focus craft”). */
  setSelectedCraftId(id: number): void {
    if (!Number.isFinite(id) || id === this.selectedCraftId) return;
    this.selectedCraftId = id;
    this.statusFingerprint = "";
    this.actionStructureKey = "";
    if (this.currentSnapshot) this.update(this.currentSnapshot);
  }

  /** Contract the selected craft is serving (else the first active one). */
  private contractFor(
    s: Snapshot,
    craft: SpacecraftSnapshot | null,
  ): ActiveContractSnapshot | null {
    return (
      (craft &&
        s.active_contracts.find((c) => c.assigned_craft_id === craft.id)) ??
      s.active_contracts[0] ??
      null
    );
  }

  private canAcceptMoreContracts(s: Snapshot): boolean {
    return s.active_contracts.length < s.max_flights;
  }

  private craftsAvailableForContract(s: Snapshot): SpacecraftSnapshot[] {
    const assigned = new Set(
      s.active_contracts
        .map((c) => c.assigned_craft_id)
        .filter((id): id is number => id != null),
    );
    return s.spacecraft.filter((c) => !assigned.has(c.id));
  }

  private craftHasContract(s: Snapshot, craftId: number): boolean {
    return s.active_contracts.some((c) => c.assigned_craft_id === craftId);
  }

  private resolveAcceptCraftId(s: Snapshot): number | undefined {
    const available = this.craftsAvailableForContract(s);
    if (available.length === 0) return undefined;
    if (available.length === 1) return available[0]!.id;
    const sel = document.getElementById(
      "accept-craft-select",
    ) as HTMLSelectElement | null;
    const id = sel ? Number(sel.value) : this.selectedCraftId;
    return available.some((c) => c.id === id) ? id : available[0]!.id;
  }

  private renderMarketplaceOffers(
    s: Snapshot,
    compact = false,
  ): string[] {
    if (!this.canAcceptMoreContracts(s)) return [];
    const availableCrafts = this.craftsAvailableForContract(s);
    const parts: string[] = [];
    const slotHint = `<span class="hint">${s.active_contracts.length}/${s.max_flights} contract slots · ${s.active_flights}/${s.max_flights} flights active</span>`;
    parts.push(
      `<div class="field"><label>Contract marketplace</label>${slotHint}${
        availableCrafts.length > 1
          ? `<select id="accept-craft-select" title="Craft that will serve the contract">${availableCrafts
              .map((c) => {
                const body =
                  s.bodies.find((b) => b.id === c.body_id)?.name ?? c.body_id;
                return `<option value="${c.id}" ${c.id === this.selectedCraftId ? "selected" : ""}>${esc(c.name)} (${esc(body)})</option>`;
              })
              .join("")}</select>`
          : availableCrafts.length === 1
            ? `<p class="hint">Assigning to <strong>${esc(availableCrafts[0]!.name)}</strong></p>`
            : `<p class="hint warn">No free craft — add a bus in design or decommission one.</p>`
      }</div>`,
    );
    if (availableCrafts.length === 0) return parts;

    if (s.market_enabled && (s.competitors?.length ?? 0) > 0) {
      const rivalStrip = (s.competitors ?? [])
        .map(
          (r) =>
            `<span class="chip rival" title="Focus: ${esc(r.focus_bodies.join(", ") || "any")}">${esc(r.name)} · ${r.contracts_won} wins</span>`,
        )
        .join(" ");
      parts.push(
        `<div class="market-pressure"><label>Market pressure</label><div class="rival-strip">${rivalStrip}</div></div>`,
      );
    }

    const bodies = [
      ...new Set(s.marketplace.map((o) => o.body_id).filter(Boolean)),
    ].sort();
    parts.push(`
      <div class="field market-filter-row">
        <label for="market-filter">Filter</label>
        <select id="market-filter" title="Filter marketplace offers">
          <option value="all" ${this.marketFilter === "all" ? "selected" : ""}>All</option>
          <option value="available" ${this.marketFilter === "available" ? "selected" : ""}>Available</option>
          <option value="contested" ${this.marketFilter === "contested" ? "selected" : ""}>Contested</option>
          ${bodies
            .map(
              (b) =>
                `<option value="body:${esc(b)}" ${this.marketFilter === `body:${b}` ? "selected" : ""}>${esc(b)}</option>`,
            )
            .join("")}
        </select>
      </div>
    `);

    const filtered = s.marketplace.filter((offer) => {
      if (this.marketFilter === "available") return offer.available;
      if (this.marketFilter === "contested") return Boolean(offer.contested_by);
      if (this.marketFilter.startsWith("body:")) {
        return offer.body_id === this.marketFilter.slice(5);
      }
      return true;
    });

    for (const offer of filtered) {
      const repPct = Math.min(
        100,
        (s.reputation / Math.max(offer.reputation_required, 1)) * 100,
      );
      const orbitReq = orbitRequirementText(offer);
      const contested = Boolean(offer.contested_by);
      const claimed = Boolean(offer.claimed_by);
      const contestEta =
        contested && offer.contest_ends_sim_seconds != null
          ? Math.max(0, offer.contest_ends_sim_seconds - s.sim_seconds)
          : null;
      const contestChip = contested
        ? `<span class="chip contest">CONTESTED by ${esc(offer.contested_by!)}${contestEta != null ? ` · ${formatDuration(contestEta)}` : ""}</span>`
        : "";
      const claimedChip = claimed
        ? `<span class="chip claimed">CLAIMED by ${esc(offer.claimed_by!)}</span>`
        : "";
      const rushBtn =
        contested && offer.available && (offer.rush_bid_premium_usd ?? 0) > 0
          ? `<button class="btn-rush-bid" data-contract="${esc(offer.template_id)}" title="Pay premium to snatch this contested offer">Rush bid +$${Math.round(offer.rush_bid_premium_usd!).toLocaleString()}</button>`
          : "";
      if (compact) {
        parts.push(`
          <button class="btn-contract ${contested ? "contested" : ""}" data-contract="${esc(offer.template_id)}" ${offer.available ? "" : "disabled"}>
            ${esc(offer.name)} — $${offer.payout_usd.toLocaleString()}${offer.repeat ? " (repeat)" : ""}${contested ? " ⚡" : ""}
          </button>
          ${contestChip}${claimedChip}
          ${rushBtn}
          ${orbitReq ? `<p class="hint">${orbitReq}</p>` : ""}
        `);
      } else {
        parts.push(`
          <div class="offer-card ${offer.available ? "" : "locked"} ${contested ? "contested" : ""} ${claimed ? "claimed" : ""}">
            <button class="btn-contract" data-contract="${esc(offer.template_id)}" ${offer.available ? "" : "disabled"}>
              ${esc(offer.name)}
            </button>
            ${rushBtn}
            ${offer.repeat ? `<span class="chip repeat">REPEAT — ×0.6 payout, no rep</span>` : ""}
            ${contestChip}${claimedChip}
            <p class="offer-meta">$${offer.payout_usd.toLocaleString()} · ${offer.downlink_mb} MB · ${offer.deadline_days} d · rep ${offer.reputation_required}${offer.min_distinct_stations > 0 ? ` · ${offer.min_distinct_stations} stations` : ""}${orbitReq ? ` · ${orbitReq}` : ""}</p>
            <p class="offer-desc">${esc(offer.description)}</p>
            ${!offer.available && !claimed ? `<div class="progress-bar thin"><div class="progress-fill" style="width:${repPct}%"></div></div>` : ""}
          </div>
        `);
      }
    }
    return parts;
  }

  private renderActiveContractsPanel(s: Snapshot): string[] {
    if (!s.active_contracts.length) return [];
    const parts = [`<div class="field"><label>Active contracts</label></div>`];
    for (const contract of s.active_contracts) {
      const assigned = contract.assigned_craft_id;
      const craftName = assigned
        ? (s.spacecraft.find((c) => c.id === assigned)?.name ??
          `Craft ${assigned}`)
        : "Unassigned";
      const craftOptions = s.spacecraft
        .map((c) => {
          const taken = s.active_contracts.find(
            (k) =>
              k.assigned_craft_id === c.id &&
              k.template_id !== contract.template_id,
          );
          if (taken) return "";
          return `<option value="${c.id}" ${assigned === c.id ? "selected" : ""}>${esc(c.name)}</option>`;
        })
        .join("");
      parts.push(`
        <div class="contract-card compact">
          <div class="contract-head">
            <span>${esc(contract.name)}</span>
            <span class="deadline${contractDaysLeft(contract, s.sim_seconds) < 2 ? " urgent" : ""}">${contractDaysLeft(contract, s.sim_seconds).toFixed(1)} d</span>
          </div>
          <div class="progress-bar thin"><div class="progress-fill" style="width:${contractProgress(contract)}%"></div></div>
          <div class="progress-label">${contract.downlink_mb_done.toFixed(0)} / ${contract.downlink_mb_required.toFixed(0)} MB · ${esc(craftName)}</div>
          <label class="assign-row">Serve with
            <select data-assign-contract="${esc(contract.template_id)}" title="Assign or reassign craft">
              <option value="" ${assigned == null ? "selected" : ""}>Unassigned</option>
              ${craftOptions}
            </select>
          </label>
        </div>
      `);
    }
    return parts;
  }

  private renderFleetStrip(s: Snapshot): void {
    if (s.spacecraft.length <= 1) {
      this.fleetEl.innerHTML = "";
      return;
    }
    const selected = this.selectedCraft(s);
    this.fleetEl.innerHTML = s.spacecraft
      .map((c) => {
        const body = s.bodies.find((b) => b.id === c.body_id);
        const contract = s.active_contracts.find(
          (k) => k.assigned_craft_id === c.id,
        );
        const progress = contract
          ? Math.round(
              (contract.downlink_mb_done / contract.downlink_mb_required) * 100,
            )
          : null;
        const phaseLabel =
          c.phase === "Design"
            ? "Design"
            : c.phase === "Transfer"
              ? "Transit"
              : "Orbit";
        const ariaLabel = `${c.name}, ${phaseLabel}, ${body?.name ?? c.body_id}${c.anomaly_label ? `, ${c.anomaly_label}` : ""}`;
        const alert = c.anomaly_label
          ? `<span class="chip-alert" aria-hidden="true">!</span>`
          : "";
        return `<button type="button" class="fleet-chip${selected?.id === c.id ? " selected" : ""}" data-craft="${c.id}" aria-label="${esc(ariaLabel)}" title="${esc(c.name)} — ${esc(body?.name ?? c.body_id)}${c.anomaly_label ? ` · ${esc(c.anomaly_label)}` : ""}">
          <span class="chip-phase">${phaseLabel}</span> ${esc(c.name)}
          <span class="chip-body">${esc(body?.name ?? c.body_id)}</span>
          ${progress != null ? `<span class="chip-progress" aria-hidden="true"><div style="width:${progress}%"></div></span>` : ""}
          ${alert}
        </button>`;
      })
      .join("");
  }

  update(snapshot: Snapshot): void {
    this.currentSnapshot = snapshot;
    this.syncChromeLayout(snapshot);
    this.renderFleetStrip(snapshot);
    const craft = this.selectedCraft(snapshot);
    const phaseKey = `${craft?.phase ?? ""}|${snapshot.transfers_unlocked}|${snapshot.spacecraft.length}`;
    if (
      snapshot.transfers_unlocked !== this.transfersUnlocked ||
      phaseKey !== this.tabsPhaseKey
    ) {
      this.transfersUnlocked = snapshot.transfers_unlocked;
      this.tabsPhaseKey = phaseKey;
      if (!this.transfersUnlocked && this.activeTab === "transfer") {
        this.activeTab = "mission";
      }
      // Redirect away from tabs that no longer apply.
      if (
        craft?.phase === "Design" &&
        (this.activeTab === "ops" ||
          this.activeTab === "orbit" ||
          this.activeTab === "transfer")
      ) {
        this.activeTab = "mission";
      } else if (craft?.phase === "Transfer" && this.activeTab === "ops") {
        this.activeTab = this.transfersUnlocked ? "transfer" : "mission";
      }
      this.renderTabs();
    }
    const bodyName =
      craft && craft.body_id !== "earth"
        ? snapshot.bodies.find((b) => b.id === craft.body_id)?.name
        : null;
    const programsDone = snapshot.programs.filter((p) => p.completed).length;
    const programsTotal = snapshot.programs.length;
    const slots = `${snapshot.active_flights}/${snapshot.max_flights}`;
    const programProgress =
      programsTotal > 0 && snapshot.game_mode === "campaign"
        ? ` · P${programsDone}/${programsTotal}`
        : "";
    this.phaseEl.textContent = `${snapshot.phase.replace(/_/g, " ")}${bodyName ? ` · ${bodyName}` : ""} · ${slots}${programProgress}`;
    this.renderStatus(snapshot);
    this.renderTimeToolbar(snapshot);
    this.renderActions(snapshot);
    this.updateActionDynamicState(snapshot);
    this.maybeToastContract(snapshot);
    this.renderEndScreen(snapshot);
  }

  /**
   * Show/hide chrome sections based on fleet size, log activity, and phase.
   * Keeps the HUD from reserving empty vertical space.
   */
  private syncChromeLayout(s: Snapshot): void {
    const multiFleet = s.spacecraft.length > 1;
    this.fleetEl.classList.toggle("hud-section-hidden", !multiFleet);
    this.fleetEl.setAttribute("aria-hidden", multiFleet ? "false" : "true");

    const hasLog = this.eventEl.childElementCount > 0;
    this.eventEl.classList.toggle("hud-section-hidden", !hasLog);
    this.eventEl.setAttribute("aria-hidden", hasLog ? "false" : "true");
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
    this.eventEl.classList.remove("hud-section-hidden");
    this.eventEl.setAttribute("aria-hidden", "false");
  }

  logError(message: string): void {
    this.logEvent(message, "error");
    this.showToast(message, "error");
  }

  showToast(message: string, kind: ToastKind = "info"): void {
    this.toasts.show(message, kind);
  }

  private maybeToastContract(s: Snapshot): void {
    const c = this.contractFor(s, this.selectedCraft(s));
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
        this.endscreenUntrap?.();
        this.endscreenUntrap = null;
        releaseFocus();
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
      this.endscreenUntrap?.();
      this.endscreenUntrap = null;
      releaseFocus();
      el.classList.add("hidden");
      return;
    }
    if (s.phase === "victory" && this.victoryDismissed) {
      this.endscreenUntrap?.();
      this.endscreenUntrap = null;
      releaseFocus();
      el.classList.add("hidden");
      return;
    }

    const title = document.getElementById("endscreen-title")!;
    const body = document.getElementById("endscreen-body")!;
    const continueBtn = document.getElementById("endscreen-continue")!;
    if (s.phase === "victory") {
      const done = s.programs.filter((p) => p.completed).map((p) => p.name);
      const pr = s.prestige;
      const rankLine = pr
        ? ` Prestige: ${pr.rank_label} (score ${pr.score}).`
        : "";
      title.textContent = "Victory!";
      body.textContent =
        (done.length > 0
          ? `All programs completed (${done.join(", ")}) — Satellite Inc is an interplanetary agency.`
          : "All programs completed — Satellite Inc is an interplanetary agency.") +
        rankLine +
        " Keep flying or start over.";
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
    this.endscreenUntrap?.();
    this.endscreenUntrap = trapFocus(el);
    document.getElementById("endscreen-restart")?.focus();
  }

  private bindTabs(): void {
    this.tabBar.addEventListener("click", (e) => {
      const btn = (e.target as HTMLElement).closest<HTMLButtonElement>(
        "[data-tab]",
      );
      if (!btn) return;
      this.setTab(btn.dataset.tab as HudTab);
    });
    this.tabBar.addEventListener("keydown", (e) => {
      if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
      const tabs = Array.from(
        this.tabBar.querySelectorAll<HTMLButtonElement>('[role="tab"]'),
      );
      const idx = tabs.findIndex((t) => t === document.activeElement);
      if (idx === -1) return;
      e.preventDefault();
      const next =
        e.key === "ArrowRight"
          ? tabs[(idx + 1) % tabs.length]!
          : tabs[(idx - 1 + tabs.length) % tabs.length]!;
      this.setTab(next.dataset.tab as HudTab);
      next.focus();
    });
  }

  private renderTabs(): void {
    const craft = this.currentSnapshot
      ? this.selectedCraft(this.currentSnapshot)
      : null;
    const flying =
      craft?.phase === "OnOrbit" || craft?.phase === "Transfer";
    // Only show tabs that matter for the current phase (Mission + Company always).
    const tabs: { id: HudTab; label: string }[] = [
      { id: "mission", label: "Mission" },
      ...(flying ? [{ id: "ops" as HudTab, label: "Ops" }] : []),
      ...(craft?.phase === "OnOrbit"
        ? [{ id: "orbit" as HudTab, label: "Orbit" }]
        : []),
      ...(this.transfersUnlocked && craft?.phase !== "Design"
        ? [{ id: "transfer" as HudTab, label: "Transfer" }]
        : []),
      { id: "company", label: "Company" },
    ];
    // If active tab was removed, fall back to Mission.
    if (!tabs.some((t) => t.id === this.activeTab)) {
      this.activeTab = "mission";
    }
    this.tabBar.setAttribute("role", "tablist");
    this.tabBar.innerHTML = tabs
      .map((t) => {
        const active = this.activeTab === t.id;
        return `<button type="button" role="tab" id="tab-${t.id}" class="tab${active ? " active" : ""}${this.tabAttention.has(t.id) ? " attention" : ""}" data-tab="${t.id}" aria-selected="${active}" aria-controls="action-panel" tabindex="${active ? "0" : "-1"}">${t.label} <span class="tab-kbd" aria-hidden="true">${TAB_KBD[t.id]}</span></button>`;
      })
      .join("");
    this.tabBar.style.setProperty(
      "--tab-count",
      String(Math.min(tabs.length, 5)),
    );
  }

  private computeStatusFingerprint(s: Snapshot): string {
    const c = this.selectedCraft(s);
    const contract = this.contractFor(s, this.selectedCraft(s));
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
      this.contractFor(s, c)?.orbit_ok ?? "",
      s.outcome ?? "",
      s.ops_cost_usd_per_day.toFixed(0),
      c ? Math.round((c.power_wh / c.battery_wh_capacity) * 50) : "",
      c ? Math.round((c.buffer_mb / c.buffer_capacity_mb) * 50) : "",
      c?.propellant_kg.toFixed(2) ?? "",
      c?.solar_flux.toFixed(2) ?? "",
      c?.elements
        ? `${c.elements.inclination_deg.toFixed(1)}:${c.elements.raan_deg.toFixed(0)}`
        : "",
      dataFlowStatus(s, c, this.contractFor(s, c))?.text ?? "",
      c ? isDownlinkLinkLive(c) : "",
      c?.downlink_station_id ?? "",
      Math.round(bankruptcyRunwayDays(s) ?? -1),
      (c?.station_status ?? [])
        .map(
          (st) =>
            // Coarse ETA bucket so the "next pass" countdown refreshes.
            `${st.station_id}:${st.in_pass}:${st.elevation_deg.toFixed(0)}:${Math.round((st.next_pass_in_seconds ?? -1) / 15)}`,
        )
        .join(","),
      s.ground_stations.map((g) => `${g.id}:${g.unlocked ? 1 : 0}`).join(","),
      s.active_world_event
        ? `${s.active_world_event.event_id}:${Math.round((s.active_world_event.ends_at_sim_seconds - s.sim_seconds) / 30)}`
        : "",
      s.marketplace.filter((o) => o.contested_by).length,
      s.next_event_sim_seconds == null
        ? ""
        : Math.round((s.next_event_sim_seconds - s.sim_seconds) / 30),
    ].join("|");
  }

  private renderStatus(s: Snapshot): void {
    const fp = this.computeStatusFingerprint(s);
    if (fp === this.statusFingerprint) return;
    this.statusFingerprint = fp;

    const craft = this.selectedCraft(s);
    const contract = this.contractFor(s, craft);
    const nextPass = nextPassStation(s, craft);
    const phase = craft?.phase;
    const onOrbit = phase === "OnOrbit";
    const inTransfer = phase === "Transfer";
    const flying = onOrbit || inTransfer;
    const craftBodyId = craft?.body_id ?? "earth";
    const selectedDish = craft?.downlink_station_id ?? null;

    // Stations: only rows that are in pass or next soon (hide idle clutter).
    // Selected downlink dish is always listed when present.
    let stationRows = "";
    if (craft && onOrbit) {
      const ranked = craft.station_status
        .map((st, i) => {
          const selected = st.station_id === selectedDish;
          const priority = st.in_pass
            ? 0
            : selected
              ? 1
              : st.next_pass_in_seconds != null && st.next_pass_in_seconds < 3600
                ? 2
                : 9;
          return { st, selected, priority, i };
        })
        .filter(
          (row) =>
            row.priority < 9 ||
            row.st.in_pass ||
            (row.st.next_pass_in_seconds != null &&
              row.st.next_pass_in_seconds < 3600),
        )
        .sort((a, b) => a.priority - b.priority || a.i - b.i)
        .slice(0, 4);

      stationRows = ranked
        .map(({ st, selected }) => {
          const info = s.ground_stations.find((g) => g.id === st.station_id);
          const name = info?.name ?? st.station_id;
          const isDsnLink =
            (info?.dsn ?? false) && info?.body_id !== craftBodyId;
          const passEta =
            st.in_pass || st.next_pass_in_seconds == null
              ? ""
              : `<span class="stat-sub">next ${formatDuration(st.next_pass_in_seconds)}</span>`;
          const idle = isDsnLink
            ? "NO LINK"
            : st.elevation_deg > -89
              ? `${st.elevation_deg.toFixed(0)}°`
              : "—";
          const active = isDsnLink ? "LINK" : "PASS";
          const mark = selected ? " ★" : "";
          return `<div class="stat${selected ? " station-selected" : ""}"><label>${esc(name)}${mark}</label><strong class="${st.in_pass ? "pass-active" : ""}">${st.in_pass ? active : idle}</strong><span class="stat-sub">${st.bandwidth_mbps.toFixed(1)} Mbps</span>${passEta}</div>`;
        })
        .join("");
    }

    const lightDelayLine =
      craft && flying && craft.light_delay_seconds > 1.0
        ? `<div class="stat"><label>OWLT</label><strong>${craft.light_delay_seconds < 120 ? `${craft.light_delay_seconds.toFixed(0)} s` : `${(craft.light_delay_seconds / 60).toFixed(1)} min`}</strong></div>`
        : "";

    const orbitReq = contract ? orbitRequirementText(contract) : "";
    const orbitSpecLine =
      contract && orbitReq
        ? contract.orbit_ok === false
          ? `<div class="orbit-spec bad">Orbit OUT OF SPEC — needs ${orbitReq}</div>`
          : `<div class="orbit-spec ok">Orbit OK (${orbitReq})</div>`
        : "";

    // No "pick a contract" filler in design — Mission tab already handles it.
    const contractBlock = contract
      ? `<div class="contract-card">
          <div class="contract-head">
            <span>${esc(contract.name)}</span>
            <span class="deadline${contractDaysLeft(contract, s.sim_seconds) < 2 ? " urgent" : ""}">${contractDaysLeft(contract, s.sim_seconds).toFixed(1)} d</span>
          </div>
          ${orbitSpecLine}
          <div class="progress-bar"><div class="progress-fill" style="width:${contractProgress(contract)}%"></div></div>
          <div class="progress-label">${contract.downlink_mb_done.toFixed(0)} / ${contract.downlink_mb_required.toFixed(0)} MB</div>
        </div>`
      : "";

    const flow = dataFlowStatus(s, craft, contract);
    const flowLine =
      flow && onOrbit
        ? `<div class="orbit-spec ${flow.kind === "bad" ? "bad" : flow.kind === "ok" ? "ok" : "warn"}">${flow.text}</div>`
        : "";

    // Full craft telemetry only while flying; design stays minimal.
    let craftBlock = "";
    if (craft && flying) {
      const dlLive = isDownlinkLinkLive(craft);
      craftBlock = `<div class="stat-grid">
          <div class="stat"><label>Alt</label><strong>${craft.altitude_km.toFixed(0)} km</strong></div>
          ${gauge("Bat", craft.power_wh, craft.battery_wh_capacity, "Wh", { lowIsBad: true })}
          ${gauge("Buf", craft.buffer_mb, craft.buffer_capacity_mb, "MB", { highIsWarn: true })}
          ${onOrbit ? gauge("Prop", craft.propellant_kg, craft.propellant_capacity_kg, "kg", { lowIsBad: true }) : ""}
          ${onOrbit ? `<div class="stat"><label>Sun</label><strong class="${craft.in_sunlight ? "pass-active" : ""}">${craft.in_sunlight ? "YES" : "Ecl"}</strong></div>` : ""}
          ${lightDelayLine}
          ${onOrbit ? `<div class="stat"><label>DL</label><strong class="${craft.downlink_active && dlLive ? "pass-active" : ""}">${downlinkRateLabel(craft)}</strong></div>` : ""}
          ${onOrbit ? `<div class="stat"><label>Mode</label><strong>${payloadModeLabel(craft.payload_mode)}</strong></div>` : ""}
        </div>
        ${flowLine}
        ${craft.anomaly_label ? `<div class="alert warn">⚠ ${esc(craft.anomaly_label)}</div>` : ""}
        ${craft.command_queue.length ? `<ul class="queue-list">${craft.command_queue.map((q) => `<li>${esc(commandLabel(q.command))} → ${esc(q.station_id)}</li>`).join("")}</ul>` : ""}`;
    } else if (craft && phase === "Design") {
      craftBlock = `<p class="hint status-hint">${esc(craft.bus_name)} · design · ${craft.target_altitude_km.toFixed(0)} km / ${craft.target_inclination_deg.toFixed(0)}°</p>`;
    }

    const nextPassBlock =
      onOrbit && nextPass
        ? `<p class="hint pass-hint">Next pass: <strong>${esc(nextPass.name)}</strong>${nextPass.etaSeconds != null ? ` · ${formatDuration(nextPass.etaSeconds)}` : ""}</p>`
        : "";

    const runway = bankruptcyRunwayDays(s);
    const runwayDays =
      runway != null && (runway < 45 || s.cash_usd < 0) ? runway : null;
    const runwayBanner =
      runwayDays != null
        ? `<div class="alert danger">💸 ~${Math.max(0, runwayDays).toFixed(0)} d runway</div>`
        : "";

    const contestedCount = s.marketplace.filter((o) => o.contested_by).length;
    const nextEventEta =
      s.next_event_sim_seconds != null
        ? formatDuration(Math.max(0, s.next_event_sim_seconds - s.sim_seconds))
        : null;
    const wx = s.active_world_event;
    const wxEta = wx
      ? formatDuration(Math.max(0, wx.ends_at_sim_seconds - s.sim_seconds))
      : "";
    const weatherBanner = wx
      ? `<div class="alert warn weather-banner" title="${esc(wx.description)}">🌩 <strong>${esc(wx.name)}</strong> · ${wxEta}</div>`
      : "";

    // Global telemetry lives in the top bar (outside the left dock).
    const stripItems = [
      `<span class="ps-item"><label>Cash</label><strong class="${s.cash_usd < 0 ? "negative" : ""}">$${s.cash_usd.toLocaleString(undefined, { maximumFractionDigits: 0 })}</strong></span>`,
      `<span class="ps-item"><label>Rep</label><strong>${s.reputation.toFixed(0)}</strong></span>`,
      `<span class="ps-item"><label>Day</label><strong>${s.sim_days.toFixed(1)}</strong></span>`,
      `<span class="ps-item"><label>Spd</label><strong>${s.time_accel === 0 ? "❚❚" : `${s.time_accel.toFixed(0)}×`}</strong></span>`,
      `<span class="ps-item"><label>Slots</label><strong>${s.active_flights}/${s.max_flights}</strong></span>`,
    ];
    if (runwayDays != null) {
      stripItems.push(
        `<span class="ps-item"><label>Runway</label><strong class="warn-text">~${Math.max(0, runwayDays).toFixed(0)}d</strong></span>`,
      );
    }
    if (nextEventEta) {
      stripItems.push(
        `<span class="ps-item"><label>Next</label><strong>${nextEventEta}</strong></span>`,
      );
    }
    if (contestedCount > 0) {
      stripItems.push(
        `<span class="ps-item"><label>Bid</label><strong class="warn-text">${contestedCount}</strong></span>`,
      );
    }
    if (wx) {
      stripItems.push(
        `<span class="ps-item"><label>Wx</label><strong class="warn-text">${esc(wx.name)}</strong></span>`,
      );
    }
    if (s.ops_cost_usd_per_day > 0) {
      stripItems.push(
        `<span class="ps-item"><label>Ops</label><strong>$${(s.ops_cost_usd_per_day / 1000).toFixed(1)}k/d</strong></span>`,
      );
    }
    this.priorityEl.innerHTML = stripItems.join("");

    // Contextual dock status: alerts, contract, craft telemetry, stations.
    const statusHtml = [
      weatherBanner,
      runwayBanner,
      contractBlock,
      craftBlock,
      nextPassBlock,
      stationRows ? `<div class="stat-grid stations">${stationRows}</div>` : "",
    ]
      .filter(Boolean)
      .join("");
    this.statusEl.innerHTML = statusHtml;
    const hasStatus = statusHtml.length > 0;
    this.statusEl.classList.toggle("hud-section-hidden", !hasStatus);
    this.statusEl.setAttribute("aria-hidden", hasStatus ? "false" : "true");
  }

  /** Time-accel lives in the top bar; rebuild only when presets need a repaint. */
  private renderTimeToolbar(s: Snapshot): void {
    // Structure is fixed; only active state changes every tick via updateActionDynamicState.
    const key = "accel-presets";
    if (this.timeToolbarKey === key) return;
    this.timeToolbarKey = key;
    this.timeToolbarEl.innerHTML = ACCEL_PRESETS.map(
      (p) =>
        `<button type="button" class="accel-btn${s.time_accel === p ? " active" : ""}" data-accel="${p}" title="${p === 0 ? "Pause (Space)" : `${p}× speed`}">${p === 0 ? "⏸" : `${p}×`}</button>`,
    ).join("");
  }

  private bindTimeToolbar(): void {
    this.timeToolbarEl.addEventListener("click", (e) => {
      const target = (e.target as HTMLElement).closest("button");
      if (!target || target.disabled) return;
      if (target.classList.contains("accel-btn")) {
        this.callbacks.onTimeAccel(Number(target.dataset.accel));
      }
    });
  }

  private renderActions(s: Snapshot): void {
    // Bail before building any HTML: this runs every frame.
    const structureKey = `${this.activeTab}|${this.computeActionStructureKey(s)}`;
    if (structureKey === this.actionStructureKey) return;
    this.actionStructureKey = structureKey;

    const craft = this.selectedCraft(s);
    const parts: string[] = [];

    const flying =
      craft?.phase === "OnOrbit" || craft?.phase === "Transfer";
    // Contextual dock toolbar (accel lives in #time-toolbar top bar).
    const dockTools: string[] = [];
    if (flying) {
      dockTools.push(`<label class="toggle compact-toggle">
          <input type="checkbox" id="auto-downlink" ${this.callbacks.getAutoDownlink() ? "checked" : ""} />
          Auto-DL
        </label>`);
    }
    if (s.next_event_sim_seconds != null) {
      dockTools.push(
        `<button id="btn-warp-next" data-warp="${s.next_event_sim_seconds}" title="Warp to next fleet event">⏭ ${formatDuration(s.next_event_sim_seconds - s.sim_seconds)}</button>`,
      );
    }
    if (dockTools.length) {
      parts.push(`<div class="toolbar dock-toolbar">${dockTools.join("")}</div>`);
    }
    switch (this.activeTab) {
      case "mission":
        parts.push(...this.renderMissionTab(s));
        break;
      case "ops":
        parts.push(...renderOpsTab(s, craft));
        break;
      case "orbit":
        parts.push(...renderOrbitTab(s, craft));
        break;
      case "transfer":
        parts.push(...renderTransferTab(s, craft));
        break;
      case "company":
        parts.push(
          ...renderCompanyTab(s, this.content, {
            autoDownlink: this.callbacks.getAutoDownlink(),
            autoPauseArrival: this.callbacks.getAutoPauseArrival(),
            autoPauseAnomaly: this.callbacks.getAutoPauseAnomaly(),
            showTleRefs: this.callbacks.getShowTleRefs(),
            showLegend: this.callbacks.getShowLegend(),
          }),
        );
        break;
    }

    this.actionEl.innerHTML = parts.join("");
  }

  /** Compact Earth→Jupiter spine for the Mission tab. */
  private renderCareerPath(s: Snapshot): string {
    const nodes = s.programs
      .map((p, i) => {
        const short = p.name.replace(/\s*Program\s*/i, "").trim() || p.name;
        let state = "locked";
        let mark = "○";
        if (p.completed) {
          state = "done";
          mark = "●";
        } else if (p.unlocked) {
          state = "active";
          mark = "◉";
        }
        const canBuy =
          s.game_mode === "campaign" &&
          !p.unlocked &&
          !p.completed &&
          s.reputation >= p.reputation_required &&
          p.keystone_done &&
          p.tech_done &&
          s.cash_usd >= p.buyin_usd;
        const title = p.completed
          ? `${p.name} complete`
          : p.unlocked
            ? `${p.name} active`
            : canBuy
              ? `Unlock ${p.name} — $${p.buyin_usd.toLocaleString()}`
              : !p.keystone_done
                ? `Needs keystone ${p.requires_keystone ?? ""}`
                : !p.tech_done
                  ? `Needs tech ${p.requires_tech ?? ""}`
                  : s.reputation < p.reputation_required
                    ? `Needs rep ${p.reputation_required}`
                    : `Needs $${p.buyin_usd.toLocaleString()}`;
        const sep =
          i < s.programs.length - 1
            ? `<span class="career-sep" aria-hidden="true">─</span>`
            : "";
        const inner = `<span class="career-mark">${mark}</span><span class="career-label">${esc(short)}</span>`;
        const node = canBuy
          ? `<button type="button" class="career-node buyable btn-unlock-program" data-program="${esc(p.id)}" title="${esc(title)}">${inner}</button>`
          : `<span class="career-node ${state}" title="${esc(title)}">${inner}</span>`;
        return `${node}${sep}`;
      })
      .join("");
    const done = s.programs.filter((p) => p.completed).length;
    return `<div class="field career-path" aria-label="Career path">
      <label>Career · ${done}/${s.programs.length} programs</label>
      <div class="career-strip">${nodes}</div>
      <p class="hint">One agency path: complete body contracts → unlock the next world.</p>
    </div>`;
  }

  private renderMissionTab(s: Snapshot): string[] {
    const parts: string[] = [];
    const craft = this.selectedCraft(s);

    if (craft && craft.phase !== "Design") {
      const age =
        craft.launched_at_sim_seconds == null
          ? null
          : (s.sim_seconds - craft.launched_at_sim_seconds) / 86_400;
      const net = craft.lifetime_revenue_usd - craft.lifetime_ops_usd;
      parts.push(`
        <div class="field"><label>Service record — ${esc(craft.name)}</label>
          <div class="row">
            <input type="text" id="craft-name" value="${esc(craft.name)}" maxlength="40" title="Rename this craft" />
            <button id="btn-rename-craft">Rename</button>
          </div>
          <div class="stat"><label>Contracts</label><strong>${craft.contracts_completed}</strong>${age != null ? `<span class="stat-sub">${age.toFixed(1)} d in service</span>` : ""}</div>
          <div class="stat"><label>Net</label><strong class="${net >= 0 ? "" : "negative"}">${net >= 0 ? "+" : "−"}$${Math.abs(net).toLocaleString(undefined, { maximumFractionDigits: 0 })}</strong><span class="stat-sub">revenue vs lifetime ops</span></div>
        </div>
      `);
    }

    // Unified career spine (programs), not a multi-scenario picker.
    if (s.programs.length > 0) {
      parts.push(this.renderCareerPath(s));
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

      parts.push(...this.renderActiveContractsPanel(s));
      parts.push(...this.renderMarketplaceOffers(s));

      if (craft && this.craftHasContract(s, craft.id)) {
        const cost =
          this.content.buses.find((b) => b.id === craft.bus_id)
            ?.launch_cost_usd ?? 0;
        const atCap = s.active_flights >= s.max_flights;
        parts.push(`
          <button id="btn-launch" class="primary" ${s.cash_usd < cost || atCap ? "disabled" : ""} title="${atCap ? `Mission control full (${s.active_flights}/${s.max_flights})` : ""}">
            Launch ${esc(craft.name)} — $${cost.toLocaleString()}
          </button>
          <p class="hint">Checklist: bus configured · contract accepted · funds available · flight slot free</p>
        `);
      }
    } else {
      parts.push(...this.renderActiveContractsPanel(s));

      if (!s.active_contracts.length) {
        parts.push(
          `<div class="success-banner">Contract complete! Choose your next mission.</div>`,
        );
      }
      parts.push(...this.renderMarketplaceOffers(s, !s.active_contracts.length));

      if (s.active_contracts.length && !this.canAcceptMoreContracts(s)) {
        parts.push(
          `<p class="hint">Satellite on orbit. Use Ops and Orbit tabs during the mission.</p>`,
        );
      } else if (s.active_contracts.length) {
        parts.push(
          `<p class="hint">More contract slots available — accept another mission or focus the selected craft in Ops.</p>`,
        );
      }
      // else-branch: craft is non-null (design/on-orbit gate above).
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

  private computeActionStructureKey(s: Snapshot): string {
    const craft = this.selectedCraft(s);
    return [
      s.phase,
      s.game_mode,
      craft?.phase ?? "",
      craft?.bus_id ?? "",
      s.active_contracts
        .map((k) => `${k.template_id}:${k.assigned_craft_id ?? "-"}`)
        .join(",") || "none",
      this.selectedCraftId,
      s.transfers_unlocked,
      s.next_event_sim_seconds == null
        ? ""
        : Math.round((s.next_event_sim_seconds - s.sim_seconds) / 60),
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
      craft?.contracts_completed ?? "",
      craft?.name ?? "",
      craft?.transfer
        ? Math.round((craft.transfer.arrival_sim_seconds - s.sim_seconds) / 900)
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
        .map((m) => {
          const eta =
            m.contest_ends_sim_seconds != null
              ? Math.round((m.contest_ends_sim_seconds - s.sim_seconds) / 30)
              : "";
          return `${m.template_id}:${m.available}:${m.repeat}:${m.contested_by ?? ""}:${m.claimed_by ?? ""}:${Math.round(m.rush_bid_premium_usd ?? 0)}:${eta}`;
        })
        .join(","),
      s.market_enabled ? "1" : "0",
      (s.competitors ?? [])
        .map((c) => `${c.id}:${c.contracts_won}`)
        .join(","),
      s.max_flights,
      s.active_flights,
      this.marketFilter,
      s.active_world_event?.event_id ?? "",
      s.prestige
        ? `${s.prestige.score}:${s.prestige.rank}:${s.prestige.contested_wins}`
        : "",
    ].join("|");
  }

  private updateActionDynamicState(s: Snapshot): void {
    const craft = this.selectedCraft(s);

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
      launchBtn.disabled =
        s.cash_usd < cost || s.active_flights >= s.max_flights;
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

    const planeBtn = document.getElementById(
      "btn-plane-change",
    ) as HTMLButtonElement | null;
    if (planeBtn && craft) {
      const di = Number(
        (document.getElementById("plane-di") as HTMLInputElement | null)
          ?.value ?? 0,
      );
      planeBtn.disabled = !craft.at_node || di === 0;
      const node = document.getElementById("plane-node");
      if (node) {
        node.textContent = craft.at_node ? "READY" : "—";
        node.className = craft.at_node ? "pass-active" : "";
      }
      const dv = document.getElementById("plane-dv");
      if (dv) {
        const v = Math.hypot(...craft.velocity_eci_km_s);
        dv.textContent = `${(2 * v * Math.sin((Math.abs(di) * Math.PI) / 360) * 1000).toFixed(0)} m/s`;
      }
    }

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
        const craftId = this.resolveAcceptCraftId(s);
        this.callbacks.onAcceptContract(target.dataset.contract!, craftId);
        return;
      }
      if (target.classList.contains("btn-rush-bid")) {
        const craftId = this.resolveAcceptCraftId(s);
        this.callbacks.onRushBid(target.dataset.contract!, craftId);
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
        const busId = this.selectedCraft(s)?.bus_id;
        if (busId) this.callbacks.onDecommission(this.salvageUsd(busId));
        return;
      }
      if (target.id === "btn-downlink") {
        this.callbacks.onToggleDownlink(
          !this.selectedCraft(s)?.downlink_active,
        );
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
          this.selectedCraft(s)?.downlink_station_id ??
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
      if (target.id === "btn-plane-change") {
        const di = Number(
          (document.getElementById("plane-di") as HTMLInputElement | null)
            ?.value ?? 0,
        );
        if (di !== 0) this.callbacks.onPlaneChange(di);
        return;
      }
      if (target.classList.contains("btn-unlock-program")) {
        const programId = target.dataset.program;
        if (programId) {
          this.callbacks.onUnlockProgram(programId);
        }
        return;
      }
      if (target.id === "btn-rename-craft") {
        const input = document.getElementById(
          "craft-name",
        ) as HTMLInputElement | null;
        const craft = this.currentSnapshot
          ? this.selectedCraft(this.currentSnapshot)
          : null;
        if (input && craft && input.value.trim()) {
          this.callbacks.onRenameCraft(craft.id, input.value.trim());
        }
        return;
      }
      if (target.id === "btn-begin-transfer") {
        this.callbacks.onBeginTransfer();
        return;
      }
      if (
        target.id === "btn-warp-window" ||
        target.id === "btn-warp-arrival" ||
        target.id === "btn-warp-next"
      ) {
        const warpTo = Number(target.dataset.warp);
        if (Number.isFinite(warpTo)) {
          this.callbacks.onWarpTo(warpTo);
        }
        return;
      }
      if (
        target.id === "btn-new-career" ||
        target.id === "btn-new-sandbox" ||
        target.id === "btn-load-scenario"
      ) {
        const id =
          target.dataset.scenario ??
          (
            document.getElementById("scenario-select") as HTMLSelectElement | null
          )?.value;
        if (id) this.callbacks.onLoadScenario(id);
        return;
      }
      if (target.id === "btn-enter-sandbox-mode") {
        this.callbacks.onSetGameMode("sandbox");
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
        return;
      }
      if (target.id === "plane-di") {
        const di = Number((target as HTMLInputElement).value);
        document.getElementById("plane-di-val")!.textContent =
          `${di.toFixed(1)}°`;
        const craft = this.currentSnapshot
          ? this.selectedCraft(this.currentSnapshot)
          : null;
        const dv = document.getElementById("plane-dv");
        if (dv && craft) {
          const v = Math.hypot(...craft.velocity_eci_km_s);
          dv.textContent = `${(2 * v * Math.sin((Math.abs(di) * Math.PI) / 360) * 1000).toFixed(0)} m/s`;
        }
      }
    });

    this.actionEl.addEventListener("change", (e) => {
      const target = e.target as HTMLElement;
      if (target.id === "bus-select") {
        this.callbacks.onSelectBus((target as HTMLSelectElement).value);
        return;
      }
      if (target.id === "game-mode") {
        // Legacy select (if present): only allow campaign→sandbox mid-run.
        const mode = (target as HTMLSelectElement).value;
        if (mode === "sandbox") this.callbacks.onSetGameMode("sandbox");
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
      if (
        target.id === "auto-downlink" ||
        target.id === "pref-auto-downlink"
      ) {
        this.callbacks.onAutoDownlinkChange(
          (target as HTMLInputElement).checked,
        );
        return;
      }
      if (target.id === "pref-auto-pause-arrival") {
        this.callbacks.onAutoPauseArrivalChange(
          (target as HTMLInputElement).checked,
        );
        return;
      }
      if (target.id === "pref-auto-pause-anomaly") {
        this.callbacks.onAutoPauseAnomalyChange(
          (target as HTMLInputElement).checked,
        );
        return;
      }
      if (target.dataset.assignContract) {
        const val = (target as HTMLSelectElement).value;
        this.callbacks.onAssignContract(
          target.dataset.assignContract,
          val === "" ? null : Number(val),
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
            (document.getElementById("transfer-alt") as HTMLInputElement).value,
          );
          const inc = Number(
            (document.getElementById("transfer-inc") as HTMLInputElement).value,
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
            (document.getElementById("transfer-alt") as HTMLInputElement).value,
          );
          const inc = Number(
            (document.getElementById("transfer-inc") as HTMLInputElement).value,
          );
          this.callbacks.onSetTransferTarget(destSel.value, alt, inc);
        }
        return;
      }
    });
  }
}

import { Pane } from "tweakpane";
import gameContent from "../../content/game.json";
import { BodyView } from "./render/body-view.ts";
import { ViewManager } from "./render/view-manager.ts";
import {
  exportSave,
  importSave,
  initSim,
  initWasm,
  pullEvents,
  simAction,
  simStep,
} from "./sim/bridge.ts";
import type { GameContent, SimEvent, Snapshot } from "./types.ts";
import { isModalOpen } from "./ui/focus-trap.ts";
import { HudLayout } from "./ui/hud-layout.ts";
import { LoadingController } from "./ui/loading.ts";
import { confirmModal } from "./ui/modal.ts";
import { Onboarding } from "./ui/onboarding.ts";
import { GuidedMissions } from "./ui/guided.ts";
import { ACCEL_PRESETS, HudPanel, type HudTab } from "./ui/panel.ts";

const content = gameContent as GameContent;
const DEBUG = new URLSearchParams(location.search).has("debug");
const SETTINGS_KEY = "satelliteinc_settings";

interface Settings {
  autoDownlink: boolean;
  autoPauseArrival: boolean;
  autoPauseAnomaly: boolean;
  showTleRefs: boolean;
  showLegend: boolean;
  timeAccelBeforePause: number;
}

function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

function loadSettings(): Settings {
  const defaults: Settings = {
    autoDownlink: true,
    autoPauseArrival: true,
    autoPauseAnomaly: true,
    showTleRefs: true,
    showLegend: true,
    timeAccelBeforePause: 100,
  };
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    return raw
      ? { ...defaults, ...(JSON.parse(raw) as Partial<Settings>) }
      : defaults;
  } catch {
    return defaults;
  }
}

async function main(): Promise<void> {
  const loading = new LoadingController();
  loading.setStage("wasm");
  const canvas = document.getElementById("globe") as HTMLCanvasElement;

  const settings = loadSettings();
  let autoDownlink = settings.autoDownlink;
  let autoPauseArrival = settings.autoPauseArrival;
  let autoPauseAnomaly = settings.autoPauseAnomaly;

  function maybeAutoPause(kind: "arrival" | "anomaly"): void {
    const enabled = kind === "arrival" ? autoPauseArrival : autoPauseAnomaly;
    if (!enabled || !snapshot || snapshot.time_accel === 0) return;
    timeAccelBeforePause = snapshot.time_accel;
    runAction({ type: "set_time_accel", multiplier: 0 }, "Pause failed");
    panel.showToast("Paused — press Space to resume", "info");
  }
  let showTleRefs = settings.showTleRefs;
  let showLegend = settings.showLegend;
  let timeAccelBeforePause = settings.timeAccelBeforePause;
  let snapshot: Snapshot | null = null;

  function saveSettings(): void {
    try {
      localStorage.setItem(
        SETTINGS_KEY,
        JSON.stringify({
          autoDownlink,
          autoPauseArrival,
          autoPauseAnomaly,
          showTleRefs,
          showLegend,
          timeAccelBeforePause,
        }),
      );
    } catch {
      // Storage unavailable (private mode) — settings just won't persist.
    }
  }

  function applyLegendVisibility(): void {
    document
      .getElementById("globe-legend")
      ?.classList.toggle("hidden", !showLegend);
  }
  applyLegendVisibility();

  /** Apply a sim action; on rejection surface the error instead of throwing out of the handler. */
  function runAction(
    action: Record<string, unknown>,
    errorLabel = "Action failed",
  ): boolean {
    try {
      snapshot = simAction(action);
      panel.update(snapshot);
      return true;
    } catch (e) {
      panel.logError(`${errorLabel}: ${errorText(e)}`);
      return false;
    }
  }

  const globe = new BodyView(canvas, {
    onStationSelect: (stationId) => {
      if (snapshot?.spacecraft[0]?.phase !== "OnOrbit") {
        panel.showToast("Launch first to select a downlink station", "info");
        return;
      }
      if (
        runAction(
          {
            type: "select_downlink_station",
            craft_id: panel.getSelectedCraftId(),
            station_id: stationId,
          },
          "Station select failed",
        )
      ) {
        panel.logEvent(`Downlink station: ${stationId}`, "pass");
        panel.markTabAttention("ops");
      }
    },
    showTleRefs: () => showTleRefs,
  });

  await initWasm();
  loading.setStage("sim");
  initSim(content);

  loading.setStage("world");
  await globe.preloadBody("earth");
  new HudLayout();

  const saveImportInput = document.createElement("input");
  saveImportInput.type = "file";
  saveImportInput.accept = ".json,application/json";
  saveImportInput.hidden = true;
  document.body.appendChild(saveImportInput);

  const panel = new HudPanel(content, {
    onSelectBus: (busId) => {
      runAction({ type: "select_bus", bus_id: busId }, "Bus select failed");
    },
    onSetOrbit: (altitude, inclination) => {
      runAction(
        {
          type: "set_orbit",
          craft_id: panel.getSelectedCraftId(),
          altitude_km: altitude,
          inclination_deg: inclination,
        },
        "Set orbit failed",
      );
    },
    onAcceptContract: (contractId, craftId) => {
      try {
        snapshot = simAction({
          type: "accept_contract",
          contract_id: contractId,
          ...(craftId != null ? { assigned_craft_id: craftId } : {}),
        });
        panel.update(snapshot);
        panel.logEvent("Contract accepted", "contract");
        panel.markTabAttention("ops");
      } catch (e) {
        panel.logError(`Contract failed: ${errorText(e)}`);
      }
    },
    onRushBid: (contractId, craftId) => {
      try {
        snapshot = simAction({
          type: "rush_bid",
          contract_id: contractId,
          ...(craftId != null ? { assigned_craft_id: craftId } : {}),
        });
        panel.update(snapshot);
        panel.logEvent("Rush bid won — contract secured", "contract");
        panel.showToast("Rush bid secured", "success");
        panel.markTabAttention("ops");
      } catch (e) {
        panel.logError(`Rush bid failed: ${errorText(e)}`);
      }
    },
    onAssignContract: (contractId, craftId) => {
      try {
        snapshot = simAction({
          type: "assign_contract",
          contract_id: contractId,
          craft_id: craftId,
        });
        panel.update(snapshot);
        panel.logEvent(
          craftId == null
            ? `Contract unassigned: ${contractId}`
            : `Contract assigned to craft ${craftId}`,
          "contract",
        );
      } catch (e) {
        panel.logError(`Assign failed: ${errorText(e)}`);
      }
    },
    onLaunch: () => {
      try {
        snapshot = simAction({
          type: "launch",
          craft_id: panel.getSelectedCraftId(),
        });
        panel.update(snapshot);
        panel.logEvent("Satellite launched!", "contract");
        panel.showToast(
          "Satellite on orbit — collect data in sunlight",
          "success",
        );
        panel.markTabAttention("ops");
      } catch (e) {
        panel.logError(`Launch failed: ${errorText(e)}`);
      }
    },
    onToggleDownlink: (active) => {
      runAction(
        { type: "set_downlink", craft_id: panel.getSelectedCraftId(), active },
        "Downlink toggle failed",
      );
    },
    onTimeAccel: (multiplier) => {
      if (multiplier > 0) {
        timeAccelBeforePause = multiplier;
        saveSettings();
      }
      runAction({ type: "set_time_accel", multiplier }, "Time accel failed");
    },
    onProgradeBurn: (deltaV) => {
      try {
        snapshot = simAction({
          type: "prograde_burn",
          craft_id: panel.getSelectedCraftId(),
          delta_v_km_s: deltaV,
        });
        panel.update(snapshot);
        panel.logEvent(
          `Prograde +${(deltaV * 1000).toFixed(0)} m/s`,
          "contract",
        );
      } catch (e) {
        panel.logError(`Burn failed: ${errorText(e)}`);
      }
    },
    onRetrogradeBurn: (deltaV) => {
      try {
        snapshot = simAction({
          type: "retrograde_burn",
          craft_id: panel.getSelectedCraftId(),
          delta_v_km_s: deltaV,
        });
        panel.update(snapshot);
        panel.logEvent(
          `Retrograde −${(deltaV * 1000).toFixed(0)} m/s`,
          "contract",
        );
      } catch (e) {
        panel.logError(`Burn failed: ${errorText(e)}`);
      }
    },
    onSetManeuverTarget: (altitude) => {
      runAction(
        {
          type: "set_maneuver_target",
          craft_id: panel.getSelectedCraftId(),
          target_altitude_km: altitude,
        },
        "Maneuver target failed",
      );
    },
    onStartHohmann: () => {
      try {
        snapshot = simAction({
          type: "start_hohmann",
          craft_id: panel.getSelectedCraftId(),
        });
        panel.update(snapshot);
        panel.logEvent("Hohmann burn 1 — coast to next apsis", "contract");
      } catch (e) {
        panel.logError(`Hohmann failed: ${errorText(e)}`);
      }
    },
    onCompleteHohmann: () => {
      try {
        snapshot = simAction({
          type: "complete_hohmann",
          craft_id: panel.getSelectedCraftId(),
        });
        panel.update(snapshot);
        panel.logEvent("Hohmann transfer complete!", "contract");
        panel.showToast("Orbit circularized", "success");
      } catch (e) {
        panel.logError(`Circularize failed: ${errorText(e)}`);
      }
    },
    onPlaneChange: (deltaInclinationDeg) => {
      try {
        snapshot = simAction({
          type: "plane_change_burn",
          craft_id: panel.getSelectedCraftId(),
          delta_inclination_deg: deltaInclinationDeg,
        });
        panel.update(snapshot);
        panel.logEvent(
          `Plane change ${deltaInclinationDeg > 0 ? "+" : ""}${deltaInclinationDeg.toFixed(1)}° at node`,
          "contract",
        );
      } catch (e) {
        panel.logError(`Plane change failed: ${errorText(e)}`);
      }
    },
    onSelectDownlinkStation: (stationId) => {
      runAction(
        {
          type: "select_downlink_station",
          craft_id: panel.getSelectedCraftId(),
          station_id: stationId,
        },
        "Station select failed",
      );
    },
    onSetPayloadMode: (mode) => {
      runAction(
        {
          type: "set_payload_mode",
          craft_id: panel.getSelectedCraftId(),
          mode,
        },
        "Payload mode failed",
      );
    },
    onQueueCommand: (command, stationId) => {
      if (
        runAction(
          {
            type: "queue_command",
            craft_id: panel.getSelectedCraftId(),
            command,
            station_id: stationId,
          },
          "Queue failed",
        )
      ) {
        panel.logEvent(`Queued ${command} @ ${stationId}`);
      }
    },
    onPurchaseTech: (techId) => {
      try {
        snapshot = simAction({ type: "purchase_tech", tech_id: techId });
        panel.update(snapshot);
        panel.logEvent(`Purchased: ${techId}`, "contract");
        panel.showToast("R&D unlocked", "success");
      } catch (e) {
        panel.logError(`R&D failed: ${errorText(e)}`);
      }
    },
    onSetGameMode: (mode) => {
      runAction({ type: "set_game_mode", mode }, "Game mode failed");
    },
    onDecommission: (salvageUsd) => {
      void confirmModal(
        "Decommission satellite?",
        `You'll receive $${salvageUsd.toLocaleString()} salvage and return to the design phase. Any active contract keeps its deadline.`,
        "Decommission",
      ).then((confirmed) => {
        if (!confirmed) return;
        if (
          runAction(
            { type: "decommission", craft_id: panel.getSelectedCraftId() },
            "Decommission failed",
          )
        ) {
          panel.logEvent("Satellite decommissioned", "contract");
          panel.markTabAttention("mission");
        }
      });
    },
    onSetTransferTarget: (bodyId, captureAltitudeKm, captureInclinationDeg) => {
      runAction(
        {
          type: "set_transfer_target",
          craft_id: panel.getSelectedCraftId(),
          body_id: bodyId,
          capture_altitude_km: captureAltitudeKm,
          capture_inclination_deg: captureInclinationDeg,
        },
        "Transfer target failed",
      );
    },
    onRenameCraft: (craftId, name) => {
      if (
        runAction(
          { type: "rename_craft", craft_id: craftId, name },
          "Rename failed",
        )
      ) {
        panel.showToast(`Renamed to ${name}`, "info");
      }
    },
    onUnlockProgram: (programId) => {
      if (
        runAction(
          { type: "unlock_program", program_id: programId },
          "Program unlock failed",
        )
      ) {
        panel.showToast(
          "Program unlocked — new contracts in the marketplace",
          "success",
        );
        panel.markTabAttention("mission");
      }
    },
    onBeginTransfer: () => {
      if (
        runAction(
          { type: "begin_transfer", craft_id: panel.getSelectedCraftId() },
          "Transfer failed",
        )
      ) {
        panel.showToast("Injection burn complete — cruise started", "success");
        views.setActive("map");
      }
    },
    onWarpTo: (targetSimSeconds) => {
      const before = snapshot?.sim_days ?? 0;
      // WarpTo strides ~23 sim-days per call: re-issue until reached.
      for (let i = 0; i < 40; i++) {
        if (
          !runAction(
            { type: "warp_to", target_sim_seconds: targetSimSeconds },
            "Warp failed",
          )
        ) {
          return;
        }
        if ((snapshot?.sim_seconds ?? 0) >= targetSimSeconds - 1) break;
      }
      const days = (snapshot?.sim_days ?? 0) - before;
      panel.showToast(`Warped +${days.toFixed(1)} days`, "info");
    },
    onRestartCampaign: () => {
      const scenarioId =
        content.scenarios?.find((sc) => sc.game_mode === "campaign")?.id ??
        content.scenarios?.[0]?.id ??
        "career";
      if (
        runAction(
          { type: "load_scenario", scenario_id: scenarioId },
          "Restart failed",
        )
      ) {
        panel.logEvent("Career restarted", "contract");
        panel.setTab("mission");
      }
    },
    onShowHelp: () => {
      onboarding.show(true);
    },
    onAutoDownlinkChange: (active) => {
      autoDownlink = active;
      saveSettings();
    },
    onAutoPauseArrivalChange: (active) => {
      autoPauseArrival = active;
      saveSettings();
    },
    onAutoPauseAnomalyChange: (active) => {
      autoPauseAnomaly = active;
      saveSettings();
    },
    getAutoPauseArrival: () => autoPauseArrival,
    getAutoPauseAnomaly: () => autoPauseAnomaly,
    onShowTleRefsChange: (active) => {
      showTleRefs = active;
      globe.setShowTleRefs(active);
      saveSettings();
    },
    onShowLegendChange: (active) => {
      showLegend = active;
      applyLegendVisibility();
      saveSettings();
    },
    getShowLegend: () => showLegend,
    onLoadScenario: (scenarioId) => {
      try {
        snapshot = simAction({
          type: "load_scenario",
          scenario_id: scenarioId,
        });
        panel.update(snapshot);
        const label =
          content.scenarios?.find((sc) => sc.id === scenarioId)?.name ??
          scenarioId;
        panel.logEvent(`Started: ${label}`, "contract");
        panel.showToast(label, "success");
        panel.markTabAttention("mission");
      } catch (e) {
        panel.logError(`Start failed: ${errorText(e)}`);
      }
    },
    onExportSave: () => {
      try {
        const json = exportSave();
        localStorage.setItem("satelliteinc_save", json);
        const blob = new Blob([json], { type: "application/json" });
        const url = URL.createObjectURL(blob);
        const anchor = document.createElement("a");
        anchor.href = url;
        anchor.download = `satelliteinc-save-${new Date().toISOString().slice(0, 10)}.json`;
        anchor.click();
        URL.revokeObjectURL(url);
        panel.showToast("Save downloaded", "success");
        panel.logEvent("Game saved");
      } catch (e) {
        panel.logError(`Save failed: ${errorText(e)}`);
      }
    },
    onImportSave: () => {
      saveImportInput.click();
    },
    getAutoDownlink: () => autoDownlink,
    getShowTleRefs: () => showTleRefs,
  });
  panel.showSkeleton();

  /** Default parking orbit for map → transfer staging. */
  function captureOrbitFor(bodyId: string): {
    altitude: number;
    inclination: number;
  } {
    const defaults: Record<string, { altitude: number; inclination: number }> =
      {
        earth: { altitude: 400, inclination: 51.6 },
        moon: { altitude: 100, inclination: 85 },
        mars: { altitude: 400, inclination: 30 },
        venus: { altitude: 400, inclination: 30 },
        jupiter: { altitude: 5000, inclination: 20 },
      };
    return defaults[bodyId] ?? { altitude: 400, inclination: 30 };
  }

  const views = new ViewManager(globe, canvas, {
    getSelectedCraftId: () => panel.getSelectedCraftId(),
    onOpenBodyView: () => {
      views.setActive("body");
    },
    onFocusCraft: (craftId) => {
      panel.setSelectedCraftId(craftId);
      views.setActive("body");
      panel.showToast("Focused craft at selected body", "info");
    },
    onStageAtBody: (bodyId) => {
      const craftId = panel.getSelectedCraftId();
      if (
        runAction(
          { type: "set_design_body", craft_id: craftId, body_id: bodyId },
          "Stage failed",
        )
      ) {
        const parking = captureOrbitFor(bodyId);
        runAction(
          {
            type: "set_orbit",
            craft_id: craftId,
            altitude_km: parking.altitude,
            inclination_deg: parking.inclination,
          },
          "Set parking orbit failed",
        );
        panel.logEvent(`Staged design craft at ${bodyId}`, "contract");
        panel.showToast(`Design craft staged at ${bodyId}`, "success");
        views.setActive("body");
        panel.markTabAttention("mission");
      }
    },
    onPlanTransfer: (bodyId) => {
      const parking = captureOrbitFor(bodyId);
      if (
        runAction(
          {
            type: "set_transfer_target",
            craft_id: panel.getSelectedCraftId(),
            body_id: bodyId,
            capture_altitude_km: parking.altitude,
            capture_inclination_deg: parking.inclination,
          },
          "Transfer target failed",
        )
      ) {
        panel.setTab("transfer");
        panel.logEvent(`Transfer target: ${bodyId}`, "contract");
        panel.showToast(
          `Transfer planned to ${bodyId} — check Transfer tab`,
          "success",
        );
        // Stay on map so the player sees the target; they can press M for body.
      }
    },
  });

  saveImportInput.addEventListener("change", () => {
    void (async () => {
      const file = saveImportInput.files?.[0];
      if (!file) return;
      try {
        const json = await file.text();
        snapshot = importSave(json);
        localStorage.setItem("satelliteinc_save", json);
        panel.update(snapshot);
        panel.showToast(`Loaded ${file.name}`, "success");
        panel.logEvent(`Game loaded from ${file.name}`);
      } catch (e) {
        panel.logError(`Load failed: ${errorText(e)}`);
      }
      saveImportInput.value = "";
    })();
  });

  if (DEBUG) {
    const debugPane = new Pane({ title: "Debug", expanded: false });
    const debugParams = { timeAccel: 100, autoDownlink: true };
    debugPane.addBinding(debugParams, "timeAccel", {
      min: 1,
      max: 10000,
      step: 10,
    });
    debugPane.addBinding(debugParams, "autoDownlink");
    setInterval(() => {
      if (debugParams.timeAccel !== snapshot?.time_accel) {
        runAction(
          { type: "set_time_accel", multiplier: debugParams.timeAccel },
          "Debug time accel failed",
        );
      }
      autoDownlink = debugParams.autoDownlink;
    }, 200);
  }

  const onboarding = new Onboarding();
  const guided = new GuidedMissions();

  // Contextual first-time cards replace the front-loaded deck: one card per
  // trigger, at the moment the system first matters.
  const CONTEXT_CARDS: Partial<
    Record<SimEvent["type"], { title: string; body: string }>
  > = {
    launched: {
      title: "You're in orbit",
      body: "Collect imagery in sunlight; the buffer fills. During a station pass, downlink to make contract progress. The status panel always says why data is (or isn't) flowing.",
    },
    pass_started: {
      title: "First pass",
      body: "A ground station sees your craft: downlink now (D key or the Ops tab). Auto-downlink can do this for you — it's on by default in settings.",
    },
    anomaly_started: {
      title: "First anomaly",
      body: "Something broke and it drains extra power until handled. Queue Clear anomaly / Reboot comms / Safe mode from the Ops tab — commands execute at the next pass.",
    },
    cash_negative: {
      title: "You're burning reserves",
      body: "Cash is negative. Bankruptcy hits at three days of burn below zero — get a contract flowing or decommission idle craft (their ops bill never sleeps).",
    },
    transfer_started: {
      title: "Cruise mode",
      body: "The craft hibernates: 10% ops cost, no anomalies, capture propellant already reserved. Warp to arrival whenever you like — insertion is automatic.",
    },
    transfer_arrived: {
      title: "Captured",
      body: "You're in orbit at a new body. Local stations (if any) or the Earth DSN carry your downlink — the pipe is thinner out here; check the effective Mbps.",
    },
  };
  const seenCards = new Set<string>(
    (() => {
      try {
        return JSON.parse(
          localStorage.getItem("satelliteinc_seen_cards") ?? "[]",
        ) as string[];
      } catch {
        return [];
      }
    })(),
  );
  function maybeShowContextCard(type: SimEvent["type"]): void {
    const card = CONTEXT_CARDS[type];
    if (!card || seenCards.has(type)) return;
    seenCards.add(type);
    try {
      localStorage.setItem(
        "satelliteinc_seen_cards",
        JSON.stringify([...seenCards]),
      );
    } catch {
      // storage unavailable
    }
    onboarding.showSingle(card.title, card.body);
  }
  // The linear deck stays a reference manual under `?`; teaching happens
  // through contextual first-time cards and guided missions.

  // Keyboard shortcuts (skip while typing in a form control or a modal is up).
  const TAB_KEYS: Record<string, HudTab> = {
    "5": "transfer",
    "1": "mission",
    "2": "ops",
    "3": "orbit",
    "4": "company",
  };
  window.addEventListener("keydown", (e) => {
    const target = e.target as HTMLElement;
    if (
      target instanceof HTMLInputElement ||
      target instanceof HTMLSelectElement ||
      target instanceof HTMLTextAreaElement
    ) {
      return;
    }
    if (isModalOpen()) return;
    const accel = snapshot?.time_accel ?? 1;
    if (e.key === " ") {
      e.preventDefault();
      if (accel === 0) {
        runAction(
          { type: "set_time_accel", multiplier: timeAccelBeforePause },
          "Resume failed",
        );
      } else {
        timeAccelBeforePause = accel;
        saveSettings();
        runAction({ type: "set_time_accel", multiplier: 0 }, "Pause failed");
      }
    } else if (e.key === "+" || e.key === "=" || e.key === "-") {
      const idx = ACCEL_PRESETS.findIndex((p) => p >= accel);
      const cur = idx === -1 ? ACCEL_PRESETS.length - 1 : idx;
      const next =
        e.key === "-"
          ? Math.max(0, cur - 1)
          : Math.min(ACCEL_PRESETS.length - 1, cur + 1);
      const multiplier = ACCEL_PRESETS[next]!;
      if (multiplier > 0) {
        timeAccelBeforePause = multiplier;
        saveSettings();
      }
      runAction({ type: "set_time_accel", multiplier }, "Time accel failed");
    } else if (TAB_KEYS[e.key]) {
      panel.setTab(TAB_KEYS[e.key]!);
    } else if (e.key.toLowerCase() === "d") {
      const sel = snapshot?.spacecraft.find(
        (c) => c.id === panel.getSelectedCraftId(),
      );
      if (sel?.phase === "OnOrbit") {
        runAction(
          {
            type: "set_downlink",
            craft_id: sel.id,
            active: !sel.downlink_active,
          },
          "Downlink toggle failed",
        );
      }
    } else if (e.key.toLowerCase() === "m") {
      views.toggle();
    } else if (e.key === "?") {
      onboarding.show(true);
    }
  });

  let lastTime = performance.now();

  function craftName(id: number): string {
    return snapshot?.spacecraft.find((c) => c.id === id)?.name ?? `craft ${id}`;
  }

  function formatEvent(ev: SimEvent): string {
    const prefix =
      "craft_id" in ev && typeof ev.craft_id === "number"
        ? `[${"craft_name" in ev ? ev.craft_name : craftName(ev.craft_id)}] `
        : "";
    return prefix + formatEventBody(ev);
  }

  function formatEventBody(ev: SimEvent): string {
    switch (ev.type) {
      case "pass_started":
        return `Pass started — ${ev.station_name}`;
      case "pass_ended":
        return "Pass ended";
      case "contract_completed":
        return `Contract complete! +$${ev.payout_usd.toLocaleString()}`;
      case "contract_failed":
        return "Contract failed — deadline missed";
      case "launched":
        return `Launched ${ev.altitude_km} km / ${ev.inclination_deg}°`;
      case "insufficient_funds":
        return `Need $${ev.required_usd.toLocaleString()}`;
      case "action_applied":
        return ev.message;
      case "maneuver_executed":
        return `${ev.label}: ${(ev.delta_v_km_s * 1000).toFixed(0)} m/s`;
      case "hohmann_burn1_complete":
        return `Hohmann burn 1 — coast to ${ev.target_altitude_km} km`;
      case "hohmann_transfer_complete":
        return `Hohmann complete @ ${ev.target_altitude_km} km`;
      case "anomaly_started":
        return `Anomaly: ${ev.label}`;
      case "anomaly_cleared":
        return "Anomaly cleared";
      case "command_executed":
        return `Command @ ${ev.station_name}: ${ev.message}`;
      case "tech_purchased":
        return `R&D: ${ev.name}`;
      case "reputation_changed":
        return `Rep ${ev.delta >= 0 ? "+" : ""}${ev.delta.toFixed(0)} → ${ev.new_value.toFixed(0)}`;
      case "decommissioned":
        return `Decommissioned — +$${ev.salvage_usd.toLocaleString()} salvage`;
      case "game_won":
        return "VICTORY — all contracts complete!";
      case "game_over":
        return `GAME OVER — ${ev.reason === "bankrupt" ? "bankruptcy" : "reputation lost"}`;
      case "cash_negative":
        return "WARNING: cash is negative — bankruptcy at −$50k";
      case "transfer_started":
        return `Transfer injection — $${ev.injection_cost_usd.toLocaleString()}, arrival day ${(ev.arrival_sim_seconds / 86_400).toFixed(1)}`;
      case "transfer_arrived":
        return `Arrived at ${ev.body_id}`;
      case "capture_executed":
        return `Capture burn executed — ${ev.delta_v_km_s.toFixed(2)} km/s`;
      case "warp_completed":
        return `Warped +${(ev.seconds / 86_400).toFixed(1)} days`;
      case "program_unlocked":
        return `Program unlocked: ${ev.program_id.replace(/_/g, " ")}`;
      case "program_completed":
        return `PROGRAM COMPLETE: ${ev.program_id.replace(/_/g, " ")} — +$${ev.grant_usd.toLocaleString()} grant`;
      case "contract_contested":
        return `${ev.competitor_name} is bidding on ${ev.contract_id.replace(/_/g, " ")} — accept or rush bid before day ${(ev.ends_at_sim_seconds / 86_400).toFixed(1)}`;
      case "contract_claimed_by_rival":
        return `${ev.competitor_name} claimed ${ev.contract_id.replace(/_/g, " ")}`;
      case "rush_bid_won":
        return `Rush bid secured ${ev.contract_id.replace(/_/g, " ")} (−$${ev.premium_usd.toLocaleString()})`;
      case "world_event_started":
        return `Weather: ${ev.name} until day ${(ev.ends_at_sim_seconds / 86_400).toFixed(1)}`;
      case "world_event_ended":
        return `Weather cleared: ${ev.name}`;
      default:
        return JSON.stringify(ev);
    }
  }

  function eventClass(ev: SimEvent): string {
    if (ev.type.includes("pass")) return "pass";
    if (ev.type.includes("contract") || ev.type.includes("tech") || ev.type.includes("rush") || ev.type.includes("rival"))
      return "contract";
    if (ev.type.includes("anomaly")) return "pass";
    if (ev.type === "contract_failed" || ev.type === "contract_claimed_by_rival")
      return "error";
    if (ev.type === "contract_contested") return "pass";
    return "";
  }

  function frame(now: number): void {
    // Never let a sim error kill the loop: log it and keep rendering.
    try {
      const realDt = Math.min((now - lastTime) / 1000, 0.1);
      lastTime = now;

      snapshot = simStep(realDt);

      if (autoDownlink && snapshot.phase !== "game_over") {
        for (const c of snapshot.spacecraft) {
          if (
            c.phase === "OnOrbit" &&
            c.in_pass &&
            !c.downlink_active &&
            c.buffer_mb > 1
          ) {
            runAction(
              { type: "set_downlink", craft_id: c.id, active: true },
              "Auto-downlink failed",
            );
          }
        }
      }

      for (const ev of pullEvents()) {
        maybeShowContextCard(ev.type);
        const msg = formatEvent(ev);
        panel.logEvent(msg, eventClass(ev));
        if (
          ev.type === "pass_started" &&
          ev.craft_id === panel.getSelectedCraftId()
        ) {
          // Queue tier: only the selected craft's passes toast — with four
          // LEO craft at 90-minute periods, everything else is spam (log
          // still records all of them).
          panel.showToast(msg, "success");
        }
        if (ev.type === "contract_completed") {
          panel.showToast(msg, "success");
          panel.markTabAttention("mission");
        }
        if (ev.type === "anomaly_started") {
          panel.showToast(msg, "error");
          panel.markTabAttention("ops");
          // Interrupt tier only when the stricken craft carries a contract.
          if (
            snapshot.active_contracts.some(
              (c) => c.assigned_craft_id === ev.craft_id,
            )
          ) {
            maybeAutoPause("anomaly");
          }
        }
        if (ev.type === "transfer_arrived") {
          panel.showToast(msg, "success");
          views.setActive("body");
          panel.markTabAttention("mission");
          maybeAutoPause("arrival");
        }
        if (ev.type === "program_completed") {
          panel.showToast(msg, "success");
          panel.markTabAttention("company");
        }
        if (ev.type === "game_won") {
          panel.showToast(msg, "success");
        }
        if (ev.type === "game_over" || ev.type === "cash_negative") {
          panel.showToast(msg, "error");
        }
        if (ev.type === "contract_contested") {
          panel.showToast(msg, "info");
          panel.markTabAttention("mission");
        }
        if (ev.type === "contract_claimed_by_rival") {
          panel.showToast(msg, "error");
          panel.markTabAttention("mission");
        }
        if (ev.type === "rush_bid_won") {
          panel.showToast(msg, "success");
        }
        if (ev.type === "world_event_started") {
          panel.showToast(msg, "info");
          panel.markTabAttention("ops");
        }
        if (ev.type === "world_event_ended") {
          panel.showToast(msg, "success");
        }
      }

      views.update(snapshot, panel.getSelectedCraftId());
      panel.update(snapshot);
      guided.update(snapshot);
      views.render();
    } catch (e) {
      console.error("frame error:", e);
      panel.logError(`Sim error: ${errorText(e)}`);
    } finally {
      requestAnimationFrame(frame);
    }
  }

  snapshot = simStep(0);
  const startScenario =
    content.scenarios?.find((sc) => sc.game_mode === "campaign")?.id ??
    content.scenarios?.[0]?.id ??
    null;
  if (startScenario) {
    snapshot = simAction({ type: "load_scenario", scenario_id: startScenario });
  } else if (snapshot.spacecraft.length > 0) {
    snapshot = simAction({ type: "select_bus", bus_id: content.buses[0]!.id });
  }
  panel.update(snapshot);
  loading.hide();
  requestAnimationFrame(frame);
}

main().catch((err: unknown) => {
  console.error(err);
  document.getElementById("loading")?.classList.add("hidden");
  const pre = document.createElement("pre");
  pre.className = "fatal-error";
  pre.textContent = `Failed to start: ${errorText(err)}`;
  document.body.replaceChildren(pre);
});

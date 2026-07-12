import { Pane } from "tweakpane";
import gameContent from "../../content/game.json";
import { BodyView } from "./render/body-view.ts";
import {
  exportSave,
  importSave,
  loadSim,
  pullEvents,
  simAction,
  simStep,
} from "./sim/bridge.ts";
import type { GameContent, SimEvent, Snapshot } from "./types.ts";
import { confirmModal } from "./ui/modal.ts";
import { Onboarding } from "./ui/onboarding.ts";
import { ACCEL_PRESETS, HudPanel, type HudTab } from "./ui/panel.ts";

const content = gameContent as GameContent;
const DEBUG = new URLSearchParams(location.search).has("debug");
const SETTINGS_KEY = "satelliteinc_settings";

interface Settings {
  autoDownlink: boolean;
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
  const loadingEl = document.getElementById("loading")!;
  const canvas = document.getElementById("globe") as HTMLCanvasElement;

  const settings = loadSettings();
  let autoDownlink = settings.autoDownlink;
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
      if (snapshot?.spacecraft?.phase !== "OnOrbit") {
        panel.showToast("Launch first to select a downlink station", "info");
        return;
      }
      if (
        runAction(
          { type: "select_downlink_station", station_id: stationId },
          "Station select failed",
        )
      ) {
        panel.logEvent(`Downlink station: ${stationId}`, "pass");
        panel.markTabAttention("ops");
      }
    },
    showTleRefs: () => showTleRefs,
  });

  await loadSim(content);
  loadingEl.classList.add("hidden");

  const panel = new HudPanel(content, {
    onSelectBus: (busId) => {
      runAction({ type: "select_bus", bus_id: busId }, "Bus select failed");
    },
    onSetOrbit: (altitude, inclination) => {
      runAction(
        {
          type: "set_orbit",
          altitude_km: altitude,
          inclination_deg: inclination,
        },
        "Set orbit failed",
      );
    },
    onAcceptContract: (contractId) => {
      try {
        snapshot = simAction({
          type: "accept_contract",
          contract_id: contractId,
        });
        panel.update(snapshot);
        panel.logEvent("Contract accepted", "contract");
      } catch (e) {
        panel.logError(`Contract failed: ${errorText(e)}`);
      }
    },
    onLaunch: () => {
      try {
        snapshot = simAction({ type: "launch" });
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
      runAction({ type: "set_downlink", active }, "Downlink toggle failed");
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
        snapshot = simAction({ type: "prograde_burn", delta_v_km_s: deltaV });
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
        snapshot = simAction({ type: "retrograde_burn", delta_v_km_s: deltaV });
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
        { type: "set_maneuver_target", target_altitude_km: altitude },
        "Maneuver target failed",
      );
    },
    onStartHohmann: () => {
      try {
        snapshot = simAction({ type: "start_hohmann" });
        panel.update(snapshot);
        panel.logEvent("Hohmann burn 1 — coast to next apsis", "contract");
      } catch (e) {
        panel.logError(`Hohmann failed: ${errorText(e)}`);
      }
    },
    onCompleteHohmann: () => {
      try {
        snapshot = simAction({ type: "complete_hohmann" });
        panel.update(snapshot);
        panel.logEvent("Hohmann transfer complete!", "contract");
        panel.showToast("Orbit circularized", "success");
      } catch (e) {
        panel.logError(`Circularize failed: ${errorText(e)}`);
      }
    },
    onSelectDownlinkStation: (stationId) => {
      runAction(
        { type: "select_downlink_station", station_id: stationId },
        "Station select failed",
      );
    },
    onSetPayloadMode: (mode) => {
      runAction({ type: "set_payload_mode", mode }, "Payload mode failed");
    },
    onQueueCommand: (command, stationId) => {
      if (
        runAction(
          { type: "queue_command", command, station_id: stationId },
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
        if (runAction({ type: "decommission" }, "Decommission failed")) {
          panel.logEvent("Satellite decommissioned", "contract");
          panel.markTabAttention("mission");
        }
      });
    },
    onRestartCampaign: () => {
      const scenarioId = content.scenarios?.[0]?.id ?? "campaign_start";
      if (
        runAction(
          { type: "load_scenario", scenario_id: scenarioId },
          "Restart failed",
        )
      ) {
        panel.logEvent("Campaign restarted", "contract");
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
        panel.logEvent(`Scenario loaded: ${scenarioId}`, "contract");
        panel.showToast(`Scenario: ${scenarioId}`, "success");
        panel.markTabAttention("mission");
      } catch (e) {
        panel.logError(`Scenario failed: ${errorText(e)}`);
      }
    },
    onExportSave: () => {
      try {
        const json = exportSave();
        localStorage.setItem("satelliteinc_save", json);
        panel.showToast("Save exported to browser storage", "success");
        panel.logEvent("Game saved");
      } catch (e) {
        panel.logError(`Save failed: ${errorText(e)}`);
      }
    },
    onImportSave: () => {
      try {
        const json = localStorage.getItem("satelliteinc_save");
        if (!json) {
          panel.logError("No save found in browser storage");
          return;
        }
        snapshot = importSave(json);
        panel.update(snapshot);
        panel.showToast("Save imported", "success");
        panel.logEvent("Game loaded");
      } catch (e) {
        panel.logError(`Load failed: ${errorText(e)}`);
      }
    },
    getAutoDownlink: () => autoDownlink,
    getShowTleRefs: () => showTleRefs,
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
  onboarding.show();

  // Keyboard shortcuts (skip while typing in a form control or a modal is up).
  const TAB_KEYS: Record<string, HudTab> = {
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
      if (snapshot?.spacecraft?.phase === "OnOrbit") {
        runAction(
          {
            type: "set_downlink",
            active: !snapshot.spacecraft.downlink_active,
          },
          "Downlink toggle failed",
        );
      }
    } else if (e.key === "?") {
      onboarding.show(true);
    }
  });

  let lastTime = performance.now();

  function formatEvent(ev: SimEvent): string {
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
      default:
        return JSON.stringify(ev);
    }
  }

  function eventClass(ev: SimEvent): string {
    if (ev.type.includes("pass")) return "pass";
    if (ev.type.includes("contract") || ev.type.includes("tech"))
      return "contract";
    if (ev.type.includes("anomaly")) return "pass";
    if (ev.type === "contract_failed") return "error";
    return "";
  }

  function frame(now: number): void {
    // Never let a sim error kill the loop: log it and keep rendering.
    try {
      const realDt = Math.min((now - lastTime) / 1000, 0.1);
      lastTime = now;

      snapshot = simStep(realDt);

      if (
        autoDownlink &&
        snapshot.phase !== "game_over" &&
        snapshot.spacecraft?.phase === "OnOrbit" &&
        snapshot.spacecraft.in_pass &&
        !snapshot.spacecraft.downlink_active &&
        snapshot.spacecraft.buffer_mb > 1
      ) {
        runAction(
          { type: "set_downlink", active: true },
          "Auto-downlink failed",
        );
      }

      for (const ev of pullEvents()) {
        const msg = formatEvent(ev);
        panel.logEvent(msg, eventClass(ev));
        if (ev.type === "pass_started") {
          panel.showToast(msg, "success");
        }
        if (ev.type === "contract_completed") {
          panel.showToast(msg, "success");
          panel.markTabAttention("mission");
        }
        if (ev.type === "anomaly_started") {
          panel.showToast(msg, "error");
          panel.markTabAttention("ops");
        }
        if (ev.type === "game_won") {
          panel.showToast(msg, "success");
        }
        if (ev.type === "game_over" || ev.type === "cash_negative") {
          panel.showToast(msg, "error");
        }
      }

      globe.update(snapshot);
      panel.update(snapshot);
      globe.render();
    } catch (e) {
      console.error("frame error:", e);
      panel.logError(`Sim error: ${errorText(e)}`);
    } finally {
      requestAnimationFrame(frame);
    }
  }

  snapshot = simStep(0);
  const startScenario = content.scenarios?.[0]?.id ?? null;
  if (startScenario) {
    snapshot = simAction({ type: "load_scenario", scenario_id: startScenario });
  } else if (snapshot.spacecraft) {
    snapshot = simAction({ type: "select_bus", bus_id: content.buses[0]!.id });
  }
  panel.update(snapshot);
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

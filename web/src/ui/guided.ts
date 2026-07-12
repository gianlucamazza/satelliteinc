import type { Snapshot } from "../types.ts";

const STORAGE_KEY = "satelliteinc_guided_done";

interface GuidedStep {
  text: string;
  done: (s: Snapshot) => boolean;
}

interface GuidedMission {
  id: string;
  title: string;
  /** The mission attaches when this becomes true in the player's campaign. */
  active: (s: Snapshot) => boolean;
  steps: GuidedStep[];
}

function craftFor(s: Snapshot, contractBody: string) {
  const contract = s.active_contracts.find((c) => c.body_id === contractBody);
  return s.spacecraft.find((c) => c.id === contract?.assigned_craft_id) ?? null;
}

/**
 * In-campaign guided missions: checklists that advance on SIM STATE, not
 * clicks — the replacement for the old tutorial scenarios that taught with
 * a company that wasn't yours.
 */
const MISSIONS: GuidedMission[] = [
  {
    id: "hohmann-basics",
    title: "Guided: fix a non-compliant orbit",
    active: (s) =>
      s.game_mode === "campaign" &&
      s.active_contracts.some(
        (c) =>
          c.orbit_ok === false &&
          s.spacecraft.some(
            (k) => k.id === c.assigned_craft_id && k.body_id === c.body_id,
          ),
      ),
    steps: [
      {
        text: "Open the Orbit tab and set the maneuver target inside the contract band",
        done: (s) =>
          s.spacecraft.some(
            (c) => c.hohmann_preview != null || c.pending_hohmann != null,
          ),
      },
      {
        text: "Burn 1 at the apsis (wait for READY)",
        done: (s) => s.spacecraft.some((c) => c.pending_hohmann != null),
      },
      {
        text: "Coast to the other apsis and fire burn 2",
        done: (s) => s.active_contracts.every((c) => c.orbit_ok !== false),
      },
    ],
  },
  {
    id: "first-transfer",
    title: "Guided: your first transfer",
    active: (s) =>
      s.game_mode === "campaign" &&
      s.active_contracts.some(
        (c) => c.body_id !== "earth" && c.assigned_craft_id != null,
      ),
    steps: [
      {
        text: "Transfer tab: pick the contract's body as destination",
        done: (s) =>
          s.active_contracts.some(
            (c) =>
              c.body_id !== "earth" &&
              (craftFor(s, c.body_id)?.transfer_planner?.target_body_id ===
                c.body_id ||
                craftFor(s, c.body_id)?.transfer?.target_body_id ===
                  c.body_id ||
                craftFor(s, c.body_id)?.body_id === c.body_id),
          ),
      },
      {
        text: "Wait (or warp) for the window, then buy the injection",
        done: (s) =>
          s.spacecraft.some((c) => c.phase === "Transfer") ||
          s.active_contracts.some(
            (c) =>
              c.body_id !== "earth" &&
              craftFor(s, c.body_id)?.body_id === c.body_id,
          ),
      },
      {
        text: "Cruise hibernated (10% ops) — warp to arrival; capture is automatic",
        done: (s) =>
          s.active_contracts.some(
            (c) =>
              c.body_id !== "earth" &&
              craftFor(s, c.body_id)?.body_id === c.body_id,
          ),
      },
      {
        text: "Reach a compliant orbit and start the downlink",
        done: (s) =>
          s.active_contracts.some(
            (c) => c.body_id !== "earth" && c.orbit_ok === true,
          ),
      },
    ],
  },
];

export class GuidedMissions {
  private readonly el: HTMLElement | null;
  private done: Set<string>;

  constructor() {
    this.el = document.getElementById("guided-overlay");
    let stored: string[] = [];
    try {
      stored = JSON.parse(
        localStorage.getItem(STORAGE_KEY) ?? "[]",
      ) as string[];
    } catch {
      // corrupted flags: start over
    }
    this.done = new Set(stored);
  }

  private persist(): void {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify([...this.done]));
    } catch {
      // storage unavailable — checklists just reappear next session
    }
  }

  update(s: Snapshot): void {
    if (!this.el) return;
    const mission = MISSIONS.find((m) => !this.done.has(m.id) && m.active(s));
    if (!mission) {
      this.el.classList.add("hidden");
      return;
    }
    const states = mission.steps.map((step) => step.done(s));
    if (states.every(Boolean)) {
      this.done.add(mission.id);
      this.persist();
      this.el.classList.add("hidden");
      return;
    }
    // First undone step is the current one; earlier ones show as complete.
    const firstOpen = states.findIndex((d) => !d);
    const rows = mission.steps
      .map((step, i) => {
        const complete = i < firstOpen || states[i];
        return `<li class="${complete ? "done" : i === firstOpen ? "current" : ""}">${step.text}</li>`;
      })
      .join("");
    this.el.innerHTML = `<h4>${mission.title}</h4><ol>${rows}</ol>`;
    this.el.classList.remove("hidden");
  }
}

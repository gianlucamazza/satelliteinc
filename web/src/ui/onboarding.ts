import { releaseFocus, trapFocus } from "./focus-trap.ts";

const STORAGE_KEY = "satelliteinc_onboarding_done";

const STEPS = [
  {
    title: "Welcome to Satellite Inc",
    body: "One career, one agency: grow from an Earth imaging startup into an interplanetary operator. Design, launch, downlink — then unlock the Moon, Mars, Venus, and Jupiter through Programs.",
  },
  {
    title: "Design & contract",
    body: "Pick a bus, set altitude and inclination, then accept a contract from the marketplace. Watch the requirements: some contracts demand a specific altitude band or inclination.",
  },
  {
    title: "Launch",
    body: "Launch when ready. Your satellite enters a real Keplerian orbit simulated in Rust/WASM.",
  },
  {
    title: "Operate",
    body: "Collect imagery in sunlight. During a ground-station pass, downlink data to complete the contract. The status panel tells you why data is (or isn't) flowing.",
  },
  {
    title: "Watch your burn rate",
    body: "Operating costs money every sim-day — idling drains cash. If cash falls below −$50k, you're bankrupt. Keep a contract in the pipeline. Rivals and space weather grow harsher as you complete programs.",
  },
  {
    title: "Maneuver",
    body: "Use the Orbit tab to change altitude with a Hohmann transfer: burn 1 at one apsis, coast, burn 2 at the other. The HUD counts down to the next apsis.",
  },
  {
    title: "Handle anomalies (FDIR)",
    body: "Faults are procedure-specific: EPS clear for power glitches, reboot for comms dropouts, safe mode to shed load. SEU buffer hits cost data immediately. Commands uplink at the next pass (plus light-time beyond Earth).",
  },
  {
    title: "Redesign when needed",
    body: "Inclination can't change on orbit. Decommission (15% salvage) and re-launch with a new design to meet polar contracts.",
  },
  {
    title: "Transfer windows",
    body: "Beyond Earth: research Deep Space Operations, pick a destination in the Transfer tab, and wait for the window — planets align on their own schedule (Mars: every ~26 months). The injection is bought from your launch provider in cash.",
  },
  {
    title: "Cruise & capture",
    body: "During the cruise the craft hibernates at 10% ops cost. Propellant for the capture burn is reserved before departure, so check the Δv budget first. Warp to arrival — capture is automatic.",
  },
  {
    title: "Career path & victory",
    body: "Programs are the chapters of one career: finish a body's contracts, buy into the next program, push to Europa. Complete every program for grand victory. Sandbox Lab is a separate free-play mode. Press ? anytime for this guide.",
  },
];

export class Onboarding {
  private step = 0;
  private single: { title: string; body: string } | null = null;
  private readonly root: HTMLElement;
  private untrap: (() => void) | null = null;

  constructor() {
    this.root = document.getElementById("onboarding")!;
    this.root
      .querySelector("#onboarding-skip")
      ?.addEventListener("click", () => {
        this.dismiss();
      });
    this.root
      .querySelector("#onboarding-next")
      ?.addEventListener("click", () => {
        this.next();
      });
    this.root
      .querySelector("#onboarding-back")
      ?.addEventListener("click", () => {
        this.back();
      });
  }

  shouldShow(): boolean {
    return localStorage.getItem(STORAGE_KEY) !== "1";
  }

  show(force = false): void {
    if (!force && !this.shouldShow()) return;
    this.single = null;
    this.step = 0;
    this.render();
    this.open();
  }

  /** One contextual card (first-time triggers), outside the deck flow. */
  showSingle(title: string, body: string): void {
    this.single = { title, body };
    this.render();
    this.open();
  }

  private open(): void {
    this.root.classList.remove("hidden");
    this.untrap?.();
    this.untrap = trapFocus(this.root);
  }

  private close(): void {
    this.root.classList.add("hidden");
    this.untrap?.();
    this.untrap = null;
    releaseFocus();
  }

  private dismiss(): void {
    localStorage.setItem(STORAGE_KEY, "1");
    this.close();
  }

  private next(): void {
    if (this.single) {
      this.single = null;
      this.close();
      return;
    }
    if (this.step >= STEPS.length - 1) {
      this.dismiss();
      return;
    }
    this.step += 1;
    this.render();
    this.root.querySelector<HTMLButtonElement>("#onboarding-next")?.focus();
  }

  private back(): void {
    if (this.step <= 0) return;
    this.step -= 1;
    this.render();
    this.root.querySelector<HTMLButtonElement>("#onboarding-next")?.focus();
  }

  private render(): void {
    if (this.single) {
      this.root.querySelector("#onboarding-title")!.textContent =
        this.single.title;
      this.root.querySelector("#onboarding-body")!.textContent =
        this.single.body;
      this.root.querySelector("#onboarding-progress")!.textContent = "";
      this.root.querySelector<HTMLElement>("#onboarding-shortcuts")?.remove();
      const backBtn = this.root.querySelector<HTMLButtonElement>(
        "#onboarding-back",
      )!;
      const nextBtn = this.root.querySelector<HTMLButtonElement>(
        "#onboarding-next",
      )!;
      backBtn.disabled = true;
      nextBtn.textContent = "Got it";
      return;
    }
    const current = STEPS[this.step]!;
    const title = this.root.querySelector("#onboarding-title")!;
    const body = this.root.querySelector("#onboarding-body")!;
    const progress = this.root.querySelector("#onboarding-progress")!;
    const backBtn =
      this.root.querySelector<HTMLButtonElement>("#onboarding-back")!;
    const nextBtn =
      this.root.querySelector<HTMLButtonElement>("#onboarding-next")!;

    title.textContent = current.title;
    body.textContent = current.body;
    progress.textContent = `Step ${this.step + 1} / ${STEPS.length}`;
    backBtn.disabled = this.step === 0;
    nextBtn.textContent =
      this.step >= STEPS.length - 1 ? "Start playing" : "Next";

    let shortcutsEl = this.root.querySelector<HTMLElement>(
      "#onboarding-shortcuts",
    );
    if (!shortcutsEl) {
      shortcutsEl = document.createElement("p");
      shortcutsEl.id = "onboarding-shortcuts";
      shortcutsEl.className = "hint";
      body.insertAdjacentElement("afterend", shortcutsEl);
    }
    if (this.step >= STEPS.length - 1) {
      shortcutsEl.textContent =
        "Shortcuts: Space pause/resume · +/− speed · 1–5 tabs · D downlink · M map · ? help";
      shortcutsEl.hidden = false;
    } else {
      shortcutsEl.hidden = true;
    }
  }
}
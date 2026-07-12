const STORAGE_KEY = "satelliteinc_onboarding_done";

const STEPS = [
  {
    title: "Welcome to Satellite Inc",
    body: "Build a space company: design satellites, launch to orbit, and fulfill imaging contracts. Complete all five contracts to win the campaign.",
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
    body: "Operating costs money every sim-day — idling drains cash. If cash falls below −$50k, you're bankrupt. Keep a contract in the pipeline.",
  },
  {
    title: "Maneuver",
    body: "Use the Orbit tab to change altitude with a Hohmann transfer: burn 1 at one apsis, coast, burn 2 at the other. The HUD counts down to the next apsis.",
  },
  {
    title: "Handle anomalies",
    body: "Anomalies drain extra power until handled. Queue Clear anomaly, Reboot comms, or Safe mode at a station — they execute at the next pass.",
  },
  {
    title: "Redesign when needed",
    body: "Inclination can't change on orbit. Decommission (15% salvage) and re-launch with a new design to meet polar contracts.",
  },
  {
    title: "Grow and win",
    body: "Earn cash and reputation, invest in R&D, unlock buses. Complete every contract at least once to win. Press ? anytime to reopen this guide.",
  },
];

export class Onboarding {
  private step = 0;
  private readonly root: HTMLElement;

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
    this.step = 0;
    this.render();
    this.root.classList.remove("hidden");
  }

  private dismiss(): void {
    localStorage.setItem(STORAGE_KEY, "1");
    this.root.classList.add("hidden");
  }

  private next(): void {
    if (this.step >= STEPS.length - 1) {
      this.dismiss();
      return;
    }
    this.step += 1;
    this.render();
  }

  private back(): void {
    if (this.step <= 0) return;
    this.step -= 1;
    this.render();
  }

  private render(): void {
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
  }
}

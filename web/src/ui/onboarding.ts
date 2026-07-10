const STORAGE_KEY = "satelliteinc_onboarding_done";

const STEPS = [
  {
    title: "Welcome to Satellite Inc",
    body: "Build a space company: design satellites, launch to orbit, and fulfill imaging contracts.",
  },
  {
    title: "Design & contract",
    body: "Pick a bus, set altitude and inclination, then accept a contract from the marketplace.",
  },
  {
    title: "Launch",
    body: "Launch when ready. Your satellite enters a real Keplerian orbit simulated in Rust/WASM.",
  },
  {
    title: "Operate",
    body: "Collect imagery in sunlight. During a ground-station pass, downlink data to complete the contract.",
  },
  {
    title: "Grow",
    body: "Earn cash and reputation, invest in R&D, unlock new buses, and take on harder contracts.",
  },
];

export class Onboarding {
  private step = 0;
  private readonly root: HTMLElement;

  constructor() {
    this.root = document.getElementById("onboarding")!;
    this.root.querySelector("#onboarding-skip")?.addEventListener("click", () => this.dismiss());
    this.root.querySelector("#onboarding-next")?.addEventListener("click", () => this.next());
    this.root.querySelector("#onboarding-back")?.addEventListener("click", () => this.back());
  }

  shouldShow(): boolean {
    return localStorage.getItem(STORAGE_KEY) !== "1";
  }

  show(): void {
    if (!this.shouldShow()) return;
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
    const backBtn = this.root.querySelector("#onboarding-back") as HTMLButtonElement;
    const nextBtn = this.root.querySelector("#onboarding-next") as HTMLButtonElement;

    title.textContent = current.title;
    body.textContent = current.body;
    progress.textContent = `Step ${this.step + 1} / ${STEPS.length}`;
    backBtn.disabled = this.step === 0;
    nextBtn.textContent = this.step >= STEPS.length - 1 ? "Start playing" : "Next";
  }
}
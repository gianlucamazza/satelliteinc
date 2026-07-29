export type LoadingStage = "wasm" | "sim" | "world" | "ready";

const STAGES: Record<LoadingStage, { pct: number; message: string }> = {
  wasm: { pct: 15, message: "Loading simulation core…" },
  sim: { pct: 40, message: "Parsing game content…" },
  world: { pct: 70, message: "Preparing globe…" },
  ready: { pct: 100, message: "Ready" },
};

export class LoadingController {
  private readonly root: HTMLElement;
  private readonly stageEl: HTMLElement;
  private readonly fillEl: HTMLElement;

  constructor() {
    this.root = document.getElementById("loading")!;
    this.stageEl = document.getElementById("loading-stage")!;
    this.fillEl = document.getElementById("loading-bar-fill")!;
  }

  setStage(stage: LoadingStage): void {
    const { pct, message } = STAGES[stage];
    this.stageEl.textContent = message;
    this.fillEl.style.width = `${pct}%`;
  }

  hide(): void {
    this.setStage("ready");
    this.root.classList.add("hidden");
  }
}
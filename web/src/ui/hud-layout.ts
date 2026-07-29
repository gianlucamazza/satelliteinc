const STORAGE_KEY = "satelliteinc_hud_collapsed";
const TABLET_BREAKPOINT = 768;

export class HudLayout {
  private readonly hud: HTMLElement;
  private readonly toggle: HTMLButtonElement;
  private readonly topbar: HTMLElement | null;
  private collapsed: boolean;
  private readonly observer: ResizeObserver;

  constructor() {
    this.hud = document.getElementById("hud")!;
    this.toggle = document.getElementById("hud-toggle") as HTMLButtonElement;
    this.topbar = document.getElementById("hud-topbar");

    const saved = localStorage.getItem(STORAGE_KEY);
    this.collapsed =
      saved !== null ? saved === "1" : window.innerWidth <= TABLET_BREAKPOINT;

    this.toggle.addEventListener("click", (e) => {
      e.stopPropagation();
      this.setCollapsed(!this.collapsed);
    });
    this.hud.addEventListener("click", () => {
      if (this.collapsed) this.setCollapsed(false);
    });

    this.observer = new ResizeObserver(() => {
      this.syncChromeMetrics();
    });
    this.observer.observe(this.hud);
    if (this.topbar) this.observer.observe(this.topbar);
    window.addEventListener("resize", () => {
      this.syncChromeMetrics();
    });

    this.apply();
  }

  isCollapsed(): boolean {
    return this.collapsed;
  }

  private setCollapsed(value: boolean): void {
    this.collapsed = value;
    try {
      localStorage.setItem(STORAGE_KEY, value ? "1" : "0");
    } catch {
      // private mode — skip persist
    }
    this.apply();
  }

  private apply(): void {
    this.hud.classList.toggle("collapsed", this.collapsed);
    this.toggle.setAttribute("aria-expanded", String(!this.collapsed));
    this.toggle.setAttribute(
      "aria-label",
      this.collapsed ? "Expand mission control HUD" : "Collapse mission control HUD",
    );
    this.toggle.textContent = this.collapsed ? "▶" : "◀";
    this.syncChromeMetrics();
  }

  /** Publish dock/topbar metrics so CSS can keep chrome zones non-overlapping. */
  private syncChromeMetrics(): void {
    const right = Math.ceil(this.hud.getBoundingClientRect().right + 12);
    document.documentElement.style.setProperty("--hud-clip-right", `${right}px`);
    if (this.topbar) {
      const h = Math.ceil(this.topbar.getBoundingClientRect().height);
      if (h > 0) {
        document.documentElement.style.setProperty(
          "--topbar-height",
          `${Math.max(36, h)}px`,
        );
      }
    }
  }
}
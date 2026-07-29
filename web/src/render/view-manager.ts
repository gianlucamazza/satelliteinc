import type { Snapshot } from "../types.ts";
import type { BodyView } from "./body-view.ts";
import { prefersReducedMotion } from "./colors.ts";
import {
  SystemMapView,
  type SystemMapCallbacks,
} from "./system-map.ts";

export type ActiveView = "body" | "map";

/**
 * Routes updates/rendering between the Body View and the System Map.
 * The BodyView owns the WebGL/CSS2D renderers (it is the primary view);
 * the map renders its own scene through the same renderer, so switching
 * views costs nothing but a 200 ms fade.
 */
export class ViewManager {
  readonly map: SystemMapView;
  private active: ActiveView = "body";
  private readonly fadeEl: HTMLElement | null;
  private readonly toggleBtn: HTMLElement | null;

  constructor(
    private readonly bodyView: BodyView,
    canvas: HTMLCanvasElement,
    mapCallbacks: SystemMapCallbacks | null = null,
  ) {
    this.map = new SystemMapView(canvas, mapCallbacks);
    this.fadeEl = document.getElementById("view-fade");
    this.toggleBtn = document.getElementById("view-toggle");
    this.toggleBtn?.addEventListener("click", () => {
      this.toggle();
    });
    this.syncChrome();
  }

  get activeView(): ActiveView {
    return this.active;
  }

  toggle(): void {
    this.setActive(this.active === "body" ? "map" : "body");
  }

  setActive(view: ActiveView): void {
    if (view === this.active) return;
    // Fade through black: never lerp between projections.
    if (this.fadeEl && !prefersReducedMotion()) {
      this.fadeEl.classList.add("active");
      window.setTimeout(() => {
        this.fadeEl?.classList.remove("active");
      }, 220);
      window.setTimeout(() => {
        this.applyActive(view);
      }, 100);
    } else {
      this.applyActive(view);
    }
  }

  private applyActive(view: ActiveView): void {
    this.active = view;
    this.syncChrome();
    // After chrome/class toggles: one paint of the new view so CSS2D labels
    // for the inactive scene are not left mid-transform on screen.
    this.render();
  }

  private syncChrome(): void {
    this.bodyView.controls.enabled = this.active === "body";
    this.map.setActive(this.active === "map");
    // The CSS2D label layer is shared: scope which view's labels show.
    this.bodyView.labelRenderer.domElement.classList.toggle(
      "map-mode",
      this.active === "map",
    );
    // When leaving map mode, also scrub any map-label nodes that still have
    // an inline display from the last CSS2DRenderer pass.
    if (this.active === "body") {
      this.bodyView.labelRenderer.domElement
        .querySelectorAll(".map-label")
        .forEach((el) => {
          (el as HTMLElement).style.display = "none";
        });
    }
    if (this.toggleBtn) {
      this.toggleBtn.textContent =
        this.active === "body" ? "System map (M)" : "Body view (M)";
    }
    document
      .getElementById("globe-legend")
      ?.classList.toggle("map-hidden", this.active === "map");
  }

  update(snapshot: Snapshot, selectedCraftId: number): void {
    this.bodyView.update(snapshot, selectedCraftId);
    this.map.update(snapshot, selectedCraftId);
  }

  render(): void {
    if (this.active === "body") {
      this.bodyView.render();
    } else {
      this.map.render(this.bodyView.renderer, this.bodyView.labelRenderer);
    }
  }
}

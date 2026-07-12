import * as THREE from "three";
import { CSS2DObject } from "three/addons/renderers/CSS2DRenderer.js";
import type { CSS2DRenderer } from "three/addons/renderers/CSS2DRenderer.js";
import type { BodySnapshot, Snapshot } from "../types.ts";

// Radial power-law compression: angles stay TRUE (windows are about phase
// angles), radii are squashed so LEO and Jupiter fit one orthographic view.
// K normalizes Earth's orbit to 10 render units.
const COMPRESS_EXP = 0.45;
const K = 10 / Math.pow(1.495_982_612e8, COMPRESS_EXP);

const BODY_COLORS: Record<string, number> = {
  sun: 0xffcc44,
  earth: 0x4da3ff,
  moon: 0xaaaaaa,
  venus: 0xd9b078,
  mars: 0xff6644,
  jupiter: 0xc8a678,
};

function compress(rKm: number): number {
  return rKm <= 0 ? 0 : K * Math.pow(rKm, COMPRESS_EXP);
}

/** Heliocentric km → compressed map plane (render X-Z, true longitude). */
function toMap(posKm: [number, number, number]): THREE.Vector3 {
  const r = Math.hypot(posKm[0], posKm[1]);
  const theta = Math.atan2(posKm[1], posKm[0]);
  const cr = compress(r);
  // Same axis convention as the body view: ecliptic XY → render X-Z.
  return new THREE.Vector3(cr * Math.cos(theta), 0, cr * Math.sin(theta));
}

interface MapBody {
  marker: THREE.Mesh;
  hit: THREE.Mesh;
  label: CSS2DObject;
  orbitLine: THREE.LineLoop | null;
}

/**
 * Schematic top-down view of the solar system: sun at the origin, orbit
 * guides at compressed radii, bodies at their true ecliptic longitudes.
 */
export class SystemMapView {
  readonly scene: THREE.Scene;
  readonly camera: THREE.OrthographicCamera;

  private readonly bodies = new Map<string, MapBody>();
  private readonly raycaster = new THREE.Raycaster();
  private readonly pointer = new THREE.Vector2();
  private readonly tooltip: HTMLElement | null;
  private readonly infoCard: HTMLElement | null;
  private readonly dateChip: HTMLElement | null;
  private readonly focusRing: THREE.Mesh;
  private cruiseLine!: THREE.Line;
  private craftMarker!: THREE.Mesh;
  private cruiseKey = "";
  private selectedId: string | null = null;
  private lastSnapshot: Snapshot | null = null;
  private viewSize = 26;
  private zoom = 1;

  constructor(private readonly canvas: HTMLCanvasElement) {
    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x03060c);

    this.camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.1, 500);
    this.camera.position.set(0, 100, 0);
    this.camera.up.set(0, 0, -1);
    this.camera.lookAt(0, 0, 0);
    this.applyFrustum();

    this.scene.add(new THREE.AmbientLight(0xffffff, 1.2));

    // Ring highlighting the craft's current body.
    const ringGeo = new THREE.RingGeometry(0.55, 0.7, 48);
    const ringMat = new THREE.MeshBasicMaterial({
      color: 0xffdd44,
      side: THREE.DoubleSide,
      transparent: true,
      opacity: 0.9,
    });
    this.focusRing = new THREE.Mesh(ringGeo, ringMat);
    this.focusRing.rotation.x = -Math.PI / 2;
    this.focusRing.visible = false;
    this.scene.add(this.focusRing);

    const cruiseMat = new THREE.LineBasicMaterial({
      color: 0xffaa44,
      transparent: true,
      opacity: 0.9,
    });
    this.cruiseLine = new THREE.Line(new THREE.BufferGeometry(), cruiseMat);
    this.cruiseLine.visible = false;
    this.scene.add(this.cruiseLine);
    this.craftMarker = new THREE.Mesh(
      new THREE.SphereGeometry(0.22, 12, 12),
      new THREE.MeshBasicMaterial({ color: 0xffdd44 }),
    );
    this.craftMarker.visible = false;
    this.scene.add(this.craftMarker);

    this.tooltip = document.getElementById("map-tooltip");
    this.infoCard = document.getElementById("map-infocard");
    this.dateChip = document.getElementById("sim-date");

    canvas.addEventListener("pointermove", (e) => {
      this.onPointerMove(e);
    });
    canvas.addEventListener("pointerdown", (e) => {
      this.onPointerDown(e);
    });
    canvas.addEventListener(
      "wheel",
      (e) => {
        this.onWheel(e);
      },
      { passive: false },
    );
    window.addEventListener("resize", () => {
      this.applyFrustum();
    });
  }

  private applyFrustum(): void {
    const aspect = window.innerWidth / window.innerHeight;
    const size = this.viewSize / this.zoom;
    this.camera.left = -size * aspect;
    this.camera.right = size * aspect;
    this.camera.top = size;
    this.camera.bottom = -size;
    this.camera.updateProjectionMatrix();
  }

  private onWheel(e: WheelEvent): void {
    if (!this.activeFlag) return;
    e.preventDefault();
    this.zoom = Math.min(
      6,
      Math.max(0.5, this.zoom * (e.deltaY < 0 ? 1.15 : 0.87)),
    );
    this.applyFrustum();
  }

  private activeFlag = false;
  setActive(active: boolean): void {
    this.activeFlag = active;
    if (!active) {
      this.tooltip?.classList.add("hidden");
      this.infoCard?.classList.add("hidden");
      this.dateChip?.classList.add("hidden");
    } else {
      this.dateChip?.classList.remove("hidden");
      if (this.selectedId) this.infoCard?.classList.remove("hidden");
    }
  }

  private hitBody(event: PointerEvent): string | null {
    const rect = this.canvas.getBoundingClientRect();
    this.pointer.x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
    this.pointer.y = -((event.clientY - rect.top) / rect.height) * 2 + 1;
    this.raycaster.setFromCamera(this.pointer, this.camera);
    const hits = this.raycaster.intersectObjects(
      [...this.bodies.values()].map((b) => b.hit),
      false,
    );
    return (hits[0]?.object.userData.bodyId as string | undefined) ?? null;
  }

  private onPointerMove(event: PointerEvent): void {
    if (!this.activeFlag || !this.tooltip) return;
    const id = this.hitBody(event);
    this.canvas.style.cursor = id ? "pointer" : "";
    if (!id || !this.lastSnapshot) {
      this.tooltip.classList.add("hidden");
      return;
    }
    const body = this.lastSnapshot.bodies.find((b) => b.id === id);
    if (!body) return;
    const au =
      Math.hypot(body.position_helio_km[0], body.position_helio_km[1]) /
      1.495_978_707e8;
    this.tooltip.textContent =
      body.id === "sun" ? body.name : `${body.name} — ${au.toFixed(2)} AU`;
    this.tooltip.style.left = `${event.clientX + 14}px`;
    this.tooltip.style.top = `${event.clientY + 10}px`;
    this.tooltip.classList.remove("hidden");
  }

  private onPointerDown(event: PointerEvent): void {
    if (!this.activeFlag) return;
    this.selectedId = this.hitBody(event);
    this.renderInfoCard();
  }

  private renderInfoCard(): void {
    if (!this.infoCard) return;
    const snap = this.lastSnapshot;
    const body = snap?.bodies.find((b) => b.id === this.selectedId);
    if (!snap || !body) {
      this.infoCard.classList.add("hidden");
      return;
    }
    const au =
      Math.hypot(body.position_helio_km[0], body.position_helio_km[1]) /
      1.495_978_707e8;
    const kmToEarth = (() => {
      const e = snap.bodies.find((b) => b.id === "earth");
      if (!e || body.id === "earth") return null;
      return Math.hypot(
        body.position_helio_km[0] - e.position_helio_km[0],
        body.position_helio_km[1] - e.position_helio_km[1],
      );
    })();
    const lightMin = kmToEarth == null ? null : kmToEarth / 299_792.458 / 60;
    const rows = [
      `<h3>${body.name}</h3>`,
      body.id === "sun" ? "" : `<div>Sun distance: ${au.toFixed(3)} AU</div>`,
      lightMin == null
        ? ""
        : `<div>Light time to Earth: ${lightMin < 1 ? `${(lightMin * 60).toFixed(1)} s` : `${lightMin.toFixed(1)} min`}</div>`,
      body.soi_radius_km == null
        ? ""
        : `<div>SOI: ${(body.soi_radius_km / 1000).toFixed(0)}k km</div>`,
      snap.spacecraft[0]?.body_id === body.id
        ? `<div class="focus">Your spacecraft operates here</div>`
        : "",
    ];
    this.infoCard.innerHTML = rows.join("");
    this.infoCard.classList.remove("hidden");
  }

  private ensureBody(body: BodySnapshot): MapBody {
    let entry = this.bodies.get(body.id);
    if (entry) return entry;

    const color = BODY_COLORS[body.id] ?? 0x8899aa;
    const radius = body.id === "sun" ? 0.7 : 0.35;
    const marker = new THREE.Mesh(
      new THREE.SphereGeometry(radius, 20, 20),
      new THREE.MeshBasicMaterial({ color }),
    );
    const hit = new THREE.Mesh(
      new THREE.SphereGeometry(1.3, 8, 8),
      new THREE.MeshBasicMaterial({ visible: false }),
    );
    hit.userData.bodyId = body.id;

    const labelDiv = document.createElement("div");
    labelDiv.className = "map-label";
    labelDiv.textContent = body.name;
    const label = new CSS2DObject(labelDiv);
    label.position.set(0, 0, -1.1);

    // Orbit guide: a circle at the compressed mean radius around the parent.
    // Only sun-orbiting bodies get one (the Moon's would hug Earth's dot).
    let orbitLine: THREE.LineLoop | null = null;
    if (body.parent_id === "sun") {
      const meanR = Math.hypot(
        body.position_helio_km[0],
        body.position_helio_km[1],
      );
      const cr = compress(meanR);
      const pts: THREE.Vector3[] = [];
      for (let i = 0; i < 128; i++) {
        const a = (i / 128) * Math.PI * 2;
        pts.push(new THREE.Vector3(cr * Math.cos(a), 0, cr * Math.sin(a)));
      }
      orbitLine = new THREE.LineLoop(
        new THREE.BufferGeometry().setFromPoints(pts),
        new THREE.LineBasicMaterial({
          color: 0x334466,
          transparent: true,
          opacity: 0.8,
        }),
      );
      this.scene.add(orbitLine);
    }

    marker.add(hit, label);
    this.scene.add(marker);
    entry = { marker, hit, label, orbitLine };
    this.bodies.set(body.id, entry);
    return entry;
  }

  update(snapshot: Snapshot, selectedCraftId = 0): void {
    this.lastSnapshot = snapshot;
    const selected =
      snapshot.spacecraft.find((c) => c.id === selectedCraftId) ??
      snapshot.spacecraft[0] ??
      null;
    const craftBodyId = selected?.body_id ?? null;

    for (const body of snapshot.bodies) {
      const entry = this.ensureBody(body);
      if (body.id === "sun") {
        entry.marker.position.set(0, 0, 0);
      } else if (body.parent_id !== "sun") {
        // Satellite of a planet (the Moon): true phase angle around its
        // parent at an exaggerated fixed offset so it stays selectable.
        const parent = snapshot.bodies.find((b) => b.id === body.parent_id);
        const parentPos = parent
          ? toMap(parent.position_helio_km)
          : new THREE.Vector3();
        const rel = body.position_parent_km;
        const theta = Math.atan2(rel[1], rel[0]);
        entry.marker.position.set(
          parentPos.x + 1.0 * Math.cos(theta),
          0,
          parentPos.z + 1.0 * Math.sin(theta),
        );
      } else {
        entry.marker.position.copy(toMap(body.position_helio_km));
      }
      if (body.id === craftBodyId) {
        this.focusRing.visible = true;
        this.focusRing.position.copy(entry.marker.position);
      }
    }
    this.focusRing.visible = craftBodyId != null;

    // Cruise: craft marker + transfer arc in heliocentric map space. The
    // craft's frame coordinates are offset by the frame body's position
    // (zero when the frame IS the sun).
    const cruising = snapshot.spacecraft.find(
      (c) => c.phase === "Transfer" && c.transfer,
    );
    const craft =
      selected?.phase === "Transfer" && selected.transfer
        ? selected
        : (cruising ?? null);
    const transfer = craft?.transfer ?? null;
    if (craft && transfer && craft.phase === "Transfer") {
      const frame = snapshot.bodies.find(
        (b) => b.id === transfer.frame_body_id,
      );
      const frameHelio: [number, number, number] =
        frame && frame.id !== "sun" ? frame.position_helio_km : [0, 0, 0];
      const toHelio = (
        p: [number, number, number],
      ): [number, number, number] => [
        p[0] + frameHelio[0],
        p[1] + frameHelio[1],
        p[2] + frameHelio[2],
      ];
      this.craftMarker.position.copy(toMap(toHelio(craft.position_eci_km)));
      this.craftMarker.visible = true;
      const key = `${transfer.target_body_id}|${transfer.departure_sim_seconds}`;
      if (key !== this.cruiseKey && craft.orbit_trail_km.length > 1) {
        this.cruiseKey = key;
        const pts = craft.orbit_trail_km.map((p) => toMap(toHelio(p)));
        this.cruiseLine.geometry.dispose();
        this.cruiseLine.geometry = new THREE.BufferGeometry().setFromPoints(
          pts,
        );
      }
      this.cruiseLine.visible = true;
      this.focusRing.visible = false;
    } else {
      this.craftMarker.visible = false;
      this.cruiseLine.visible = false;
      this.cruiseKey = "";
    }

    if (this.dateChip && this.activeFlag) {
      const t = Date.parse(snapshot.epoch_utc) + snapshot.sim_seconds * 1000;
      this.dateChip.textContent = new Date(t).toISOString().slice(0, 10);
    }
    if (this.activeFlag && this.selectedId) this.renderInfoCard();
  }

  render(renderer: THREE.WebGLRenderer, labelRenderer: CSS2DRenderer): void {
    renderer.render(this.scene, this.camera);
    labelRenderer.render(this.scene, this.camera);
  }
}

import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import {
  CSS2DRenderer,
  CSS2DObject,
} from "three/addons/renderers/CSS2DRenderer.js";
import type { BodySnapshot, Snapshot } from "../types.ts";
import {
  bodyRadiusRender,
  eciKmToRender,
  stationLatLonToRender,
} from "./coords.ts";
import { bodyStyle } from "./body-specs.ts";
import { prefersReducedMotion, RENDER_COLORS } from "./colors.ts";
import { orbitTrailGeometry } from "./orbit-trail.ts";
import { designOrbitPoints } from "./orbit-preview.ts";
import {
  createSatelliteMesh,
  setSatelliteColor,
} from "./satellite-mesh.ts";
import { offsetAlongNormal, shouldShowLabel } from "./visibility.ts";

const STATION_PICK_RADIUS = 0.14;
const STATION_VIS_RADIUS = 0.05;

// Marker/label lifts as fractions of the body radius so they stay visually
// proportionate from the Moon (1737 km) to Jupiter (69911 km).
const MARKER_LIFT_FRACTION = 0.1 / 6.371;
const LABEL_LIFT_FRACTION = 0.28 / 6.371;

export interface BodyViewOptions {
  onStationSelect?: (stationId: string) => void;
  showTleRefs?: () => boolean;
}

interface RefSatObjects {
  dot: THREE.Mesh;
  label: CSS2DObject;
}

/**
 * Renders the spacecraft's current central body at the world origin.
 * `setBody` swaps the mesh, stations, and camera scale when the craft's
 * `body_id` changes; everything body-agnostic (satellite, trail, downlink
 * line, starfield, controls) survives the swap.
 */
export class BodyView {
  readonly scene: THREE.Scene;
  readonly camera: THREE.PerspectiveCamera;
  readonly renderer: THREE.WebGLRenderer;
  readonly labelRenderer: CSS2DRenderer;
  readonly controls: OrbitControls;

  private bodyMesh: THREE.Mesh | null = null;
  private bodyId = "";
  private radiusRender = bodyRadiusRender(6371);
  private readonly bodyFixed: THREE.Group;
  private readonly sunLight: THREE.DirectionalLight;
  private readonly orbitLine: THREE.Line;
  private readonly designOrbitLine: THREE.Line;
  private readonly downlinkLine: THREE.Line;
  private readonly satellite: THREE.Group;
  private readonly satMaterial: THREE.MeshBasicMaterial;
  private readonly fleetDots = new Map<number, THREE.Mesh>();
  private readonly stationMarkers: THREE.Group;
  private readonly stationLabels: THREE.Group;
  private readonly referenceSats: THREE.Group;
  private readonly starfield: THREE.Points;
  private readonly markerGeometry: THREE.SphereGeometry;
  private readonly stationPickGeometry: THREE.SphereGeometry;
  private readonly stationDots = new Map<string, THREE.Group>();
  private readonly stationLabelObjs = new Map<string, CSS2DObject>();
  private readonly refSats = new Map<number, RefSatObjects>();
  private readonly raycaster = new THREE.Raycaster();
  private readonly pointer = new THREE.Vector2();
  private readonly worldPos = new THREE.Vector3();
  private readonly options: BodyViewOptions;
  private orbitTrailKey = "";
  private designOrbitKey = "";
  private autoFollow = true;
  private userControlUntil = 0;
  private showTleRefs = true;

  constructor(canvas: HTMLCanvasElement, options: BodyViewOptions = {}) {
    this.options = options;
    this.showTleRefs = options.showTleRefs?.() ?? true;

    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(RENDER_COLORS.sceneBg);

    this.camera = new THREE.PerspectiveCamera(
      45,
      window.innerWidth / window.innerHeight,
      0.01,
      400,
    );
    this.camera.position.set(0, 0, 18);

    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setSize(window.innerWidth, window.innerHeight);
    this.renderer.sortObjects = true;

    this.labelRenderer = new CSS2DRenderer();
    this.labelRenderer.setSize(window.innerWidth, window.innerHeight);
    this.labelRenderer.domElement.className = "globe-labels";
    canvas.parentElement?.appendChild(this.labelRenderer.domElement);

    // Orbit on canvas; labels layer is pointer-transparent except station picks via raycast on canvas.
    this.controls = new OrbitControls(this.camera, canvas);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.08;
    this.controls.minDistance = 8;
    this.controls.maxDistance = 40;
    this.controls.addEventListener("start", () => {
      this.autoFollow = false;
      this.userControlUntil = performance.now() + 8000;
    });

    const ambient = new THREE.AmbientLight(RENDER_COLORS.ambient, 0.45);
    this.sunLight = new THREE.DirectionalLight(RENDER_COLORS.white, 1.6);
    this.sunLight.position.set(12, 4, 8);
    this.scene.add(ambient, this.sunLight);

    // Everything anchored to the ground (body mesh, station markers/labels)
    // lives in this group, rotated together each frame.
    this.bodyFixed = new THREE.Group();
    this.stationMarkers = new THREE.Group();
    this.stationLabels = new THREE.Group();
    this.bodyFixed.add(this.stationMarkers, this.stationLabels);
    this.scene.add(this.bodyFixed);

    const orbitMat = new THREE.LineBasicMaterial({
      color: RENDER_COLORS.accent,
      vertexColors: true,
      transparent: true,
      opacity: 0.85,
    });
    this.orbitLine = new THREE.Line(new THREE.BufferGeometry(), orbitMat);
    this.scene.add(this.orbitLine);

    const designMat = new THREE.LineDashedMaterial({
      color: RENDER_COLORS.accentLight,
      transparent: true,
      opacity: 0.5,
      dashSize: 0.15,
      gapSize: 0.1,
    });
    this.designOrbitLine = new THREE.Line(
      new THREE.BufferGeometry(),
      designMat,
    );
    this.designOrbitLine.visible = false;
    this.scene.add(this.designOrbitLine);

    const downlinkMat = new THREE.LineBasicMaterial({
      color: RENDER_COLORS.success,
      transparent: true,
      opacity: 0.85,
    });
    const downlinkGeo = new THREE.BufferGeometry();
    downlinkGeo.setAttribute(
      "position",
      new THREE.BufferAttribute(new Float32Array(6), 3),
    );
    this.downlinkLine = new THREE.Line(downlinkGeo, downlinkMat);
    this.downlinkLine.frustumCulled = false;
    this.downlinkLine.visible = false;
    this.scene.add(this.downlinkLine);

    this.satMaterial = new THREE.MeshBasicMaterial({
      color: RENDER_COLORS.satellite,
      depthTest: true,
    });
    this.satellite = createSatelliteMesh(this.satMaterial);
    this.satellite.visible = false;
    this.scene.add(this.satellite);

    this.markerGeometry = new THREE.SphereGeometry(STATION_VIS_RADIUS, 8, 8);
    this.stationPickGeometry = new THREE.SphereGeometry(
      STATION_PICK_RADIUS,
      10,
      10,
    );
    this.referenceSats = new THREE.Group();
    this.scene.add(this.referenceSats);

    this.starfield = this.createStarfield();
    this.scene.add(this.starfield);

    canvas.addEventListener("pointerdown", (e) => {
      this.onPointerDown(e);
    });
    canvas.addEventListener("pointermove", (e) => {
      this.onPointerMove(e);
    });
    window.addEventListener("resize", () => {
      this.onResize();
    });

    // One-time camera-controls hint.
    const hint = document.getElementById("camera-hint");
    if (hint && window.innerWidth <= 768) {
      hint.textContent = "Drag to orbit · Pinch to zoom";
    }
    if (hint && localStorage.getItem("satelliteinc_camera_hint") !== "1") {
      hint.classList.remove("hidden");
      canvas.addEventListener(
        "pointerdown",
        () => {
          hint.classList.add("hidden");
          localStorage.setItem("satelliteinc_camera_hint", "1");
        },
        { once: true },
      );
    }
  }

  setShowTleRefs(show: boolean): void {
    this.showTleRefs = show;
  }

  /** Preload a body's texture and mesh before the first snapshot. */
  async preloadBody(bodyId: string): Promise<void> {
    const style = bodyStyle(bodyId);
    const urls = [style.textureUrl, style.remoteFallbackUrl].filter(
      Boolean,
    ) as string[];

    await new Promise<void>((resolve) => {
      if (urls.length === 0) {
        resolve();
        return;
      }
      const loader = new THREE.TextureLoader();
      let idx = 0;
      const tryNext = () => {
        if (idx >= urls.length) {
          resolve();
          return;
        }
        const url = urls[idx++]!;
        loader.load(url, () => { resolve(); }, undefined, tryNext);
      };
      tryNext();
    });

    if (this.bodyId === bodyId) return;
    this.setBody({
      id: bodyId,
      name: bodyId,
      parent_id: null,
      radius_km: bodyId === "earth" ? 6371 : 1737,
      rotation_rad_s: 0,
      position_helio_km: [0, 0, 0],
      position_parent_km: [0, 0, 0],
      sun_direction: [1, 0, 0],
      soi_radius_km: null,
      texture: bodyId,
    });
  }

  private markerLift(): number {
    return this.radiusRender * MARKER_LIFT_FRACTION;
  }

  private labelLift(): number {
    return this.radiusRender * LABEL_LIFT_FRACTION;
  }

  /** Swap the rendered central body. Disposes the old mesh and stations. */
  private setBody(body: BodySnapshot): void {
    this.bodyId = body.id;
    this.radiusRender = bodyRadiusRender(body.radius_km);

    if (this.bodyMesh) {
      this.bodyFixed.remove(this.bodyMesh);
      this.bodyMesh.geometry.dispose();
      const material = this.bodyMesh.material as THREE.MeshPhongMaterial;
      material.map?.dispose();
      material.dispose();
      this.bodyMesh = null;
    }
    // Station markers belong to the previous body: rebuild from scratch.
    for (const group of this.stationDots.values()) {
      this.stationMarkers.remove(group);
      for (const child of group.children) {
        if (!(child instanceof THREE.Mesh)) continue;
        // Shared geometries (markerGeometry / stationPickGeometry) live on
        // the view — only dispose per-mesh materials here.
        const mat = child.material as THREE.Material | THREE.Material[];
        if (Array.isArray(mat)) {
          for (const m of mat) m.dispose();
        } else {
          mat.dispose();
        }
      }
    }
    for (const label of this.stationLabelObjs.values()) {
      this.stationLabels.remove(label);
      label.element.remove();
    }
    this.stationDots.clear();
    this.stationLabelObjs.clear();
    this.orbitTrailKey = "";
    this.designOrbitKey = "";

    const style = bodyStyle(body.texture);
    const geo = new THREE.SphereGeometry(this.radiusRender, 64, 64);
    const mat = new THREE.MeshPhongMaterial({
      color: style.fallbackColor,
      emissive: style.emissive,
      shininess: 18,
    });
    const applyTexture = (tex: THREE.Texture) => {
      mat.map = tex;
      // Texture carries the albedo; drop the flat tint.
      mat.color.set(RENDER_COLORS.white);
      mat.needsUpdate = true;
    };
    if (style.textureUrl) {
      const loader = new THREE.TextureLoader();
      loader.load(style.textureUrl, applyTexture, undefined, () => {
        if (style.remoteFallbackUrl) {
          loader.load(style.remoteFallbackUrl, applyTexture, undefined, () => {
            console.warn(`${body.id} texture unavailable — flat shading`);
          });
        } else {
          console.warn(`${body.id} texture unavailable — flat shading`);
        }
      });
    }
    this.bodyMesh = new THREE.Mesh(geo, mat);
    this.bodyMesh.renderOrder = 0;
    this.bodyFixed.add(this.bodyMesh);

    // Camera envelope scales with the body.
    this.controls.minDistance = this.radiusRender * 1.3;
    this.controls.maxDistance = this.radiusRender * 7;
    const distance = this.camera.position.length();
    if (
      distance < this.controls.minDistance ||
      distance > this.controls.maxDistance
    ) {
      this.camera.position.normalize().multiplyScalar(this.radiusRender * 2.8);
    }
    this.controls.target.set(0, 0, 0);
  }

  private createStarfield(): THREE.Points {
    const count = 2000;
    const positions = new Float32Array(count * 3);
    for (let i = 0; i < count; i++) {
      const r = 40 + Math.random() * 30;
      const theta = Math.random() * Math.PI * 2;
      const phi = Math.acos(2 * Math.random() - 1);
      positions[i * 3] = r * Math.sin(phi) * Math.cos(theta);
      positions[i * 3 + 1] = r * Math.sin(phi) * Math.sin(theta);
      positions[i * 3 + 2] = r * Math.cos(phi);
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    return new THREE.Points(
      geo,
      new THREE.PointsMaterial({
        color: RENDER_COLORS.white,
        size: 0.06,
        transparent: true,
        opacity: 0.7,
      }),
    );
  }

  private stationHit(event: PointerEvent): THREE.Object3D | undefined {
    const rect = this.renderer.domElement.getBoundingClientRect();
    this.pointer.x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
    this.pointer.y = -((event.clientY - rect.top) / rect.height) * 2 + 1;
    this.raycaster.setFromCamera(this.pointer, this.camera);
    const hit = this.raycaster.intersectObjects(
      this.stationMarkers.children,
      true,
    )[0]?.object;
    if (!hit) return undefined;
    let node: THREE.Object3D | null = hit;
    while (node) {
      if (node.userData.stationId) return node;
      node = node.parent;
    }
    return hit;
  }

  private onPointerDown(event: PointerEvent): void {
    const hit = this.stationHit(event);
    if (hit?.userData.stationId) {
      this.options.onStationSelect?.(hit.userData.stationId as string);
    }
  }

  private onPointerMove(event: PointerEvent): void {
    // Cheap hover affordance: pointer cursor over clickable station dots.
    this.renderer.domElement.style.cursor = this.stationHit(event)
      ? "pointer"
      : "";
  }

  private syncStations(
    snapshot: Snapshot,
    selected: Snapshot["spacecraft"][number] | null,
  ): void {
    const craftStatus = new Map(
      (selected?.station_status ?? []).map((st) => [st.station_id, st]),
    );
    for (const station of snapshot.ground_stations) {
      // Only stations sitting on the rendered body get surface markers.
      if (station.body_id !== this.bodyId) continue;
      let group = this.stationDots.get(station.id);
      let label = this.stationLabelObjs.get(station.id);
      if (!group || !label) {
        const surface = stationLatLonToRender(
          station.latitude_deg,
          station.longitude_deg,
          this.radiusRender,
        );
        const anchor = offsetAlongNormal(
          surface,
          this.radiusRender,
          this.markerLift(),
        );
        group = new THREE.Group();
        group.position.copy(anchor);
        group.userData.stationId = station.id;

        const visual = new THREE.Mesh(
          this.markerGeometry,
          new THREE.MeshBasicMaterial({ depthTest: true, depthWrite: false }),
        );
        visual.renderOrder = 2;
        group.add(visual);

        const pick = new THREE.Mesh(
          this.stationPickGeometry,
          new THREE.MeshBasicMaterial({ visible: false }),
        );
        group.add(pick);

        this.stationMarkers.add(group);
        this.stationDots.set(station.id, group);

        const labelDiv = document.createElement("div");
        labelDiv.textContent = station.name;
        label = new CSS2DObject(labelDiv);
        label.position.copy(
          offsetAlongNormal(surface, this.radiusRender, this.labelLift()),
        );
        this.stationLabels.add(label);
        this.stationLabelObjs.set(station.id, label);
      }
      // Locked dishes are visible but not selectable (tech gate).
      group.userData.stationId = station.unlocked ? station.id : undefined;
      const visual = group.children[0] as THREE.Mesh;
      const inPass = craftStatus.get(station.id)?.in_pass ?? false;
      const color = !station.unlocked
        ? RENDER_COLORS.muted
        : inPass
          ? RENDER_COLORS.success
          : RENDER_COLORS.stationIdle;
      (visual.material as THREE.MeshBasicMaterial).color.set(color);
      const labelEl = label.element as HTMLDivElement;
      labelEl.textContent = station.unlocked
        ? station.name
        : `${station.name} (locked)`;
      labelEl.className = `station-label${inPass ? " pass" : ""}${station.unlocked ? "" : " locked"}`;
      group.visible = shouldShowLabel(
        group.getWorldPosition(this.worldPos),
        this.camera,
      );
      label.visible = shouldShowLabel(
        label.getWorldPosition(this.worldPos),
        this.camera,
      );
    }
  }

  /** Small markers for every other craft orbiting the focused body. */
  private syncFleetDots(
    snapshot: Snapshot,
    selected: Snapshot["spacecraft"][number] | null,
  ): void {
    const seen = new Set<number>();
    for (const c of snapshot.spacecraft) {
      if (
        c.id === selected?.id ||
        c.body_id !== this.bodyId ||
        c.phase !== "OnOrbit"
      ) {
        continue;
      }
      seen.add(c.id);
      let dot = this.fleetDots.get(c.id);
      if (!dot) {
        dot = new THREE.Mesh(
          new THREE.SphereGeometry(0.06, 12, 12),
          new THREE.MeshBasicMaterial({ color: RENDER_COLORS.fleetDot }),
        );
        dot.renderOrder = 2;
        this.scene.add(dot);
        this.fleetDots.set(c.id, dot);
      }
      dot.position.copy(eciKmToRender(c.position_eci_km));
      dot.visible = shouldShowLabel(dot.position, this.camera, {
        occludeRadius: this.radiusRender,
      });
    }
    for (const [id, dot] of this.fleetDots) {
      if (!seen.has(id)) {
        this.scene.remove(dot);
        dot.geometry.dispose();
        (dot.material as THREE.Material).dispose();
        this.fleetDots.delete(id);
      }
    }
  }

  private syncReferenceSats(snapshot: Snapshot): void {
    // TLE reference sats are Earth-orbit by nature (SGP4).
    const showAny = this.showTleRefs && this.bodyId === "earth";
    const seen = new Set<number>();
    for (const ref of snapshot.reference_satellites) {
      seen.add(ref.norad_id);
      let entry = this.refSats.get(ref.norad_id);
      if (!entry) {
        const dot = new THREE.Mesh(
          this.markerGeometry,
          new THREE.MeshBasicMaterial({
            color: RENDER_COLORS.refSat,
            depthTest: true,
          }),
        );
        dot.renderOrder = 2;
        const labelDiv = document.createElement("div");
        labelDiv.className = "ref-label";
        labelDiv.textContent = ref.name;
        const label = new CSS2DObject(labelDiv);
        this.referenceSats.add(dot, label);
        entry = { dot, label };
        this.refSats.set(ref.norad_id, entry);
      }
      const refPos = eciKmToRender(ref.position_eci_km);
      const show =
        showAny &&
        shouldShowLabel(refPos, this.camera, {
          occludeRadius: this.radiusRender,
        });
      entry.dot.position.copy(refPos);
      entry.dot.visible = show;
      entry.label.position
        .copy(refPos)
        .add(refPos.clone().normalize().multiplyScalar(0.15));
      entry.label.visible = show;
    }
    for (const [id, entry] of this.refSats) {
      if (!seen.has(id)) {
        this.referenceSats.remove(entry.dot, entry.label);
        (entry.dot.material as THREE.Material).dispose();
        entry.label.element.remove();
        this.refSats.delete(id);
      }
    }
  }

  update(snapshot: Snapshot, selectedCraftId = 0): void {
    const selected =
      snapshot.spacecraft.find((c) => c.id === selectedCraftId) ??
      snapshot.spacecraft[0] ??
      null;
    const focusBodyId = selected?.body_id ?? "earth";
    const focusBody = snapshot.bodies.find((b) => b.id === focusBodyId);
    if (focusBody && focusBody.id !== this.bodyId) {
      this.setBody(focusBody);
    }

    // Rotation rate comes from the snapshot (single source of truth: the
    // core registry). Sign matches the ECI→render Y/Z swap so body-fixed
    // stations line up with the sim's pass geometry.
    this.bodyFixed.rotation.y =
      -snapshot.sim_seconds * (focusBody?.rotation_rad_s ?? 0);
    this.bodyFixed.updateMatrixWorld(true);

    // Light from the true per-body sun direction (same axis swap as ECI).
    if (focusBody) {
      const d = focusBody.sun_direction;
      this.sunLight.position.set(d[0] * 50, d[2] * 50, d[1] * 50);
    }

    this.syncStations(snapshot, selected);
    this.syncReferenceSats(snapshot);
    this.syncFleetDots(snapshot, selected);

    const craft = selected;
    this.downlinkLine.visible = false;

    if (!craft || craft.phase === "Design") {
      this.satellite.visible = false;
      this.orbitLine.visible = false;
      this.orbitTrailKey = "";
      if (craft?.phase === "Design") {
        const designKey = `${this.bodyId}|${craft.target_altitude_km}|${craft.target_inclination_deg}`;
        if (designKey !== this.designOrbitKey) {
          this.designOrbitKey = designKey;
          const pts = designOrbitPoints(
            this.radiusRender,
            craft.target_altitude_km,
            craft.target_inclination_deg,
          );
          this.designOrbitLine.geometry.dispose();
          this.designOrbitLine.geometry =
            new THREE.BufferGeometry().setFromPoints(pts);
          (
            this.designOrbitLine.material as THREE.LineDashedMaterial
          ).needsUpdate = true;
          this.designOrbitLine.computeLineDistances();
        }
        this.designOrbitLine.visible = true;
        this.controls.target.set(0, 0, 0);
      } else {
        this.designOrbitLine.visible = false;
      }
      return;
    }

    this.designOrbitLine.visible = false;
    this.designOrbitKey = "";

    const pos = eciKmToRender(craft.position_eci_km);
    this.satellite.visible = shouldShowLabel(pos, this.camera, {
      occludeRadius: this.radiusRender,
    });
    this.satellite.position.copy(pos);
    const satColor = craft.in_pass
      ? RENDER_COLORS.satellitePass
      : craft.downlink_active
        ? RENDER_COLORS.satelliteDownlink
        : RENDER_COLORS.satellite;
    setSatelliteColor(this.satellite, satColor);

    if (
      snapshot.phase === "victory" &&
      !prefersReducedMotion()
    ) {
      const pulse = 1 + 0.12 * Math.sin(performance.now() * 0.004);
      this.satellite.scale.setScalar(pulse);
    } else {
      this.satellite.scale.setScalar(1);
    }

    if (craft.orbit_trail_km.length > 1) {
      // The trail traces the osculating ellipse: rebuild only when the
      // orbit's shape actually changes, not every frame. RAAN/argp are
      // coarsened to 0.1° — J2 drifts them continuously and finer keys
      // would rebuild the trail every tick at high time accel.
      const el = craft.elements;
      const trailKey = el
        ? `${this.bodyId}|${el.semi_major_axis_km.toFixed(2)}|${el.eccentricity.toFixed(6)}|${el.inclination_deg.toFixed(3)}|${el.raan_deg.toFixed(1)}|${el.arg_perigee_deg.toFixed(1)}|${craft.pending_hohmann ? "xfer" : "nom"}`
        : String(craft.orbit_trail_km.length);
      if (trailKey !== this.orbitTrailKey) {
        this.orbitTrailKey = trailKey;
        const points = craft.orbit_trail_km.map((p) => eciKmToRender(p));
        const trailRgb = new THREE.Color(
          craft.pending_hohmann
            ? RENDER_COLORS.transfer
            : RENDER_COLORS.accent,
        );
        this.orbitLine.geometry.dispose();
        this.orbitLine.geometry = orbitTrailGeometry(points, trailRgb);
      }
      (this.orbitLine.material as THREE.LineBasicMaterial).color.set(
        RENDER_COLORS.white,
      );
      this.orbitLine.visible = true;
    }

    // Downlink beam: local dish → station marker; DSN deep-space → short
    // radial beam (Earth stations aren't on this body's surface).
    if (craft.downlink_active && craft.downlink_station_id) {
      const stLink = craft.station_status.find(
        (st) => st.station_id === craft.downlink_station_id,
      );
      const linkLive = stLink?.in_pass ?? craft.in_pass;
      if (linkLive) {
        const stationDot = this.stationDots.get(craft.downlink_station_id);
        const attr = this.downlinkLine.geometry.getAttribute(
          "position",
        ) as THREE.BufferAttribute;
        if (stationDot) {
          const stPos = stationDot.getWorldPosition(this.worldPos);
          attr.setXYZ(0, pos.x, pos.y, pos.z);
          attr.setXYZ(1, stPos.x, stPos.y, stPos.z);
          attr.needsUpdate = true;
          this.downlinkLine.visible = true;
        } else {
          // Deep-space / off-body dish: beam outward along craft radius.
          const len = Math.hypot(pos.x, pos.y, pos.z) || 1;
          const scale = 1 + (this.radiusRender * 0.35) / len;
          attr.setXYZ(0, pos.x, pos.y, pos.z);
          attr.setXYZ(1, pos.x * scale, pos.y * scale, pos.z * scale);
          attr.needsUpdate = true;
          this.downlinkLine.visible = true;
        }
      }
    }

    if (performance.now() >= this.userControlUntil) {
      this.autoFollow = true;
    }
    if (this.autoFollow) {
      this.controls.target.lerp(pos, 0.08);
      const camGoal = new THREE.Vector3(
        pos.x,
        pos.y + this.radiusRender * 0.47,
        pos.z + this.radiusRender * 1.26,
      );
      this.camera.position.lerp(camGoal, 0.05);
    }
  }

  render(): void {
    this.controls.update();
    this.renderer.render(this.scene, this.camera);
    this.labelRenderer.render(this.scene, this.camera);
  }

  private onResize(): void {
    const w = window.innerWidth;
    const h = window.innerHeight;
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setSize(w, h);
    this.labelRenderer.setSize(w, h);
  }
}

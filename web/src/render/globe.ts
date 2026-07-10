import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import {
  CSS2DRenderer,
  CSS2DObject,
} from "three/addons/renderers/CSS2DRenderer.js";
import type { Snapshot } from "../types.ts";
import {
  earthRadiusRender,
  eciKmToRender,
  stationLatLonToRender,
} from "./coords.ts";
import { designOrbitPoints } from "./orbit-preview.ts";
import { offsetAlongNormal, shouldShowLabel } from "./visibility.ts";

/** Earth sidereal rotation rate (rad/s) — must match the core's EARTH_ROTATION_RAD_S. */
const EARTH_ROTATION_RAD_S = 7.292_115_9e-5;

const MARKER_LIFT = 0.1;
const LABEL_LIFT = 0.28;

export interface GlobeOptions {
  onStationSelect?: (stationId: string) => void;
  showTleRefs?: () => boolean;
}

interface RefSatObjects {
  dot: THREE.Mesh;
  label: CSS2DObject;
}

export class GlobeRenderer {
  readonly scene: THREE.Scene;
  readonly camera: THREE.PerspectiveCamera;
  readonly renderer: THREE.WebGLRenderer;
  readonly labelRenderer: CSS2DRenderer;
  readonly controls: OrbitControls;

  private readonly earth: THREE.Mesh;
  private readonly earthFixed: THREE.Group;
  private readonly orbitLine: THREE.Line;
  private readonly designOrbitLine: THREE.Line;
  private readonly downlinkLine: THREE.Line;
  private readonly satellite: THREE.Mesh;
  private readonly stationMarkers: THREE.Group;
  private readonly stationLabels: THREE.Group;
  private readonly referenceSats: THREE.Group;
  private readonly starfield: THREE.Points;
  private readonly markerGeometry: THREE.SphereGeometry;
  private readonly stationDots = new Map<string, THREE.Mesh>();
  private readonly stationLabelObjs = new Map<string, CSS2DObject>();
  private readonly refSats = new Map<number, RefSatObjects>();
  private readonly raycaster = new THREE.Raycaster();
  private readonly pointer = new THREE.Vector2();
  private readonly worldPos = new THREE.Vector3();
  private readonly options: GlobeOptions;
  private orbitTrailKey = "";
  private designOrbitKey = "";
  private autoFollow = true;
  private userControlUntil = 0;
  private showTleRefs = true;

  constructor(canvas: HTMLCanvasElement, options: GlobeOptions = {}) {
    this.options = options;
    this.showTleRefs = options.showTleRefs?.() ?? true;

    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x050810);

    this.camera = new THREE.PerspectiveCamera(
      45,
      window.innerWidth / window.innerHeight,
      0.01,
      200,
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

    const ambient = new THREE.AmbientLight(0x334466, 0.45);
    const sun = new THREE.DirectionalLight(0xffffff, 1.6);
    sun.position.set(12, 4, 8);
    this.scene.add(ambient, sun);

    const earthGeo = new THREE.SphereGeometry(earthRadiusRender(), 64, 64);
    const earthMat = new THREE.MeshPhongMaterial({
      color: 0x4488cc,
      emissive: 0x0a1520,
      shininess: 18,
    });
    const loader = new THREE.TextureLoader();
    const applyEarthTexture = (tex: THREE.Texture) => {
      // The ECI→render mapping swaps Y/Z (a reflection), so mirror the
      // equirect texture in longitude to keep geography under the markers.
      tex.wrapS = THREE.RepeatWrapping;
      tex.repeat.x = -1;
      earthMat.map = tex;
      earthMat.needsUpdate = true;
    };
    loader.load(
      "/textures/earth_atmos_2048.jpg",
      applyEarthTexture,
      undefined,
      () => {
        loader.load(
          "https://raw.githubusercontent.com/mrdoob/three.js/r175/examples/textures/planets/earth_atmos_2048.jpg",
          applyEarthTexture,
          undefined,
          () => {
            console.warn(
              "Earth texture unavailable (local and remote) — using flat shading",
            );
          },
        );
      },
    );
    this.earth = new THREE.Mesh(earthGeo, earthMat);
    this.earth.renderOrder = 0;

    // Everything anchored to the ground (Earth mesh, station markers/labels)
    // lives in this group, rotated together each frame.
    this.earthFixed = new THREE.Group();
    this.stationMarkers = new THREE.Group();
    this.stationLabels = new THREE.Group();
    this.earthFixed.add(this.earth, this.stationMarkers, this.stationLabels);
    this.scene.add(this.earthFixed);

    const orbitMat = new THREE.LineBasicMaterial({
      color: 0x4da3ff,
      transparent: true,
      opacity: 0.75,
    });
    this.orbitLine = new THREE.Line(new THREE.BufferGeometry(), orbitMat);
    this.scene.add(this.orbitLine);

    const designMat = new THREE.LineDashedMaterial({
      color: 0x88bbff,
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
      color: 0x3dd68c,
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

    const satGeo = new THREE.SphereGeometry(0.08, 16, 16);
    const satMat = new THREE.MeshBasicMaterial({
      color: 0xffdd44,
      depthTest: true,
    });
    this.satellite = new THREE.Mesh(satGeo, satMat);
    this.satellite.visible = false;
    this.scene.add(this.satellite);

    this.markerGeometry = new THREE.SphereGeometry(0.05, 8, 8);
    this.referenceSats = new THREE.Group();
    this.scene.add(this.referenceSats);

    this.starfield = this.createStarfield();
    this.scene.add(this.starfield);

    canvas.addEventListener("pointerdown", (e) => this.onPointerDown(e));
    window.addEventListener("resize", () => this.onResize());
  }

  setShowTleRefs(show: boolean): void {
    this.showTleRefs = show;
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
        color: 0xffffff,
        size: 0.06,
        transparent: true,
        opacity: 0.7,
      }),
    );
  }

  private onPointerDown(event: PointerEvent): void {
    const rect = this.renderer.domElement.getBoundingClientRect();
    this.pointer.x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
    this.pointer.y = -((event.clientY - rect.top) / rect.height) * 2 + 1;
    this.raycaster.setFromCamera(this.pointer, this.camera);
    const hits = this.raycaster.intersectObjects(
      this.stationMarkers.children,
      false,
    );
    const hit = hits[0]?.object;
    if (hit?.userData.stationId) {
      this.options.onStationSelect?.(hit.userData.stationId as string);
    }
  }

  private syncStations(snapshot: Snapshot): void {
    for (const station of snapshot.ground_stations) {
      let dot = this.stationDots.get(station.id);
      let label = this.stationLabelObjs.get(station.id);
      if (!dot || !label) {
        const surface = stationLatLonToRender(
          station.latitude_deg,
          station.longitude_deg,
        );
        dot = new THREE.Mesh(
          this.markerGeometry,
          new THREE.MeshBasicMaterial({ depthTest: true, depthWrite: false }),
        );
        dot.position.copy(offsetAlongNormal(surface, MARKER_LIFT));
        dot.userData.stationId = station.id;
        dot.renderOrder = 2;
        this.stationMarkers.add(dot);
        this.stationDots.set(station.id, dot);

        const labelDiv = document.createElement("div");
        labelDiv.textContent = station.name;
        label = new CSS2DObject(labelDiv);
        label.position.copy(offsetAlongNormal(surface, LABEL_LIFT));
        this.stationLabels.add(label);
        this.stationLabelObjs.set(station.id, label);
      }
      (dot.material as THREE.MeshBasicMaterial).color.set(
        station.in_pass ? 0x3dd68c : 0xff4466,
      );
      (label.element as HTMLDivElement).className =
        `station-label${station.in_pass ? " pass" : ""}`;
      dot.visible = shouldShowLabel(
        dot.getWorldPosition(this.worldPos),
        this.camera,
      );
      label.visible = shouldShowLabel(
        label.getWorldPosition(this.worldPos),
        this.camera,
      );
    }
  }

  private syncReferenceSats(snapshot: Snapshot): void {
    const seen = new Set<number>();
    for (const ref of snapshot.reference_satellites) {
      seen.add(ref.norad_id);
      let entry = this.refSats.get(ref.norad_id);
      if (!entry) {
        const dot = new THREE.Mesh(
          this.markerGeometry,
          new THREE.MeshBasicMaterial({ color: 0xaa66ff, depthTest: true }),
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
        this.showTleRefs &&
        shouldShowLabel(refPos, this.camera, { occlude: true });
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
        this.refSats.delete(id);
      }
    }
  }

  update(snapshot: Snapshot): void {
    // Rotation sign matches the ECI→render Y/Z swap so ECEF-anchored stations
    // line up with the sim's pass geometry.
    this.earthFixed.rotation.y = -snapshot.sim_seconds * EARTH_ROTATION_RAD_S;
    this.earthFixed.updateMatrixWorld(true);

    this.syncStations(snapshot);
    this.syncReferenceSats(snapshot);

    const craft = snapshot.spacecraft;
    this.downlinkLine.visible = false;

    if (!craft || craft.phase === "Design") {
      this.satellite.visible = false;
      this.orbitLine.visible = false;
      this.orbitTrailKey = "";
      if (craft?.phase === "Design") {
        const designKey = `${craft.target_altitude_km}|${craft.target_inclination_deg}`;
        if (designKey !== this.designOrbitKey) {
          this.designOrbitKey = designKey;
          const pts = designOrbitPoints(
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

    if (craft.phase !== "OnOrbit") {
      this.satellite.visible = false;
      this.orbitLine.visible = false;
      return;
    }

    const pos = eciKmToRender(craft.position_eci_km);
    this.satellite.visible = shouldShowLabel(pos, this.camera, {
      occlude: true,
    });
    this.satellite.position.copy(pos);
    (this.satellite.material as THREE.MeshBasicMaterial).color.set(
      craft.in_pass ? 0x3dd68c : craft.downlink_active ? 0xff8844 : 0xffdd44,
    );

    if (craft.orbit_trail_km.length > 1) {
      // The trail traces the osculating ellipse: rebuild only when the
      // orbit's shape actually changes, not every frame.
      const el = craft.elements;
      const trailKey = el
        ? `${el.semi_major_axis_km.toFixed(2)}|${el.eccentricity.toFixed(6)}|${el.inclination_deg.toFixed(3)}|${el.raan_deg.toFixed(3)}|${el.arg_perigee_deg.toFixed(3)}`
        : String(craft.orbit_trail_km.length);
      if (trailKey !== this.orbitTrailKey) {
        this.orbitTrailKey = trailKey;
        const points = craft.orbit_trail_km.map((p) => eciKmToRender(p));
        this.orbitLine.geometry.dispose();
        this.orbitLine.geometry = new THREE.BufferGeometry().setFromPoints(
          points,
        );
      }
      (this.orbitLine.material as THREE.LineBasicMaterial).color.set(
        craft.pending_hohmann ? 0xff8844 : 0x4da3ff,
      );
      this.orbitLine.visible = true;
    }

    if (craft.downlink_active && craft.in_pass && craft.downlink_station_id) {
      const stationDot = this.stationDots.get(craft.downlink_station_id);
      if (stationDot) {
        const stPos = stationDot.getWorldPosition(this.worldPos);
        const attr = this.downlinkLine.geometry.getAttribute(
          "position",
        ) as THREE.BufferAttribute;
        attr.setXYZ(0, pos.x, pos.y, pos.z);
        attr.setXYZ(1, stPos.x, stPos.y, stPos.z);
        attr.needsUpdate = true;
        this.downlinkLine.visible = true;
      }
    }

    if (performance.now() >= this.userControlUntil) {
      this.autoFollow = true;
    }
    if (this.autoFollow) {
      this.controls.target.lerp(pos, 0.08);
      const camGoal = new THREE.Vector3(pos.x, pos.y + 3, pos.z + 8);
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

import * as THREE from "three";

function hudClipRightPx(): number {
  const hud = document.getElementById("hud");
  if (hud) {
    return hud.getBoundingClientRect().right + 12;
  }
  const parsed = parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue(
      "--hud-clip-right",
    ),
  );
  return Number.isFinite(parsed) ? parsed : 432;
}

/** True when the point lies on the hemisphere facing the camera. */
export function isFacingCamera(
  worldPos: THREE.Vector3,
  camera: THREE.Camera,
  minDot = 0.12,
): boolean {
  const normal = worldPos.clone().normalize();
  const viewDir = camera.position.clone().normalize();
  return normal.dot(viewDir) > minDot;
}

/**
 * True when a ray from camera to target intersects the focus body's sphere
 * (centered at the origin, radius in render units) before the target.
 */
export function isOccludedByBody(
  cameraPos: THREE.Vector3,
  target: THREE.Vector3,
  bodyRadiusRenderUnits: number,
): boolean {
  const dir = target.clone().sub(cameraPos);
  const dist = dir.length();
  if (dist < 1e-6) return false;
  dir.divideScalar(dist);

  const b = 2 * cameraPos.dot(dir);
  const c =
    cameraPos.lengthSq() - bodyRadiusRenderUnits * bodyRadiusRenderUnits;
  const disc = b * b - 4 * c;
  if (disc < 0) return false;
  const t = (-b - Math.sqrt(disc)) / 2;
  return t > 0.01 && t < dist - 0.01;
}

/** Radial offset from the body center along a surface direction. */
export function offsetAlongNormal(
  surfacePos: THREE.Vector3,
  bodyRadiusRenderUnits: number,
  height: number,
): THREE.Vector3 {
  return surfacePos
    .clone()
    .normalize()
    .multiplyScalar(bodyRadiusRenderUnits + height);
}

/** Hide labels that project into the HUD column on the left. */
export function isInHudColumn(
  worldPos: THREE.Vector3,
  camera: THREE.Camera,
  width = window.innerWidth,
): boolean {
  const projected = worldPos.clone().project(camera);
  const x = (projected.x * 0.5 + 0.5) * width;
  return x < hudClipRightPx() && projected.z < 1;
}

export function shouldShowLabel(
  worldPos: THREE.Vector3,
  camera: THREE.Camera,
  options: { occludeRadius?: number } = {},
): boolean {
  if (
    options.occludeRadius !== undefined &&
    isOccludedByBody(camera.position, worldPos, options.occludeRadius)
  ) {
    return false;
  }
  if (!isFacingCamera(worldPos, camera)) return false;
  if (isInHudColumn(worldPos, camera)) return false;
  return true;
}

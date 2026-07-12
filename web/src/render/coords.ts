import * as THREE from "three";

/** Render scale: 1 render unit = 1000 km. */
export const RENDER_SCALE_KM = 0.001;

export function kmToRender(km: number): number {
  return km * RENDER_SCALE_KM;
}

/** A body's radius in render units. */
export function bodyRadiusRender(radiusKm: number): number {
  return radiusKm * RENDER_SCALE_KM;
}

export function eciKmToRender(pos: [number, number, number]): THREE.Vector3 {
  // BCI: X through vernal equinox, Z north pole — map to Three.js Y-up
  return new THREE.Vector3(
    kmToRender(pos[0]),
    kmToRender(pos[2]),
    kmToRender(pos[1]),
  );
}

/** Ground-station position on a body (optionally raised above the surface). */
export function stationLatLonToRender(
  latDeg: number,
  lonDeg: number,
  bodyRadiusRenderUnits: number,
  heightAboveSurface = 0,
): THREE.Vector3 {
  const lat = (latDeg * Math.PI) / 180;
  const lon = (lonDeg * Math.PI) / 180;
  const r = bodyRadiusRenderUnits + heightAboveSurface;
  return new THREE.Vector3(
    r * Math.cos(lat) * Math.cos(lon),
    r * Math.sin(lat),
    r * Math.cos(lat) * Math.sin(lon),
  );
}

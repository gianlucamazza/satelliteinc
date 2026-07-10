import * as THREE from "three";

/** Earth radius in render units (1 unit = 1000 km). */
export const RENDER_SCALE_KM = 0.001;

const EARTH_RADIUS_RENDER = 6371 * RENDER_SCALE_KM; // 6.371 render units

export function kmToRender(km: number): number {
  return km * RENDER_SCALE_KM;
}

export function eciKmToRender(pos: [number, number, number]): THREE.Vector3 {
  // ECI: X through vernal equinox, Z north pole — map to Three.js Y-up
  return new THREE.Vector3(
    kmToRender(pos[0]),
    kmToRender(pos[2]),
    kmToRender(pos[1]),
  );
}

export function earthRadiusRender(): number {
  return EARTH_RADIUS_RENDER;
}

/** Ground-station position on the globe (optionally raised above the surface). */
export function stationLatLonToRender(
  latDeg: number,
  lonDeg: number,
  heightAboveSurface = 0,
): THREE.Vector3 {
  const lat = (latDeg * Math.PI) / 180;
  const lon = (lonDeg * Math.PI) / 180;
  const r = earthRadiusRender() + heightAboveSurface;
  return new THREE.Vector3(
    r * Math.cos(lat) * Math.cos(lon),
    r * Math.sin(lat),
    r * Math.cos(lat) * Math.sin(lon),
  );
}
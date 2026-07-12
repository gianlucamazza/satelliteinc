import * as THREE from "three";
import { kmToRender } from "./coords.ts";

/** Presentation-only circular orbit preview for design phase. */
export function designOrbitPoints(
  bodyRadiusRenderUnits: number,
  altitudeKm: number,
  inclinationDeg: number,
  samples = 96,
): THREE.Vector3[] {
  const r = bodyRadiusRenderUnits + kmToRender(altitudeKm);
  const inc = (inclinationDeg * Math.PI) / 180;
  const points: THREE.Vector3[] = [];
  for (let i = 0; i <= samples; i++) {
    const nu = (i / samples) * Math.PI * 2;
    const xo = r * Math.cos(nu);
    const yo = r * Math.sin(nu);
    // Matches the ECI→render mapping in coords.ts: an equatorial orbit (i=0)
    // lies flat in the render X-Z plane, tilting toward +Y with inclination.
    points.push(new THREE.Vector3(xo, yo * Math.sin(inc), yo * Math.cos(inc)));
  }
  return points;
}

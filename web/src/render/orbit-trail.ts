import * as THREE from "three";

/** Closed orbit polyline with vertex colors fading the tail (older → dimmer). */
export function orbitTrailGeometry(
  points: THREE.Vector3[],
  rgb: THREE.Color,
): THREE.BufferGeometry {
  const geo = new THREE.BufferGeometry().setFromPoints(points);
  const colors = new Float32Array(points.length * 3);
  const last = Math.max(1, points.length - 1);
  for (let i = 0; i < points.length; i++) {
    const fade = 0.28 + 0.72 * (i / last);
    colors[i * 3] = rgb.r * fade;
    colors[i * 3 + 1] = rgb.g * fade;
    colors[i * 3 + 2] = rgb.b * fade;
  }
  geo.setAttribute("color", new THREE.BufferAttribute(colors, 3));
  return geo;
}
import * as THREE from "three";

/** Simplified bus: central body + solar panels + comm stub. */
export function createSatelliteMesh(
  material: THREE.MeshBasicMaterial,
): THREE.Group {
  const craft = new THREE.Group();

  const bus = new THREE.Mesh(new THREE.BoxGeometry(0.1, 0.07, 0.07), material);
  craft.add(bus);

  const panelGeo = new THREE.BoxGeometry(0.14, 0.015, 0.055);
  const panelL = new THREE.Mesh(panelGeo, material);
  panelL.position.x = -0.12;
  const panelR = new THREE.Mesh(panelGeo, material);
  panelR.position.x = 0.12;
  craft.add(panelL, panelR);

  const comm = new THREE.Mesh(new THREE.BoxGeometry(0.02, 0.055, 0.02), material);
  comm.position.set(0, 0.055, 0);
  craft.add(comm);

  craft.userData.isSatellite = true;
  return craft;
}

/** Apply a state tint to every mesh in the craft group. */
export function setSatelliteColor(
  craft: THREE.Object3D,
  color: number,
): void {
  craft.traverse((obj) => {
    const mesh = obj as THREE.Mesh;
    if (!(mesh.material instanceof THREE.MeshBasicMaterial)) return;
    mesh.material.color.set(color);
  });
}
import * as THREE from "three";

/** Closed, low-poly torn steel: curled wall scraps and a buckled, ragged lid.
 * Unit bounds keep the instanced dimensions aligned with the simple colliders. */
export function barrelScrapGeometry(kind: "shell" | "lid"): THREE.BufferGeometry {
  const geometry =
    kind === "shell"
      ? new THREE.BoxGeometry(1, 1, 0.12, 4, 3, 1)
      : new THREE.CylinderGeometry(0.5, 0.5, 0.12, 10, 1);
  const positions = geometry.getAttribute("position");
  for (let i = 0; i < positions.count; i++) {
    const x = positions.getX(i);
    const y = positions.getY(i);
    const z = positions.getZ(i);
    if (kind === "shell") {
      positions.setXYZ(
        i,
        x * (0.83 + 0.17 * Math.cos(y * 19)),
        y + 0.07 * Math.sin(x * 23),
        z + 0.7 * x * x + 0.12 * Math.sin(y * 8 + x * 5),
      );
    } else {
      const angle = Math.atan2(z, x);
      const radius = 0.83 + 0.17 * Math.cos(angle * 5);
      positions.setXYZ(i, x * radius, y + 0.45 * Math.abs(x) - 0.18 * z, z * radius);
    }
  }
  geometry.computeBoundingBox();
  const size = geometry.boundingBox!.getSize(new THREE.Vector3());
  geometry.center();
  geometry.scale(1 / size.x, 1 / size.y, 1 / size.z);
  geometry.computeVertexNormals();
  return geometry;
}

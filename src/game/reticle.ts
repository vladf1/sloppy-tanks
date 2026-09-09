import * as THREE from "three";
import { put } from "./model-primitives";
/** Geometry lies on the aiming plane; materials remain accessible for reload/hit feedback. */
export function createReticle() {
  const crosshair = new THREE.Group();
  // Two-tone reticle stays legible over bright ground, tank paint and cover.
  const reticleMaterial = (color: number) =>
    new THREE.MeshBasicMaterial({
      color,
      side: THREE.DoubleSide,
      depthTest: false,
      depthWrite: false,
      transparent: true,
      toneMapped: false,
    });
  const outline = reticleMaterial(0x12263c);
  const ink = reticleMaterial(0xfff9da);
  const ring = (inner: number, outer: number, mat: THREE.Material, order: number) => {
    const mesh = new THREE.Mesh(new THREE.RingGeometry(inner, outer, 40), mat);
    mesh.rotation.x = -Math.PI / 2;
    mesh.renderOrder = order;
    crosshair.add(mesh);
  };
  ring(0.4, 0.64, outline, 50);
  ring(0.46, 0.57, ink, 51);
  for (const [x, z] of [
    [-0.83, 0],
    [0.83, 0],
    [0, -0.83],
    [0, 0.83],
  ]) {
    for (const back of [true, false]) {
      const length = back ? 0.43 : 0.31;
      const width = back ? 0.18 : 0.075;
      const mesh = new THREE.Mesh(
        new THREE.PlaneGeometry(x === 0 ? width : length, x === 0 ? length : width),
        back ? outline : ink,
      );
      mesh.rotation.x = -Math.PI / 2;
      mesh.renderOrder = back ? 50 : 51;
      put(crosshair, mesh, x, 0, z);
    }
  }
  const center = new THREE.Mesh(new THREE.CircleGeometry(0.075, 16), reticleMaterial(0xffdf38));
  center.rotation.x = -Math.PI / 2;
  center.renderOrder = 52;
  crosshair.add(center);
  crosshair.position.y = 1.05;
  return { crosshair, ink, center: center.material };
}

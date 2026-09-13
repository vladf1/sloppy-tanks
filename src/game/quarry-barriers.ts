import * as THREE from "three";
import { concreteWall } from "./concrete-surfaces";
import { harborBox } from "./harbor-surfaces";
import { cylinder, put } from "./model-primitives";
import { HEDGEHOG_BEAMS, TOOTH_TOP_SCALE } from "./quarry-barrier-shapes";

const teeth = new Map<string, THREE.BufferGeometry>();

export function dragonTooth(w: number, h: number, d: number): THREE.Mesh {
  const key = `${w}/${h}/${d}`;
  let geometry = teeth.get(key);
  if (!geometry) {
    geometry = new THREE.BoxGeometry(w, h, d);
    const p = geometry.getAttribute("position");
    for (let i = 0; i < p.count; i++) {
      const scale = p.getY(i) > 0 ? TOOTH_TOP_SCALE : 1;
      p.setXYZ(i, p.getX(i) * scale, p.getY(i), p.getZ(i) * scale);
    }
    geometry.computeVertexNormals();
    teeth.set(key, geometry);
  }
  const mesh = new THREE.Mesh(geometry, concreteWall(w, h, d).material);
  mesh.castShadow = mesh.receiveShadow = true;
  return mesh;
}

/** Three crossed I-sections with real flange depth and bolted connecting plates. */
export function steelHedgehog(group: THREE.Group): void {
  for (const beam of HEDGEHOG_BEAMS) {
    const assembly = new THREE.Group();
    put(assembly, harborBox(0.12, beam.length, 0.44, 0x726454));
    for (const x of [-0.22, 0.22]) {
      put(assembly, harborBox(0.1, beam.length, 0.52, 0x625b50), x);
    }
    assembly.rotation.set(beam.rx, 0, beam.rz);
    put(group, assembly, 0, 1.3, 0);
    // Bake assemblies into the cover's batch without retaining separate draw calls.
    assembly.updateMatrix();
    for (const mesh of [...assembly.children]) {
      mesh.applyMatrix4(assembly.matrix);
      group.add(mesh);
    }
    group.remove(assembly);
  }
  for (const z of [-0.29, 0.29]) {
    put(group, harborBox(0.64, 0.64, 0.08, 0x625b50), 0, 1.3, z);
    for (const x of [-0.2, 0.2]) {
      for (const y of [1.1, 1.5]) {
        const bolt = cylinder(0.055, 0.1, 0x999083, 6);
        bolt.rotation.x = Math.PI / 2;
        put(group, bolt, x, y, z);
      }
    }
  }
}

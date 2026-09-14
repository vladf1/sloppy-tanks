import * as THREE from "three";
import { batch } from "./batching";
import { harborBox } from "./harbor-surfaces";
import { box, cylinder, put } from "./model-primitives";

const PAINT = 0xb69a51;
const STEEL = 0x51504a;
const RUBBER = 0x343431;
const GLASS = 0x526c72;

// Reuse the existing scratched panel atlas, with its own material cache so
// excavator paint can weather independently of harbor props and the haul truck.
const excavatorPaint = new Map<number, THREE.MeshStandardMaterial>();
let paintWear: THREE.Texture | undefined;
function weatherExcavator(group: THREE.Group) {
  group.traverse((child) => {
    if (!(child instanceof THREE.Mesh) || !(child.material instanceof THREE.MeshStandardMaterial)) {
      return;
    }
    const color = child.material.color.getHex();
    if (color !== PAINT && color !== 0xd2c6a2) {
      return;
    }
    let material = excavatorPaint.get(color);
    if (!material) {
      if (!paintWear) {
        paintWear = new THREE.TextureLoader().load(
          `${import.meta.env?.BASE_URL ?? "/"}textures/tanks/armor-wear.webp`,
        );
        paintWear.colorSpace = THREE.SRGBColorSpace;
        paintWear.wrapS = paintWear.wrapT = THREE.RepeatWrapping;
        paintWear.anisotropy = 4;
      }
      material = new THREE.MeshStandardMaterial({
        color: color === PAINT ? 0xd3a33f : color,
        map: paintWear,
        bumpMap: paintWear,
        bumpScale: 0.045,
        roughness: 0.84,
        metalness: 0.18,
      });
      excavatorPaint.set(color, material);
    }
    child.material = material;
  });
}

function beam(group: THREE.Group, a: number[], b: number[], width: number, color: number) {
  const from = new THREE.Vector3(...a);
  const to = new THREE.Vector3(...b);
  const mesh = harborBox(width, from.distanceTo(to), width, color);
  mesh.position.copy(from).add(to).multiplyScalar(0.5);
  mesh.quaternion.setFromUnitVectors(new THREE.Vector3(0, 1, 0), to.sub(from).normalize());
  group.add(mesh);
}

function piston(group: THREE.Group, a: number[], b: number[]) {
  const from = new THREE.Vector3(...a);
  const to = new THREE.Vector3(...b);
  const direction = to.clone().sub(from);
  for (const [fraction, radius, color] of [
    [0.65, 0.21, STEEL],
    [1, 0.105, 0xb8b8af],
  ]) {
    const mesh = cylinder(radius, direction.length() * fraction, color, 10);
    mesh.position.copy(from).addScaledVector(direction, fraction / 2);
    mesh.quaternion.setFromUnitVectors(new THREE.Vector3(0, 1, 0), direction.clone().normalize());
    group.add(mesh);
  }
}

/** Parked excavator: steel tracks, glazed cab, articulated boom and bucket teeth.
 * All assemblies are static, merged into a handful of material batches. */
export function quarryExcavator(): THREE.Group {
  const group = new THREE.Group();
  group.name = "quarry-excavator";
  for (const z of [-2.25, 2.25]) {
    put(group, box(7.8, 1.55, 1.4, RUBBER, 0.3), 0, 0.8, z);
    for (const x of [-2.8, -1.4, 0, 1.4, 2.8]) {
      const wheel = cylinder(0.62, 1.48, STEEL, 12);
      wheel.rotation.x = Math.PI / 2;
      put(group, wheel, x, 0.82, z);
      const hub = cylinder(0.2, 1.52, 0x7b786c, 10);
      hub.rotation.x = Math.PI / 2;
      put(group, hub, x, 0.82, z);
    }
    for (let x = -3.4; x < 3.6; x += 0.48) {
      for (const y of [0.13, 1.52]) {
        put(group, harborBox(0.16, 0.1, 1.5, STEEL), x, y, z);
      }
    }
  }
  put(group, cylinder(1.7, 0.45, STEEL, 16), 0, 1.72, 0);
  put(group, harborBox(6.9, 1.65, 4.3, PAINT), -0.7, 2.6, 0);
  put(group, harborBox(2.1, 1.15, 4.1, PAINT), -3, 3.75, 0);
  // A dark radiator with a few broad fins reads at the gameplay camera distance.
  put(group, box(0.06, 0.85, 2.8, RUBBER, 0), -4.08, 3.75, 0);
  for (let z = -1.2; z <= 1.2; z += 0.3) {
    put(group, box(0.09, 0.83, 0.05, STEEL, 0), -4.12, 3.75, z);
  }
  put(group, cylinder(0.13, 1.3, RUBBER, 8), -2.9, 4.65, 1.45);
  put(group, harborBox(2.55, 2.65, 2.25, PAINT), 0.65, 4.55, -1.05);
  put(group, box(2.18, 1.85, 0.04, GLASS, 0), 0.65, 4.78, -2.19);
  put(group, box(0.04, 1.9, 1.88, GLASS, 0), 1.94, 4.78, -1.05);
  for (const x of [-0.47, 0.7, 1.78]) {
    put(group, harborBox(0.1, 2.1, 0.1, STEEL), x, 4.72, -2.24);
  }
  put(group, harborBox(2.85, 0.2, 2.55, 0xd2c6a2), 0.65, 5.97, -1.05);
  put(group, box(1.5, 0.18, 0.55, STEEL, 0), 0.65, 2.5, -2.6);
  put(group, cylinder(0.17, 0.3, 0xd18b35, 8), -0.15, 6.22, -1.1);
  // Two boom plates enclose visible pins and paired hydraulic rams.
  for (const z of [0.45, 1.35]) {
    beam(group, [1.2, 3.4, z], [6, 8.8, z], 0.68, PAINT);
    beam(group, [6, 8.8, z], [10.8, 3.3, z], 0.53, PAINT);
    piston(group, [1.6, 3.6, z + 0.15], [4.6, 7.3, z + 0.15]);
    piston(group, [5.8, 8.5, z - 0.12], [9.35, 5.45, z - 0.12]);
    beam(group, [10.8, 3.3, z], [11.7, 1.4, z], 0.27, STEEL);
  }
  for (const [x, y] of [
    [1.2, 3.4],
    [6, 8.8],
    [10.8, 3.3],
  ]) {
    const pin = cylinder(0.32, 1.6, STEEL, 12);
    pin.rotation.x = Math.PI / 2;
    put(group, pin, x, y, 0.9);
  }
  const bucket = new THREE.Group();
  put(bucket, harborBox(2.5, 0.16, 2.4, STEEL), 0, 0, 0);
  put(bucket, harborBox(0.18, 1.45, 2.4, STEEL), -1.16, 0.65, 0);
  for (const z of [-1.12, 1.12]) {
    put(bucket, harborBox(2.5, 1.25, 0.16, STEEL), 0, 0.57, z);
  }
  for (const z of [-0.9, -0.45, 0, 0.45, 0.9]) {
    put(bucket, harborBox(0.65, 0.18, 0.23, 0xaaa493), 1.48, 0, z);
  }
  bucket.rotation.z = -0.22;
  put(group, bucket, 11.5, 0.6, 0.9);
  weatherExcavator(group);
  batch(bucket);
  batch(group);
  return group;
}

export function quarryDumpTruck(): THREE.Group {
  const group = new THREE.Group();
  group.name = "quarry-haul-truck";
  put(group, harborBox(11.6, 0.6, 4.7, STEEL), 0, 1.8, 0);
  for (const z of [-2.6, 2.6]) {
    for (const x of [-3.8, 1.5, 3.6]) {
      const tire = cylinder(1.4, 1.15, RUBBER, 16);
      tire.rotation.x = Math.PI / 2;
      put(group, tire, x, 1.4, z);
      const hub = cylinder(0.65, 1.2, PAINT, 12);
      hub.rotation.x = Math.PI / 2;
      put(group, hub, x, 1.4, z);
    }
  }
  put(group, harborBox(3.3, 3.3, 4.45, PAINT), -3.9, 3.7, 0);
  put(group, box(0.05, 1.45, 3.85, GLASS, 0), -5.58, 4.45, 0);
  for (const side of [-1, 1]) {
    put(group, box(2.6, 1.45, 0.05, GLASS, 0), -3.9, 4.45, side * 2.25);
    put(group, harborBox(7.7, 2.6, 0.25, PAINT), 1.45, 4.2, side * 2.62);
    for (let x = -1.5; x <= 4.8; x += 1.5) {
      put(group, harborBox(0.18, 2.7, 0.16, PAINT), x, 4.2, side * 2.8);
    }
    put(group, box(0.12, 0.35, 0.55, 0xdfdac4, 0), -5.68, 3, side * 1.65);
  }
  put(group, harborBox(7.7, 0.3, 5.3, STEEL), 1.45, 2.85, 0);
  put(group, harborBox(0.25, 2.6, 5.3, PAINT), 5.2, 4.2, 0);
  put(group, harborBox(0.25, 3.2, 5.3, PAINT), -2.3, 4.5, 0);
  put(group, harborBox(3.5, 0.2, 5.3, PAINT), -3.95, 6, 0);
  put(group, harborBox(0.25, 0.6, 5.1, STEEL), -5.7, 2.25, 0);
  batch(group);
  return group;
}

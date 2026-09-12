import * as THREE from "three";
import { batch } from "./batching";
import { shippingContainer } from "./harbor-models";
import { harborBox, harborMaterial } from "./harbor-surfaces";
import { box, cylinder, material, put } from "./model-primitives";

/** Beam between authored endpoints: crane braces, rails, rigging and mooring lines. */
export function harborBeam(
  group: THREE.Group,
  a: number[],
  b: number[],
  width: number,
  color: number,
) {
  const from = new THREE.Vector3(...a);
  const to = new THREE.Vector3(...b);
  const beam = box(width, from.distanceTo(to), width, color, 0);
  beam.position.copy(from).add(to).multiplyScalar(0.5);
  beam.quaternion.setFromUnitVectors(new THREE.Vector3(0, 1, 0), to.sub(from).normalize());
  group.add(beam);
}

// Cross-sections form a tapered stern and pointed bow, with an inset lower hull.
const outline = [
  [-34, -4.8],
  [-30, -7],
  [24, -7],
  [31, -4.8],
  [36, 0],
  [31, 4.8],
  [24, 7],
  [-30, 7],
  [-34, 4.8],
];
function hullSection(bottom: number, top: number, lowerScale: number, color: number) {
  const positions: number[] = [];
  const indices: number[] = [];
  for (const [y, scale] of [
    [bottom, lowerScale],
    [top, 1],
  ]) {
    for (const [x, z] of outline) {
      positions.push(x * scale, y, z * scale);
    }
  }
  const n = outline.length;
  for (let i = 0; i < n; i++) {
    const j = (i + 1) % n;
    indices.push(i, i + n, j, j, i + n, j + n);
  }
  for (let i = 1; i < n - 1; i++) {
    indices.push(n, n + i + 1, n + i);
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  const flat = geometry.toNonIndexed();
  flat.computeVertexNormals();
  const p = flat.getAttribute("position");
  const uv = new Float32Array(p.count * 2);
  for (let i = 0; i < p.count; i++) {
    uv[i * 2] = (p.getX(i) + p.getZ(i)) / 5;
    uv[i * 2 + 1] = p.getY(i) / 5;
  }
  flat.setAttribute("uv", new THREE.BufferAttribute(uv, 2));
  geometry.dispose();
  return new THREE.Mesh(flat, harborMaterial("steel", color));
}

function containerShip(color: number, variant: number): THREE.Group {
  const ship = new THREE.Group();
  ship.name = "container-ship";
  ship.add(hullSection(-3.8, -1.3, 0.85, 0xa95443));
  ship.add(hullSection(-1.3, 2.6, 1, color));
  put(ship, harborBox(58, 0.2, 12.7, 0xbbbaa5), -2, 2.72, 0);
  const paint = [0xcb7d43, 0x3b9394, 0x6e8eae, 0xbaad7c, 0xa76155];
  for (let bay = 0; bay < 4; bay++) {
    for (const row of [-1, 1]) {
      const levels = bay === variant % 4 ? 1 : 2;
      for (let level = 0; level < levels; level++) {
        const cargo = new THREE.Group();
        shippingContainer(cargo, {
          w: 9.6,
          d: 4.5,
          h: 3.2,
          color: paint[(bay + level * 2 + variant + (row > 0 ? 1 : 0)) % paint.length],
        });
        // Bake cargo into its ship before batching the whole vessel.
        cargo.position.set(-14 + bay * 10.2, 2.85 + level * 3.27, row * 2.5);
        cargo.updateMatrix();
        for (const child of [...cargo.children]) {
          child.applyMatrix4(cargo.matrix);
          ship.add(child);
        }
      }
    }
  }
  // Accommodation block, wraparound bridge glazing, deck rails and twin exhausts.
  put(ship, harborBox(8, 7.5, 10.5, 0xf0e6c9), -26, 6.55, 0);
  put(ship, harborBox(9.3, 2.3, 11.4, 0xf0e6c9), -25.5, 11.35, 0);
  put(ship, box(9.7, 0.22, 11.8, 0xd9d4b9), -25.5, 12.62, 0);
  for (const side of [-1, 1]) {
    for (let i = 0; i < 6; i++) {
      put(ship, box(1.05, 1.15, 0.08, 0x335b6c, 0), -29.2 + i * 1.45, 11.4, side * 5.72);
    }
    put(ship, box(0.08, 1.15, 8.8, 0x335b6c, 0), -20.82, 11.4, 0);
    for (let i = 0; i < 3; i++) {
      put(ship, box(0.08, 1.1, 1.1, 0x526e75), -21.95, 5 + i * 1.7, side * 2.8);
    }
    harborBeam(ship, [-32, 3.65, side * 6.3], [25, 3.65, side * 6.3], 0.09, 0xd8d2b9);
    for (let x = -32; x < 26; x += 3) {
      harborBeam(ship, [x, 2.8, side * 6.3], [x, 3.65, side * 6.3], 0.08, 0xd8d2b9);
    }
    for (const x of [-30, 26]) {
      put(ship, cylinder(0.5, 0.35, 0x293e4a), x, 3.1, side * 4.5);
    }
    for (const x of [-22, 24]) {
      const lifering = new THREE.Mesh(
        new THREE.TorusGeometry(0.48, 0.12, 6, 12),
        material(0xe88b4a),
      );
      put(ship, lifering, x, 3.8, side * 6.42);
    }
  }
  put(ship, harborBox(2.8, 4.1, 3.2, 0xc57943), -27, 14.2, -1.7);
  for (const z of [-2.3, -1.1]) {
    put(ship, cylinder(0.43, 1.2, 0x2d4149), -27, 16.5, z);
  }
  harborBeam(ship, [-22, 12.7, 0], [-22, 18, 0], 0.1, 0xe6dec4);
  harborBeam(ship, [-22, 16.8, -2], [-22, 16.8, 2], 0.08, 0xe6dec4);
  put(ship, box(3.2, 0.18, 0.35, 0xe1d8bf), -22, 17.7, 0);
  // Bow windlass and anchor-chain guide.
  put(ship, cylinder(0.75, 0.8, 0x465558), 29, 3.1, 0);
  harborBeam(ship, [29, 3, 0], [34, 2.8, 0], 0.16, 0x687271);
  batch(ship);
  return ship;
}

function gantryCrane(): { group: THREE.Group; load: THREE.Group } {
  const group = new THREE.Group();
  group.name = "quay-crane";
  const gold = 0xe9b347;
  const dark = 0x394f5d;
  put(group, harborBox(14, 1.4, 9, 0xb9b9a7, "dock"), 0, -0.65, 0);
  for (const side of [-1, 1]) {
    put(group, harborBox(2, 0.7, 8, dark), side * 5, 0.5, 0);
    for (const z of [-2.8, 2.8]) {
      const wheel = cylinder(0.65, 0.6, 0x2b3940, 12);
      wheel.rotation.z = Math.PI / 2;
      put(group, wheel, side * 5, 0.6, z);
      harborBeam(group, [side * 5, 1, z], [side * 3.4, 18, z * 0.7], 0.65, gold);
    }
    for (let y = 2; y < 16; y += 4) {
      harborBeam(group, [side * 4.8, y, -2.5], [side * 4.1, y + 4, 2.5], 0.23, gold);
      harborBeam(group, [side * 4.8, y, 2.5], [side * 4.1, y + 4, -2.5], 0.23, gold);
    }
    // Parallel boom trusses extend over water, never across the playfield.
    for (const y of [18, 20]) {
      harborBeam(group, [side, y, 3], [side, y, -21], 0.32, gold);
    }
    for (let z = 3; z > -21; z -= 3) {
      harborBeam(group, [side, 18, z], [side, 20, z - 3], 0.19, gold);
      harborBeam(group, [side, 20, z], [side, 18, z - 3], 0.19, gold);
    }
    harborBeam(group, [side * 3.4, 17.7, 0], [side, 20, -13], 0.12, 0xb8b9a6);
  }
  put(group, harborBox(9, 1.1, 4.8, gold), 0, 17.6, 0);
  put(group, harborBox(4, 3.2, 4, dark), 0, 19, 3.8);
  put(group, harborBox(2.5, 2, 2.3, gold), 2, 16.2, -4);
  put(group, box(2.55, 1.15, 0.07, 0x6ca6b5, 0), 2, 16.4, -5.18);
  // Maintenance ladder and high-visibility railing.
  for (let y = 1; y < 17; y += 0.55) {
    put(group, box(0.7, 0.07, 0.1, 0xd2cdb8, 0), -4.7, y, 2.85);
  }
  batch(group);
  const load = new THREE.Group();
  for (const x of [-0.8, 0.8]) {
    harborBeam(load, [x, 0, 0], [x, -8, 0], 0.045, dark);
  }
  put(load, harborBox(4.5, 0.5, 2.8, gold), 0, -8, 0);
  for (const x of [-1.8, 1.8]) {
    put(load, box(0.22, 0.7, 0.2, dark), x, -8.5, 0);
  }
  batch(load);
  put(group, load, 0, 18, -15);
  return { group, load };
}

export class HarborFleet {
  group = new THREE.Group();
  private ships: THREE.Group[] = [];
  private loads: THREE.Group[] = [];
  constructor() {
    // Side berths make ships visible from both teams' normal deployment cameras.
    for (const [x, z, yaw, scale, color] of [
      [-8, -80, 0, 1, 0x3f6d80],
      [-77, -8, Math.PI / 2, 0.85, 0x527f79],
      [77, 15, -Math.PI / 2, 0.72, 0x9a6257],
    ]) {
      const ship = containerShip(color, this.ships.length);
      ship.rotation.y = yaw;
      ship.scale.setScalar(scale);
      put(this.group, ship, x, 0, z);
      this.ships.push(ship);
    }
    for (const [x, z, yaw, scale] of [
      [-40, -65, 0, 1],
      [40, -65, 0, 1],
      [-65.5, 16, Math.PI / 2, 0.75],
      [65.5, -20, -Math.PI / 2, 0.75],
    ]) {
      const crane = gantryCrane();
      crane.group.rotation.y = yaw;
      crane.group.scale.setScalar(scale);
      put(this.group, crane.group, x, 0, z);
      this.loads.push(crane.load);
    }
  }
  update(time: number): void {
    for (let i = 0; i < this.ships.length; i++) {
      this.ships[i].position.y = Math.sin(time * 0.55 + i * 2) * 0.07;
      this.ships[i].rotation.x = Math.sin(time * 0.42 + i) * 0.0018;
    }
    for (let i = 0; i < this.loads.length; i++) {
      this.loads[i].rotation.z = Math.sin(time * 0.7 + i) * 0.025;
    }
  }
}

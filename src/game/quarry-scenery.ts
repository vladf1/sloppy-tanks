import * as THREE from "three";
import { spawnPositions } from "./arena";
import { batch, freezeStatic } from "./batching";
import { TEAM_COLORS } from "./data";
import { groundMaterial, groundUVs } from "./ground-surfaces";
import { harborBox } from "./harbor-surfaces";
import { Random } from "./math";
import { box, cylinder, put } from "./model-primitives";
import { quarryDumpTruck, quarryExcavator } from "./quarry-machinery";
import { sandstoneRock } from "./quarry-surfaces";

/** Retained static scene: no per-frame animation, particles, lights or physics bodies. */
export class QuarryScenery extends THREE.Group {
  constructor(renderer: THREE.WebGLRenderer) {
    super();
    this.name = "dusty-dig-scenery";
    const ground = groundMaterial(renderer, "packed-dirt");
    ground.color.setHex(0xd9d9d4);
    ground.vertexColors = true;
    const geometry = new THREE.PlaneGeometry(420, 420, 140, 140).rotateX(-Math.PI / 2);
    groundUVs(geometry);
    const positions = geometry.getAttribute("position");
    const colors: number[] = [];
    for (let i = 0; i < positions.count; i++) {
      const x = positions.getX(i);
      const z = positions.getZ(i);
      // Flat physics arena surrounded by a shallow excavation and raised benches.
      const outside = Math.max(Math.abs(x), Math.abs(z)) - 60;
      positions.setY(i, outside > 0 ? -Math.min(1.8, outside * 0.3) : 0);
      const grain =
        0.5 + 0.25 * Math.sin(x * 0.31 + z * 0.17) + 0.25 * Math.sin(z * 0.47 - x * 0.19);
      const lane = Math.exp(-Math.pow(z / 6, 2)) + Math.exp(-Math.pow((Math.abs(z) - 51) / 4, 2));
      const wear = Math.min(1, lane) * 0.14;
      colors.push(
        0.81 + grain * 0.16 - wear,
        0.8 + grain * 0.16 - wear,
        0.82 + grain * 0.16 - wear,
      );
    }
    geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
    geometry.computeVertexNormals();
    const floor = new THREE.Mesh(geometry, ground);
    floor.receiveShadow = true;
    put(this, floor, 0, 0.008, 0);

    const geology = new THREE.Group();
    const equipment = new THREE.Group();
    const rng = new Random(9182);
    // Quarry benches step upward into the landscape. Close southern walls stay low.
    for (const side of [-1, 1]) {
      for (let i = -4; i <= 4; i++) {
        for (const [distance, height, depth] of [
          [82, 4.8, 15],
          [97, 9, 22],
          [128, 15, 30],
        ]) {
          const rock = sandstoneRock(30, height + rng.range(-0.7, 0.7), depth, (i + 4) % 4);
          put(geology, rock, i * 23 + rng.range(-1, 1), -1.6, side * distance);
        }
      }
      for (let i = -2; i <= 2; i++) {
        const rock = sandstoneRock(19, rng.range(5, 8), 23, (i + 2) % 4);
        put(geology, rock, side * 85, -1.5, i * 23);
      }
    }
    // Local rubble stays outside the boundary; it never advertises nonexistent cover.
    for (let i = 0; i < 65; i++) {
      const side = i % 2 ? -1 : 1;
      const rock = sandstoneRock(0.7 + (i % 3) * 0.5, 0.4 + (i % 4) * 0.25, 1.2, i % 4);
      rock.rotation.y = rng.range(-1, 1);
      const x = rng.range(-61, 61);
      const z = side * rng.range(64, 70);
      const y = 0.008 - Math.min(1.8, (Math.abs(z) - 60) * 0.3);
      put(geology, rock, x, y, z);
    }

    const excavator = quarryExcavator();
    excavator.rotation.y = -0.3;
    put(this, excavator, -24, -1.75, -68);
    const truck = quarryDumpTruck();
    truck.rotation.y = -0.45;
    put(this, truck, 36, -1.75, 68);
    // Load the truck with a few large chunks instead of dozens of individual stones.
    for (let i = 0; i < 5; i++) {
      put(truck, sandstoneRock(2.6, 1.25, 2.2, i % 4), -0.6 + (i % 3) * 1.8, 3.8, i % 2 ? -1 : 1);
    }
    for (const side of [-1, 1]) {
      // Survey stakes and boundary hazard paint frame the arena without fencing in views.
      for (let x = -56; x <= 56; x += 8) {
        put(equipment, box(0.13, 1.8, 0.13, 0xb6aea0, 0), x, 0.8, side * 61.2);
        put(equipment, box(0.17, 0.32, 0.17, 0xa55e3f, 0), x, 1.45, side * 61.2);
      }
      for (let z = -55; z <= 55; z += 10) {
        put(equipment, harborBox(0.05, 0.3, 1.8, 0x383a35), side * 59.97, 0.72, z);
        put(equipment, harborBox(0.05, 0.3, 0.65, 0xc1aa64), side * 59.94, 0.72, z);
      }
    }
    for (const team of [0, 1] as const) {
      for (const p of spawnPositions(team)) {
        put(equipment, cylinder(2.45, 0.08, 0x66675d, 12), p.x, 0.07, p.z);
        put(equipment, cylinder(2.1, 0.025, 0xaaa28a, 12), p.x, 0.125, p.z);
        for (const dz of [-1.7, 1.7]) {
          put(equipment, box(1.8, 0.025, 0.23, TEAM_COLORS[team], 0), p.x, 0.15, p.z + dz);
        }
      }
    }
    // Parked site office and stacked cut stone provide scale at the far quarry edge.
    put(equipment, harborBox(11, 3.5, 5, 0x9eaca5), 37, -0.04, -70);
    put(equipment, harborBox(11.6, 0.2, 5.6, 0x787e76), 37, 1.81, -70);
    for (const x of [33.5, 36.5, 39.5]) {
      put(equipment, box(1.8, 1.25, 0.05, 0x526c72, 0), x, 0.41, -67.47);
    }
    for (const z of [-2, 2]) {
      put(geology, sandstoneRock(5, 2.2, 3.5), -48, -1.79, 68 + z);
    }
    batch(geology);
    batch(equipment);
    this.add(geology, equipment);
    freezeStatic(this);
  }
}

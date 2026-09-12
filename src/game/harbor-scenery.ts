import * as THREE from "three";
import { spawnPositions } from "./arena";
import { batch, freezeStatic } from "./batching";
import { TEAM_COLORS } from "./data";
import { HarborFleet, harborBeam } from "./harbor-vessels";
import { harborBox } from "./harbor-surfaces";
import { sidingBox } from "./house-surfaces";
import { HarborWater } from "./harbor-water";
import { box, cylinder, material, put } from "./model-primitives";

function paintLabel(
  group: THREE.Group,
  text: string,
  x: number,
  z: number,
  width: number,
  depth: number,
): void {
  const canvas = document.createElement("canvas");
  canvas.width = 1024;
  canvas.height = 256;
  const ctx = canvas.getContext("2d")!;
  ctx.fillStyle = "#d9ce9d";
  ctx.font = "900 140px sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText(text, 512, 128, 990);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  const mesh = new THREE.Mesh(
    new THREE.PlaneGeometry(width, depth),
    new THREE.MeshStandardMaterial({
      map: texture,
      transparent: true,
      depthWrite: false,
      roughness: 1,
    }),
  );
  mesh.rotation.x = -Math.PI / 2;
  put(group, mesh, x, 0.035, z);
}

/** Built once and reused across rounds. All large scenery stays outside playable cover. */
export class HarborScenery {
  group = new THREE.Group();
  private water = new HarborWater();
  private fleet = new HarborFleet();
  private beacons = new THREE.Group();

  constructor() {
    this.group.add(this.water.mesh, this.fleet.group);
    const details = new THREE.Group();
    put(details, harborBox(124, 2.6, 124, 0x9aaba1, "dock"), 0, -1.4, 0);
    put(details, harborBox(120, 0.12, 120, 0xd3d4cd, "dock"), 0, -0.06, 0);
    // Broad circulation lanes surround the numbered container bays.
    for (const x of [-52, 0, 52]) {
      put(details, harborBox(x === 0 ? 15 : 10, 0.015, 118, 0x9b9d94, "dock"), x, 0.008, 0);
    }
    for (const z of [-45, 0, 45]) {
      put(details, harborBox(118, 0.012, 10, 0x9b9d94, "dock"), 0, 0.018, z);
    }
    for (const x of [-46, 46]) {
      for (let z = -54; z <= 54; z += 6) {
        put(details, box(0.16, 0.018, 2.8, 0xdacf9a, 0), x, 0.026, z);
      }
    }
    // Worn expansion joints and little aggregate flecks keep the apron from looking flat.

    for (let x = -60; x <= 60; x += 12) {
      put(details, box(0.045, 0.014, 120, 0x626e6c, 0), x, 0.02, 0);
      put(details, box(120, 0.014, 0.045, 0x626e6c, 0), 0, 0.02, x);
    }
    for (const side of [-1, 1]) {
      for (const z of [-32, -12, 12, 32]) {
        for (const dx of [-3.7, 3.7]) {
          put(details, box(0.12, 0.02, 15, 0xe4bb59, 0), side * 30 + dx, 0.03, z);
        }
        paintLabel(this.group, `B${Math.abs(z) === 32 ? "2" : "1"}`, side * 37, z, 2.5, 1.4);
      }
      for (let x = -57; x <= 57; x += 3) {
        put(details, box(1.5, 0.025, 0.65, 0xe1b64d, 0), x, 1.215, side * 60.5);
        put(details, box(1.5, 0.025, 0.65, 0x303e42, 0), x + 1.5, 1.215, side * 60.5);
      }
      for (const x of [-48, -24, 0, 24, 48]) {
        // Quay bollards, fenders and coiled mooring line are beyond the wall.
        put(details, cylinder(0.42, 0.7, 0x253e45), x, 0.05, side * 62);
        put(details, box(1.4, 0.25, 0.5, 0x253e45), x, 0.45, side * 62);
        const rope = new THREE.Mesh(new THREE.TorusGeometry(0.8, 0.07, 4, 16), material(0xb89e70));
        rope.rotation.x = Math.PI / 2;
        put(details, rope, x + 1.4, -0.02, side * 62);
        const fender = cylinder(0.65, 1.4, 0x25363d, 12);
        fender.rotation.x = Math.PI / 2;
        put(details, fender, x, -1.1, side * 62.2);
      }
      for (const x of [-54, 54]) {
        put(details, cylinder(0.16, 6, 0x45575c), x, 3, side * 62);
        put(details, box(1.5, 0.25, 0.7, 0xe3cc90), x, 6, side * 62);
        put(this.beacons, cylinder(0.18, 0.3, 0xffb54a, 8), x, 6.25, side * 62);
      }
    }
    // Central loading square is traversable; the laser pickup remains contested at its center.
    for (const side of [-1, 1]) {
      put(details, box(12, 0.025, 0.2, 0xe1b64d, 0), 0, 0.035, side * 5);
      put(details, box(0.2, 0.025, 10, 0xe1b64d, 0), side * 6, 0.035, 0);
    }
    paintLabel(this.group, "HARBOR HAVOC", 0, -48, 28, 4);
    paintLabel(this.group, "PORT 07", 0, 48, 15, 3.5);
    paintLabel(this.group, "LOADING", 0, 3.5, 8, 1.1);
    for (const team of [0, 1] as const) {
      const side = team === 0 ? -1 : 1;
      for (const p of spawnPositions(team)) {
        put(details, cylinder(2.65, 0.08, 0x293f4a, 12), p.x, 0.06, p.z);
        const ring = new THREE.Mesh(
          new THREE.RingGeometry(2.1, 2.3, 32),
          material(TEAM_COLORS[team]),
        );
        ring.rotation.x = -Math.PI / 2;
        put(details, ring, p.x, 0.11, p.z);
        put(details, cylinder(0.055, 4.8, 0x63767b, 8), side * 62, 2.4, p.z);
      }
    }
    // Working berths sit beyond the wall: forklifts, pallets, drainage and tied mooring lines.
    for (const side of [-1, 1]) {
      const x = side * 65;
      put(details, harborBox(7, 1.4, 13, 0xb9b9a7, "dock"), x, -0.65, 43);
      put(details, box(1.7, 0.75, 2.6, 0xdca53b), x, 0.65, 43);
      for (const dx of [-0.9, 0.9]) {
        for (const z of [42.2, 43.9]) {
          const wheel = cylinder(0.43, 0.3, 0x2c383c, 12);
          wheel.rotation.z = Math.PI / 2;
          put(details, wheel, x + dx, 0.45, z);
        }
        put(details, box(0.12, 1.7, 0.12, 0x344952), x + dx * 0.75, 1.6, 43);
        put(details, box(0.12, 2.4, 0.14, 0x344952), x + dx * 0.55, 1.3, 41.7);
        put(details, box(0.14, 0.12, 1.8, 0x647174), x + dx * 0.55, 0.25, 40.9);
      }
      put(details, box(1.7, 0.16, 1.7, 0xe5b441), x, 2.5, 43);
      put(details, box(0.7, 0.7, 0.6, 0x344952), x, 1.35, 43.25);
      for (let level = 0; level < 3; level++) {
        for (let plank = 0; plank < 5; plank++) {
          put(
            details,
            sidingBox(2.1, 0.12, 0.27, 0xb89764),
            x,
            0.3 + level * 0.3,
            46.2 + plank * 0.34,
          );
        }
        for (const dx of [-0.8, 0.8]) {
          put(details, sidingBox(0.25, 0.18, 1.7, 0x96764e), x + dx, 0.14 + level * 0.3, 46.85);
        }
      }
      // Flush grates remain traversable, with no fake solid cover in driving lanes.
      for (const z of [-39, 0, 39]) {
        put(details, box(1.2, 0.025, 2.2, 0x36484b, 0), side * 58, 0.035, z);
        for (let bar = 0; bar < 9; bar++) {
          put(details, box(1.1, 0.03, 0.06, 0x838c86, 0), side * 58, 0.05, z - 0.9 + bar * 0.22);
        }
      }
      harborBeam(
        details,
        [side * 62, 0.4, -24],
        [side * 72, 2.3, side < 0 ? -27 : -4],
        0.075,
        0xbaa377,
      );
      harborBeam(
        details,
        [side * 62, 0.4, 24],
        [side * 72, 2.3, side < 0 ? 12 : 34],
        0.075,
        0xbaa377,
      );
      harborBeam(details, [side * 24, 0.4, -62], [side * 20 - 8, 3, -73], 0.075, 0xbaa377);
    }
    // Distant warehouses close the horizon without creating obstacles on the board.
    for (const x of [-115, 115]) {
      put(details, harborBox(22, 3, 78, 0x9aaba1, "dock"), x, -2, 0);
      put(details, box(18, 8, 70, 0x526c76), x, 2, 0);
      put(details, box(19, 0.5, 72, 0x364f5d), x, 6.2, 0);
      for (let z = -28; z <= 28; z += 14) {
        put(details, box(0.08, 2, 5, 0xc7b887), x + (x < 0 ? 9.1 : -9.1), 3, z);
      }
    }
    batch(details);
    this.group.add(details, this.beacons);
    freezeStatic(details);
    // Painted edge stripes sit on the actual 1.2m perimeter wall.
    for (const child of details.children) {
      child.receiveShadow = true;
    }
    this.update(0);
  }

  update(time: number): void {
    this.water.update(time);
    this.fleet.update(time);
    this.beacons.visible = Math.sin(time * 2.5) > -0.3;
  }
}

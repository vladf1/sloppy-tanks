import * as THREE from "three";
import { spawnPositions } from "./arena";
import { batch, freezeStatic } from "./batching";
import { TEAM_COLORS } from "./data";
import { quarryTerrain } from "./quarry-terrain";
import {
  quarryBench,
  quarryButte,
  quarryButteSpot,
  quarryScree,
  quarryScreeSpots,
} from "./quarry-benches";
import { quarrySiteDetails } from "./quarry-site-details";
import { harborBox } from "./harbor-surfaces";
import { Random } from "./math";
import { box, cylinder, put } from "./model-primitives";
import { quarryDumpTruck, quarryExcavator } from "./quarry-machinery";
import { sandstoneRock } from "./quarry-surfaces";

export interface SpawnPadPiece {
  /** Offset from the spawn point, in metres. */
  dx: number;
  dz: number;
  /** Center height of the piece. */
  y: number;
  w: number;
  h: number;
  d: number;
  color: number;
  rotY: number;
  shape: "disc" | "dash" | "chevron" | "post" | "cap";
}

/** Graded deployment pad: a compacted gravel disc with a worn center, a ring of
 * hazard dashes echoing the boundary paint, chevrons pointing at the arena and
 * one team-capped beacon post. Everything stays ankle-high so pads read as
 * markings, never as cover. */
export function quarrySpawnPadPieces(team: 0 | 1): SpawnPadPiece[] {
  const pieces: SpawnPadPiece[] = [
    { shape: "disc", dx: 0, dz: 0, y: 0.06, w: 2.6, h: 0.09, d: 2.6, color: 0x8f7c62, rotY: 0 },
    { shape: "disc", dx: 0, dz: 0, y: 0.115, w: 2.15, h: 0.03, d: 2.15, color: 0xcbb894, rotY: 0 },
  ];
  for (let i = 0; i < 12; i++) {
    const angle = (i * Math.PI) / 6;
    pieces.push({
      shape: "dash",
      dx: Math.cos(angle) * 2.38,
      dz: Math.sin(angle) * 2.38,
      y: 0.115,
      w: 0.45,
      h: 0.03,
      d: 0.45,
      color: i % 2 ? 0xc1aa64 : 0x383a35,
      rotY: 0,
    });
  }
  // Big wedge centered on the pad: the arms meet at an apex aiming at the arena
  // while the triangle's centroid sits exactly on the spawn point.
  const inward = team === 0 ? 1 : -1;
  for (const s of [-1, 1]) {
    pieces.push({
      shape: "chevron",
      dx: inward * 0.322,
      dz: s * 0.527,
      y: 0.13,
      w: 2.2,
      h: 0.03,
      d: 0.26,
      color: TEAM_COLORS[team],
      rotY: s * 0.5 * inward,
    });
  }
  const side = team === 0 ? -1 : 1;
  pieces.push(
    {
      shape: "post",
      dx: side * 2.5,
      dz: 0,
      y: 0.45,
      w: 0.06,
      h: 0.9,
      d: 0.06,
      color: 0x535953,
      rotY: 0,
    },
    {
      shape: "cap",
      dx: side * 2.5,
      dz: 0,
      y: 0.98,
      w: 0.24,
      h: 0.2,
      d: 0.24,
      color: TEAM_COLORS[team],
      rotY: 0,
    },
  );
  return pieces;
}

function quarrySpawnPad(group: THREE.Group, team: 0 | 1, x: number, z: number): void {
  for (const piece of quarrySpawnPadPieces(team)) {
    const mesh =
      piece.shape === "disc" || piece.shape === "post"
        ? cylinder(piece.w, piece.h, piece.color, piece.shape === "disc" ? 20 : 8)
        : piece.shape === "dash"
          ? harborBox(piece.w, piece.h, piece.d, piece.color)
          : box(piece.w, piece.h, piece.d, piece.color, 0);
    mesh.rotation.y = piece.rotY;
    put(group, mesh, x + piece.dx, piece.y, z + piece.dz);
  }
}

/** Retained static scene: no per-frame animation, particles, lights or physics bodies. */
export class QuarryScenery extends THREE.Group {
  constructor(renderer: THREE.WebGLRenderer) {
    super();
    this.name = "dusty-dig-scenery";
    this.add(quarryTerrain(renderer));

    const geology = new THREE.Group();
    const equipment = new THREE.Group();
    const rng = new Random(9182);
    // Long, connected cuts replace the repeated perimeter boulders. Offset benches
    // expose broad shelves and a broken skyline above the machinery apron.
    for (const side of [-1, 1]) {
      for (const [distance, height, base, depth] of [
        [77, 6.5, -1.8, 22],
        [94, 8, 2.5, 26],
        [115, 11, 7.8, 70],
      ]) {
        const face = quarryBench(280, height, depth, distance + side * 17);
        if (side < 0) {
          face.rotation.y = Math.PI;
        }
        put(geology, face, 0, base, side * distance);
      }
      for (const [distance, height, base] of [
        [78, 6, -1.8],
        [97, 10, 2.2],
      ]) {
        const face = quarryBench(155, height, 50, distance + side * 37);
        face.rotation.y = (side * Math.PI) / 2;
        put(geology, face, side * distance, base, 0);
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

    // Collapsed runouts interrupt the first terrace; the stacked sentinel gives
    // the north-west apron one recognizable landmark. All footprints stay
    // outside the playable boundary on the machinery apron.
    for (const spot of quarryScreeSpots()) {
      const scree = quarryScree(spot.length, spot.height, spot.depth, spot.seed);
      scree.rotation.y = spot.rotY;
      const dip = Math.min(1.8, (Math.max(Math.abs(spot.x), Math.abs(spot.z)) - 60) * 0.3);
      put(geology, scree, spot.x, 0.008 - dip, spot.z);
    }
    const butteSpot = quarryButteSpot();
    const butte = quarryButte(butteSpot.scale, butteSpot.rotY);
    put(geology, butte, butteSpot.x, butteSpot.baseY, butteSpot.z);

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
        quarrySpawnPad(equipment, team, p.x, p.z);
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
    quarrySiteDetails(equipment, geology);
    batch(geology);
    batch(equipment);
    this.add(geology, equipment);
    freezeStatic(this);
  }
}

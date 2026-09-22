import * as THREE from "three/webgpu";
import { spawnPositions } from "./arena";
import { batch, freezeStatic } from "./batching";
import { TEAM_COLORS } from "./data";
import { quarryTerrain } from "./quarry-terrain";
import { quarryBench, quarryButte, quarryButteSpot, quarryScreeSpots } from "./quarry-benches";
import { quarrySiteDetails } from "./quarry-site-details";
import { quarryScree } from "./quarry-scree";
import {
  QUARRY_RAMP,
  quarryRampBoulders,
  quarryRampGeometry,
  quarryRampHeight,
  quarryRampSpoil,
} from "./quarry-ramp";
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
  constructor(renderer: THREE.WebGPURenderer) {
    super();
    this.name = "dusty-dig-scenery";
    const terrain = quarryTerrain(renderer);
    this.add(terrain);

    const geology = new THREE.Group();
    const equipment = new THREE.Group();
    const rng = new Random(9182);
    // Long, connected cuts replace the repeated perimeter boulders. Offset benches
    // expose broad shelves and a broken skyline above the machinery apron. Each
    // side digs to its own depth so the excavation never reads as a rectangle.
    for (const side of [-1, 1]) {
      const faces =
        side < 0
          ? [
              [77, 7.5, -1.8, 22],
              [95, 9, 2.5, 26],
              [115, 12, 7.8, 70],
            ]
          : [
              [78, 5.5, -1.8, 22],
              [93, 7.5, 2.5, 26],
              [114, 10, 7.8, 70],
            ];
      for (const [distance, height, base, depth] of faces) {
        const face = quarryBench(280, height, depth, distance + side * 17);
        if (side < 0) {
          face.rotation.y = Math.PI;
        }
        put(geology, face, 0, base, side * distance);
      }
      const flanks =
        side < 0
          ? [
              [80, 5.5, -1.8],
              [96, 8.5, 2.2],
            ]
          : [
              [78, 6.5, -1.8],
              [97, 9, 2.2],
            ];
      for (const [distance, height, base] of flanks) {
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
    // Half-buried flank boulders break the east/west aprons. A separate stream
    // keeps the north/south rubble exactly where it was. Each candidate clears
    // the parked truck, the scree runouts and the playable boundary.
    const flankRng = new Random(1379);
    const flankBlocks: [number, number, number, number][] = [
      [64, 76, 10, 26], // haul truck bay
      [69, 76.5, -23, -1], // east scree runout
      [-76.5, -69, -6, 18], // west scree runout
      [QUARRY_RAMP.x0, QUARRY_RAMP.x1, QUARRY_RAMP.z0, QUARRY_RAMP.z1], // east haul ramp
    ];
    let flankPlaced = 0;
    for (let i = 0; i < 40 && flankPlaced < 22; i++) {
      const side = i % 2 ? -1 : 1;
      const x = side * flankRng.range(64.5, 71);
      const z = flankRng.range(-50, 50);
      if (
        flankBlocks.some(([x0, x1, z0, z1]) => x > x0 - 2 && x < x1 + 2 && z > z0 - 2 && z < z1 + 2)
      ) {
        continue;
      }
      const w = flankRng.range(1.4, 2.8);
      const rock = sandstoneRock(w, flankRng.range(0.9, 1.8), flankRng.range(1.2, 2.4), i % 5);
      rock.rotation.y = flankRng.range(-Math.PI, Math.PI);
      const y = 0.008 - Math.min(1.8, (Math.abs(x) - 60) * 0.3) - 0.14;
      put(geology, rock, x, y, z);
      flankPlaced++;
    }

    // Collapsed runouts interrupt the first terrace; the stacked sentinel gives
    // the north apron one recognizable landmark. All footprints stay outside
    // the playable boundary on the machinery apron.
    for (const spot of quarryScreeSpots()) {
      geology.add(...quarryScree(spot, terrain.material));
    }
    // The haul ramp gives the parked machinery a believable way out of the pit.
    const rampSoil = terrain.material.clone();
    rampSoil.vertexColors = true;
    geology.add(new THREE.Mesh(quarryRampGeometry(), rampSoil));
    for (const [i, boulder] of quarryRampBoulders().entries()) {
      const rock = sandstoneRock(boulder.size, boulder.size * 0.6, boulder.size * 0.85, i % 5);
      rock.rotation.y = boulder.rotY;
      put(geology, rock, boulder.x, quarryRampHeight(boulder.x, boulder.z) - 0.2, boulder.z);
    }
    for (const [i, chip] of quarryRampSpoil().entries()) {
      const rock = sandstoneRock(chip.size, chip.size * 0.45, chip.size * 0.8, i % 7);
      rock.rotation.y = chip.rotY;
      put(geology, rock, chip.x, quarryRampHeight(chip.x, chip.z) - 0.08, chip.z);
    }
    const butteSpot = quarryButteSpot();
    const butte = quarryButte(butteSpot.scale, butteSpot.rotY);
    put(geology, butte, butteSpot.x, butteSpot.baseY, butteSpot.z);

    const excavator = quarryExcavator();
    excavator.rotation.y = -0.3;
    put(this, excavator, -24, -1.75, -68);
    // The south apron sits behind the gameplay camera, so the haul truck parks
    // on the east apron where the eastern spawn band sees it past the teeth.
    const truck = quarryDumpTruck();
    truck.rotation.y = Math.PI / 2 + 0.18;
    put(this, truck, 69, -1.75, 18);
    // Load the truck with a few large chunks instead of dozens of individual stones.
    for (let i = 0; i < 5; i++) {
      put(truck, sandstoneRock(2.6, 1.25, 2.2, i % 4), -0.6 + (i % 3) * 1.8, 3.8, i % 2 ? -1 : 1);
    }
    for (const side of [-1, 1]) {
      // Survey stakes and boundary hazard paint frame the arena without fencing in views.
      // A few missing stakes and a slight lean keep the line from reading as a fence.
      for (let x = -56; x <= 56; x += 8) {
        const index = (x + 56) / 8 + (side < 0 ? 2 : 0);
        if (index % 7 === 3) {
          continue;
        }
        const lean = 0.05 * Math.sin(x * 2.3 + side);
        const stake = box(0.13, 1.8, 0.13, 0xb6aea0, 0);
        stake.rotation.z = lean;
        put(equipment, stake, x, 0.8, side * 61.2);
        put(equipment, box(0.17, 0.32, 0.17, 0xa55e3f, 0), x - lean * 0.65, 1.45, side * 61.2);
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

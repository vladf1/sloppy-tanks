import * as THREE from "three";
import { TEAM_COLORS, VEHICLES } from "./data";
import { box, cylinder, material, put } from "./model-primitives";
import { applyTankSurface } from "./tank-surfaces";
import type { Team, VehicleKind } from "./types";
// Shared geometry keeps the more detailed silhouette inexpensive to instance/batch.
const armorGeometry = new Map<number, THREE.BufferGeometry>();
function armor(w: number, h: number, d: number, color: number, taper = 0.76) {
  let geo = armorGeometry.get(taper);
  if (!geo) {
    // Chamfered rectangular plates, with a recessed roof and sloping front glacis.
    const outline = [
      [-0.38, -0.5],
      [0.38, -0.5],
      [0.5, -0.36],
      [0.5, 0.32],
      [0.32, 0.5],
      [-0.32, 0.5],
      [-0.5, 0.32],
      [-0.5, -0.36],
    ];
    const vertices: number[] = [];
    const indices: number[] = [];
    for (const top of [false, true]) {
      for (const [x, z] of outline) {
        vertices.push(x * (top ? taper : 1), top ? 0.5 : -0.5, top ? z * taper - 0.04 : z);
      }
    }
    for (let i = 0; i < 8; i++) {
      const next = (i + 1) % 8;
      indices.push(i, i + 8, next, next, i + 8, next + 8);
    }
    for (let i = 1; i < 7; i++) {
      indices.push(0, i, i + 1, 8, i + 9, i + 8);
    }
    const indexed = new THREE.BufferGeometry();
    indexed.setAttribute("position", new THREE.Float32BufferAttribute(vertices, 3));
    indexed.setIndex(indices);
    geo = indexed.toNonIndexed();
    indexed.dispose();
    geo.computeVertexNormals();
    geo.setAttribute(
      "uv",
      new THREE.Float32BufferAttribute(new Float32Array(geo.getAttribute("position").count * 2), 2),
    );
    // Planar UVs per face keep armor texture visible on tops, cheeks and sides.
    const positions = geo.getAttribute("position");
    const normals = geo.getAttribute("normal");
    const uv = geo.getAttribute("uv");
    for (let i = 0; i < positions.count; i++) {
      const x = positions.getX(i);
      const y = positions.getY(i);
      const z = positions.getZ(i);
      const nx = Math.abs(normals.getX(i));
      const ny = Math.abs(normals.getY(i));
      const nz = Math.abs(normals.getZ(i));
      uv.setXY(
        i,
        (ny >= nx && ny >= nz ? x : nx > nz ? z : x) + 0.5,
        (ny >= nx && ny >= nz ? z : y) + 0.5,
      );
    }
    armorGeometry.set(taper, geo);
  }
  const mesh = new THREE.Mesh(geo, material(color, 0.18, 0.58));
  mesh.scale.set(w, h, d);
  mesh.castShadow = mesh.receiveShadow = true;
  return mesh;
}
const beltShape = new THREE.Shape();
beltShape.moveTo(-0.9, -0.33);
beltShape.lineTo(0.9, -0.33);
beltShape.absarc(0.9, 0, 0.33, -Math.PI / 2, Math.PI / 2, false);
beltShape.lineTo(-0.9, 0.33);
beltShape.absarc(-0.9, 0, 0.33, Math.PI / 2, Math.PI * 1.5, false);
const beltGeometry = new THREE.ExtrudeGeometry(beltShape, {
  depth: 0.54,
  bevelEnabled: false,
  curveSegments: 6,
})
  .translate(0, 0, -0.27)
  .rotateY(Math.PI / 2);
function trackBelt(color: number) {
  const mesh = new THREE.Mesh(beltGeometry, material(color));
  mesh.castShadow = mesh.receiveShadow = true;
  return mesh;
}

export interface TankModel extends THREE.Group {
  userData: {
    kind: VehicleKind;
    hull: THREE.Group;
    turret: THREE.Group;
    barrel: THREE.Group;
    trackGroup: THREE.Group;
    muzzle: THREE.Object3D;
  };
}
export function tankModel(kind: VehicleKind, team: Team, wreck = false): TankModel {
  const root = new THREE.Group() as TankModel;
  const hull = new THREE.Group();
  const turret = new THREE.Group();
  const scout = kind === "scout";
  const heavy = kind === "heavy";
  const color = wreck ? 0x3c4650 : TEAM_COLORS[team];
  const dark = 0x13232c;
  const steel = wreck ? 0x37424c : 0x637581;
  const shade = wreck ? dark : new THREE.Color(color).multiplyScalar(0.62).getHex();
  // Dimensions include the tracks and skirts, not just the center armor slab.
  // Hull length/overall width: compact scout ~1.94, Abrams/Type 99 ~2.17.
  const overallWidth = scout ? 2.3 : heavy ? 2.5 : 2.42;
  const width = overallWidth - 0.42;
  const length = scout ? 4.415 : heavy ? 5.425 : 5.244;
  const deck = scout ? 0.58 : heavy ? 0.67 : 0.65;
  root.add(hull);
  put(hull, armor(width, 0.32, length, shade, 0.94), 0, 0.22, 0);
  put(hull, armor(width, deck - 0.2, length, color, scout ? 0.72 : 0.86), 0, deck / 2 + 0.16, 0);
  const trackGroup = new THREE.Group();
  hull.add(trackGroup);
  for (const side of [-1, 1]) {
    const trackX = side * (overallWidth / 2 - 0.23);
    const belt = trackBelt(dark);
    belt.scale.set(0.7, 1.12, length / 2.46);
    put(hull, belt, trackX, 0.19, 0);
    const wheels = scout || heavy ? 6 : 7;
    for (let j = 0; j < wheels; j++) {
      const z = -length * 0.37 + (j * length * 0.74) / (wheels - 1);
      const wheel = cylinder(scout ? 0.3 : 0.34, 0.055, shade, 10);
      wheel.rotation.z = Math.PI / 2;
      put(hull, wheel, trackX + side * 0.1925, 0.18, z);
      const hub = cylinder(0.1, 0.04, steel, 8);
      hub.rotation.z = Math.PI / 2;
      put(hull, hub, trackX + side * 0.205, 0.18, z);
    }
    for (let j = 0; j < 18; j++) {
      const tread = box(0.42, 0.03, 0.075, steel, 0);
      put(trackGroup, tread, trackX, 0.565, -length * 0.44 + j * length * 0.052);
    }
    put(hull, box(0.46, 0.07, length, color, 0), trackX, deck, 0);
    {
      // Booker: short modular skirts. Abrams: long panels. Type 99: heavy blocks.
      const panels = scout ? 4 : heavy ? 5 : 3;
      for (let j = 0; j < panels; j++) {
        const skirt = box(
          heavy ? 0.12 : 0.07,
          heavy ? 0.3 : 0.26,
          length / panels - 0.04,
          heavy && j % 2 ? shade : color,
          0,
        );
        put(
          hull,
          skirt,
          trackX + side * (heavy ? 0.17 : 0.19),
          deck - 0.17,
          -length / 2 + ((j + 0.5) * length) / panels,
        );
      }
    }
    put(hull, box(0.16, 0.11, 0.1, 0xd9e6df, 0), side * 0.65, deck - 0.1, length / 2 - 0.05);
  }
  // Exposed rear engine deck gives the hull a direction even with the turret turned.
  for (let i = 0; i < 7; i++) {
    put(
      hull,
      box(width * 0.53, 0.025, 0.05, dark, 0),
      0,
      deck + 0.025,
      -length / 2 + 0.12 + i * 0.075,
    );
  }
  put(hull, box(0.32, 0.045, 0.3, steel, 0), 0, deck + 0.02, length / 2 - 0.45);
  if (heavy) {
    for (const side of [-1, 1]) {
      for (let j = 0; j < 3; j++) {
        const tile = box(0.31, 0.1, 0.24, shade, 0);
        tile.rotation.x = -0.18;
        put(hull, tile, side * (0.24 + j * 0.32), deck - 0.06, length / 2 - 0.3);
      }
    }
  }

  put(turret, cylinder(scout ? 0.59 : 0.76, 0.1, dark, 16), 0, deck + 0.055, -0.12);
  let roof: number;
  if (scout) {
    // M10 Booker-inspired compact welded turret, smooth armor and enclosed bustle.
    put(turret, armor(1.66, 0.43, 2.03, color, 0.8), 0, deck + 0.28, -0.12);
    for (const side of [-1, 1]) {
      const cheek = armor(0.5, 0.33, 0.72, shade, 0.67);
      cheek.rotation.y = side * -0.16;
      put(turret, cheek, side * 0.56, deck + 0.27, 0.48);
      put(turret, box(0.18, 0.27, 0.67, color, 0), side * 0.74, deck + 0.23, -0.64);
    }
    roof = deck + 0.5;
    put(turret, box(1.22, 0.3, 0.45, shade, 0), 0, deck + 0.22, -1.12);
    put(turret, box(0.28, 0.19, 0.27, steel, 0), 0.35, roof + 0.095, 0.11);
    put(turret, box(0.2, 0.08, 0.025, 0x8adeec, 0), 0.35, roof + 0.11, 0.26);
  } else if (!heavy) {
    // Abrams: broad trapezoidal cheeks and a long, boxy bustle behind the ring.
    put(turret, armor(1.97, 0.46, 2.65, color, 0.83), 0, deck + 0.31, -0.3);
    for (const side of [-1, 1]) {
      const cheek = armor(0.76, 0.4, 1.08, color, 0.66);
      cheek.rotation.y = side * -0.2;
      put(turret, cheek, side * 0.58, deck + 0.3, 0.58);
    }
    roof = deck + 0.56;
    put(turret, box(1.64, 0.36, 0.65, shade, 0), 0, deck + 0.27, -1.58);
    // Open rear stowage basket is a large, recognizable silhouette feature.
    for (const y of [deck + 0.18, deck + 0.49]) {
      put(turret, box(1.92, 0.055, 0.055, steel, 0), 0, y, -1.96);
      for (const side of [-1, 1]) {
        put(turret, box(0.055, 0.055, 0.69, steel, 0), side * 0.93, y, -1.64);
      }
    }
    for (const x of [-0.93, -0.46, 0, 0.46, 0.93]) {
      put(turret, box(0.04, 0.31, 0.04, steel, 0), x, deck + 0.335, -1.96);
    }
    put(turret, cylinder(0.19, 0.25, shade, 10), 0.49, roof + 0.13, 0.14);
    put(turret, box(0.19, 0.09, 0.08, 0x8adeec, 0), 0.49, roof + 0.2, 0.3);
  } else {
    // Type 99: compact center, sharply pointed twin wedges and tiled armor.
    put(turret, armor(1.5, 0.47, 2.25, shade, 0.73), 0, deck + 0.32, -0.23);
    roof = deck + 0.58;
    for (const side of [-1, 1]) {
      const wedge = armor(0.82, 0.48, 1.65, color, 0.48);
      wedge.rotation.y = side * -0.36;
      put(turret, wedge, side * 0.64, deck + 0.31, 0.38);
      for (let j = 0; j < 4; j++) {
        const tile = box(0.36, 0.1, 0.22, shade, 0);
        tile.rotation.set(-0.28, side * -0.36, side * 0.12);
        put(turret, tile, side * (0.36 + j * 0.17), roof - 0.06 - j * 0.07, 0.72 - j * 0.28);
      }
      put(turret, box(0.3, 0.38, 0.65, color, 0), side * 0.73, deck + 0.27, -1.15);
    }
    put(turret, box(0.34, 0.3, 0.32, steel, 0), 0.4, roof + 0.15, -0.52);
    put(turret, box(0.19, 0.1, 0.04, 0x8adeec, 0), 0.4, roof + 0.2, -0.35);
  }
  for (const side of [-1, 1]) {
    put(turret, cylinder(scout ? 0.18 : 0.21, 0.065, steel, 12), side * 0.28, roof + 0.025, -0.24);
    for (let j = 0; j < 3; j++) {
      const smoke = cylinder(0.055, 0.2, steel, 8);
      smoke.rotation.x = Math.PI / 3;
      put(turret, smoke, side * (scout ? 0.72 : 0.88), roof - 0.32, 0.06 - j * 0.15);
    }
  }
  // A compact roof gun and antenna distinguish equipment without expensive meshes.
  put(turret, box(0.09, 0.18, 0.1, dark, 0), -0.28, roof + 0.16, -0.24);
  put(turret, box(0.07, 0.07, scout ? 0.42 : 0.6, steel, 0), -0.28, roof + 0.25, -0.02);
  put(turret, cylinder(0.018, scout ? 0.5 : 0.7, dark, 5), 0.53, roof + 0.25, -0.67);

  const barrel = new THREE.Group();
  const gunY = deck + (scout ? 0.27 : 0.31);
  const tubeRadius = scout ? 0.062 : heavy ? 0.074 : 0.068;
  const muzzleZ = scout ? 3.72 : heavy ? 4.44 : 3.84;
  const tube = cylinder(tubeRadius, muzzleZ - 0.6, steel, 12);
  tube.rotation.x = Math.PI / 2;
  put(barrel, tube, 0, gunY, (muzzleZ + 0.6) / 2);
  for (const [z, radius, length] of [
    [0.67, tubeRadius * 2.1, 0.42],
    [muzzleZ * 0.58, tubeRadius * 1.5, 0.36],
  ]) {
    const sleeve = cylinder(radius, length, shade, 12);
    sleeve.rotation.x = Math.PI / 2;
    put(barrel, sleeve, 0, gunY, z);
  }
  if (scout) {
    // Squared muzzle brake is visually distinct from the two smoothbore guns.
    put(barrel, box(0.22, 0.17, 0.27, steel, 0), 0, gunY, muzzleZ - 0.135);
    for (const side of [-1, 1]) {
      for (const z of [muzzleZ - 0.2, muzzleZ - 0.09]) {
        put(barrel, box(0.012, 0.1, 0.05, dark, 0), side * 0.111, gunY, z);
      }
    }
  }
  const bore = cylinder(tubeRadius * 0.76, 0.008, dark, 12);
  bore.rotation.x = Math.PI / 2;
  put(barrel, bore, 0, gunY, muzzleZ + 0.005);
  turret.add(barrel);
  root.add(turret);
  root.scale.setScalar(VEHICLES[kind].scale);
  if (team === 0) {
    const badge = box(0.18, 0.025, 0.18, 0xdce7ee, 0);
    badge.rotation.y = Math.PI / 4;
    put(turret, badge, 0, roof + 0.018, 0.12);
  } else {
    for (const x of [-0.07, 0.07]) {
      put(turret, box(0.06, 0.025, 0.2, 0xdce7ee, 0), x, roof + 0.018, 0.12);
    }
  }
  root.userData = { kind, hull, turret, barrel, trackGroup, muzzle: bore };
  applyTankSurface(root, [color, shade, steel]);
  return root;
}

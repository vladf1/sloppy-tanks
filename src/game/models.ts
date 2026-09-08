import * as THREE from "three";
import { batch } from "./batching";
import { explosiveBarrel } from "./barrel-surfaces";
import { sidingBox, sidingGable, shingleRoof } from "./house-surfaces";
import { applyTankSurface } from "./tank-surfaces";
import { concreteWall } from "./concrete-surfaces";
import { TOWER_BASE } from "./tower-layout";
import { treeModel } from "./tree-models";
export { stumpModel } from "./tree-models";
import { RoundedBoxGeometry } from "three/addons/geometries/RoundedBoxGeometry.js";
import { Random, TEAM_COLORS, VEHICLES } from "./data";
import type { VehicleKind, Team, Cover, WreckPart } from "./types";
const materials = new Map<string, THREE.MeshStandardMaterial>();
export function material(color: number, metalness = 0.05, roughness = 0.65) {
  const key = `${color}/${metalness}/${roughness}`;
  let m = materials.get(key);
  if (!m) {
    m = new THREE.MeshStandardMaterial({ color, metalness, roughness });
    if (TEAM_COLORS.includes(color)) {
      m.emissive.setHex(color);
      m.emissiveIntensity = 0.04;
      m.toneMapped = false;
    }
    materials.set(key, m);
  }
  return m;
}
const boxes = new Map<string, THREE.BufferGeometry>();
export function box(w: number, h: number, d: number, color: number, r = 0.06) {
  const key = [w, h, d, r].join("/");
  let geo = boxes.get(key);
  if (!geo) {
    geo =
      r > 0
        ? new RoundedBoxGeometry(w, h, d, 1, r)
        : new THREE.BoxGeometry(w, h, d);
    boxes.set(key, geo);
  }
  const mesh = new THREE.Mesh(geo, material(color));
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  return mesh;
}
export function put(
  parent: THREE.Object3D,
  obj: THREE.Object3D,
  x = 0,
  y = 0,
  z = 0,
) {
  obj.position.set(x, y, z);
  parent.add(obj);
  return obj;
}
const cylinders = new Map<string, THREE.CylinderGeometry>();
export function cylinder(
  radius: number,
  height: number,
  color: number,
  sides = 12,
) {
  const key = `${radius}/${height}/${sides}`;
  let geo = cylinders.get(key);
  if (!geo) {
    geo = new THREE.CylinderGeometry(radius, radius, height, sides);
    cylinders.set(key, geo);
  }
  const mesh = new THREE.Mesh(geo, material(color, 0.2));
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  return mesh;
}
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
    const vertices: number[] = [],
      indices: number[] = [];
    for (const top of [false, true])
      for (const [x, z] of outline)
        vertices.push(
          x * (top ? taper : 1),
          top ? 0.5 : -0.5,
          top ? z * taper - 0.04 : z,
        );
    for (let i = 0; i < 8; i++) {
      const next = (i + 1) % 8;
      indices.push(i, i + 8, next, next, i + 8, next + 8);
    }
    for (let i = 1; i < 7; i++) indices.push(0, i, i + 1, 8, i + 9, i + 8);
    const indexed = new THREE.BufferGeometry();
    indexed.setAttribute(
      "position",
      new THREE.Float32BufferAttribute(vertices, 3),
    );
    indexed.setIndex(indices);
    geo = indexed.toNonIndexed();
    indexed.dispose();
    geo.computeVertexNormals();
    geo.setAttribute(
      "uv",
      new THREE.Float32BufferAttribute(
        new Float32Array(geo.getAttribute("position").count * 2),
        2,
      ),
    );
    // Planar UVs per face keep armor texture visible on tops, cheeks and sides.
    const positions = geo.getAttribute("position"), normals = geo.getAttribute("normal");
    const uv = geo.getAttribute("uv");
    for (let i = 0; i < positions.count; i++) {
      const x = positions.getX(i), y = positions.getY(i), z = positions.getZ(i);
      const nx = Math.abs(normals.getX(i)), ny = Math.abs(normals.getY(i)), nz = Math.abs(normals.getZ(i));
      uv.setXY(i, (ny >= nx && ny >= nz ? x : nx > nz ? z : x) + 0.5,
        (ny >= nx && ny >= nz ? z : y) + 0.5);
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
  const root = new THREE.Group() as TankModel, hull = new THREE.Group(), turret = new THREE.Group();
  const scout = kind === "scout", heavy = kind === "heavy";
  const color = wreck ? 0x3c4650 : TEAM_COLORS[team];
  const dark = 0x13232c, steel = wreck ? 0x37424c : 0x637581;
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
      const z = -length * 0.37 + j * length * 0.74 / (wheels - 1);
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
        const skirt = box(heavy ? 0.12 : 0.07, heavy ? 0.3 : 0.26, length / panels - 0.04,
          heavy && j % 2 ? shade : color, 0);
        put(hull, skirt, trackX + side * (heavy ? 0.17 : 0.19), deck - 0.17, -length / 2 + (j + 0.5) * length / panels);
      }
    }
    put(hull, box(0.16, 0.11, 0.1, 0xd9e6df, 0), side * 0.65, deck - 0.1, length / 2 - 0.05);
  }
  // Exposed rear engine deck gives the hull a direction even with the turret turned.
  for (let i = 0; i < 7; i++)
    put(hull, box(width * 0.53, 0.025, 0.05, dark, 0), 0, deck + 0.025, -length / 2 + 0.12 + i * 0.075);
  put(hull, box(0.32, 0.045, 0.3, steel, 0), 0, deck + 0.02, length / 2 - 0.45);
  if (heavy) {
    for (const side of [-1, 1]) for (let j = 0; j < 3; j++) {
      const tile = box(0.31, 0.1, 0.24, shade, 0);
      tile.rotation.x = -0.18;
      put(hull, tile, side * (0.24 + j * 0.32), deck - 0.06, length / 2 - 0.3);
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
      for (const side of [-1, 1])
        put(turret, box(0.055, 0.055, 0.69, steel, 0), side * 0.93, y, -1.64);
    }
    for (const x of [-0.93, -0.46, 0, 0.46, 0.93])
      put(turret, box(0.04, 0.31, 0.04, steel, 0), x, deck + 0.335, -1.96);
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
  for (const [z, radius, length] of [[0.67, tubeRadius * 2.1, 0.42], [muzzleZ * 0.58, tubeRadius * 1.5, 0.36]]) {
    const sleeve = cylinder(radius, length, shade, 12);
    sleeve.rotation.x = Math.PI / 2;
    put(barrel, sleeve, 0, gunY, z);
  }
  if (scout) {
    // Squared muzzle brake is visually distinct from the two smoothbore guns.
    put(barrel, box(0.22, 0.17, 0.27, steel, 0), 0, gunY, muzzleZ - 0.135);
    for (const side of [-1, 1]) for (const z of [muzzleZ - 0.2, muzzleZ - 0.09])
      put(barrel, box(0.012, 0.1, 0.05, dark, 0), side * 0.111, gunY, z);
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
  } else for (const x of [-0.07, 0.07])
    put(turret, box(0.06, 0.025, 0.2, 0xdce7ee, 0), x, roof + 0.018, 0.12);
  root.userData = { kind, hull, turret, barrel, trackGroup, muzzle: bore };
  applyTankSurface(root, [color, shade, steel]);
  return root;
}
// At most 3 chassis × 2 teams × 4 assemblies. Shared geometry lives across rounds.
const wreckTemplates = new Map<string, THREE.Group>();
/** Extract, center and batch once; instances share geometry but own their transforms. */
export function wreckModel(kind: VehicleKind, team: Team, part: WreckPart) {
  const key = `${kind}/${team}/${part}`;
  const cached = wreckTemplates.get(key);
  if (cached) return cached.clone();
  const source = tankModel(kind, team),
    result = new THREE.Group();
  const { hull, turret, barrel } = source.userData;
  if (part === "hull") result.add(hull);
  else if (part === "barrel") result.add(barrel);
  else {
    if (part === "turret") turret.remove(barrel);
    result.add(turret);
  }
  const center = new THREE.Box3()
    .setFromObject(result)
    .getCenter(new THREE.Vector3());
  for (const child of result.children) child.position.sub(center);
  result.updateMatrixWorld(true);
  const meshes: THREE.Mesh[] = [];
  result.traverse(o => { if (o instanceof THREE.Mesh) meshes.push(o); });
  const flat = new THREE.Group();
  for (const mesh of meshes) {
    mesh.applyMatrix4(mesh.parent!.matrixWorld);
    flat.add(mesh);
  }
  batch(flat);
  for (const mesh of flat.children as THREE.Mesh[]) mesh.geometry.userData.owned = false;
  wreckTemplates.set(key, flat);
  return flat.clone();
}
function towerFoundation(g: THREE.Group, x: number) {
  put(g, concreteWall(TOWER_BASE.width, TOWER_BASE.height, TOWER_BASE.depth),
    x, TOWER_BASE.height / 2, 0);
}
function towerPost(height: number) {
  // Turn the long axis of the boards upright for continuous vertical wood grain.
  const post = sidingBox(height, 0.35, 0.35, 0x887454);
  post.rotation.z = Math.PI / 2;
  return post;
}
export function coverModel(
  c: Pick<Cover, "kind" | "x" | "z" | "w" | "d" | "h" | "color" | "debrisSeed">,
  detail: "full" | "background" = "full",
  damageStage = 0,
) {
  if (c.kind === "tree") return treeModel(c, detail);
  const g = new THREE.Group();
  g.position.set(c.x, 0, c.z);
  if (c.kind === "house") {
    const wall = c.h * 0.68;
    put(g, box(c.w + 0.2, 0.22, c.d + 0.2, 0xa1977c, 0), 0, 0.11, 0);
    put(g, sidingBox(c.w, wall, c.d, c.color), 0, wall / 2, 0);
    // Pale corner boards and a stone sill frame the clapboard walls.
    for (const x of [-1, 1]) for (const z of [-1, 1])
      put(g, box(0.14, wall, 0.14, 0xd4be95, 0), x * c.w / 2, wall / 2, z * c.d / 2);
    for (const side of [-1, 1]) {
      put(g, box(c.w + 0.16, 0.16, 0.12, 0x856447, 0), 0, 0.28, side * c.d / 2);
      put(g, box(0.12, 0.16, c.d + 0.16, 0x856447, 0), side * c.w / 2, 0.28, 0);
    }
    for (const side of [-1, 1]) {
      for (const x of [-c.w * 0.29, c.w * 0.29]) {
        put(g, box(1.24, 1.16, 0.1, 0xe5cea1, 0), x, wall * 0.59, side * (c.d / 2 + 0.025));
        put(g, box(1.36, 0.1, 0.25, 0xc8b087, 0), x, wall * 0.59 - 0.6, side * (c.d / 2 + 0.09));
        for (const shutter of [-1, 1]) {
          put(g, box(0.22, 1.05, 0.1, 0x4d6650, 0), x + shutter * 0.75, wall * 0.59, side * (c.d / 2 + 0.06));
          for (const y of [-0.3, 0, 0.3])
            put(g, box(0.24, 0.035, 0.11, 0x334a3c, 0), x + shutter * 0.75, wall * 0.59 + y, side * (c.d / 2 + 0.08));
        }
        put(
          g,
          box(1.05, 0.97, 0.07, 0xffd94e, 0),
          x,
          wall * 0.59,
          side * (c.d / 2 + 0.045),
        );
        put(
          g,
          box(0.075, 0.97, 0.085, 0x875534, 0),
          x,
          wall * 0.59,
          side * (c.d / 2 + 0.09),
        );
        put(
          g,
          box(1.05, 0.075, 0.085, 0x875534, 0),
          x,
          wall * 0.59,
          side * (c.d / 2 + 0.09),
        );
      }
      put(
        g,
        box(0.07, 1.05, 1.1, 0xffd94e, 0),
        side * (c.w / 2 + 0.05),
        wall * 0.58,
        0,
      );
    }
    for (const side of [-1, 1]) {
      put(g, box(0.08, 1.22, 1.28, 0xe5cea1, 0), side * (c.w / 2 + 0.01), wall * 0.58, 0);
      put(g, box(0.10, 1.05, 0.07, 0x875534, 0), side * (c.w / 2 + 0.09), wall * 0.58, 0);
      put(g, box(0.10, 0.07, 1.1, 0x875534, 0), side * (c.w / 2 + 0.09), wall * 0.58, 0);
      put(g, box(0.25, 0.1, 1.36, 0xc8b087, 0), side * (c.w / 2 + 0.07), wall * 0.58 - 0.65, 0);
    }
    put(g, box(1.03, 1.72, 0.11, 0xe5cea1, 0), 0, 0.88, c.d / 2 + 0.015);
    put(g, box(1.2, 0.18, 0.62, 0x9a9585, 0), 0, 0.14, c.d / 2 + 0.2);
    put(g, box(0.82, 1.55, 0.1, 0x64452f, 0), 0, 0.85, c.d / 2 + 0.06);
    put(g, box(0.1, 0.1, 0.12, 0xffd24a, 0), 0.24, 0.83, c.d / 2 + 0.12);
    for (const y of [0.5, 1.15])
      put(g, box(0.6, 0.42, 0.035, 0x805b3d, 0), 0, y, c.d / 2 + 0.12);
    // Gentle paint weathering varies per cottage without splitting material batches.
    const roofColor = new THREE.Color(Math.abs(c.z) > 35 ? 0xcc493c : 0x167857)
      .multiplyScalar(0.9 + 0.12 * (0.5 + 0.5 * Math.sin(c.x * 3.7 + c.z * 1.9)))
      .getHex();
    put(
      g,
      sidingGable(c.w + 0.6, c.h - wall, c.d + 0.6, roofColor),
      0,
      wall,
      0,
    );
    put(g, shingleRoof(c.w + 0.6, c.h - wall, c.d + 0.6, roofColor), 0, wall, 0);
    for (const side of [-1, 1])
      put(g, box(0.16, 0.15, c.d + 0.7, 0xe0c79d, 0), side * (c.w + 0.6) / 2, wall, 0);
    for (let z = -(c.d + 0.6) / 2; z < (c.d + 0.6) / 2; z += 0.48)
      put(g, box(0.22, 0.11, Math.min(0.46, (c.d + 0.6) / 2 - z), 0x334a40, 0), 0, c.h + 0.04, z + 0.23);
    put(g, box(0.74, 0.14, 0.74, 0x705a4d, 0), -c.w * 0.25, c.h + 0.19, -c.d * 0.2);
    put(g, box(0.43, 0.015, 0.43, 0x302c29, 0), -c.w * 0.25, c.h + 0.27, -c.d * 0.2);
    for (let y = c.h - 0.75; y < c.h + 0.12; y += 0.22) {
      put(g, box(0.59, 0.026, 0.59, 0xd3b095, 0), -c.w * 0.25, y, -c.d * 0.2);
    }
    put(
      g,
      box(0.58, 1.0, 0.58, 0xbc5c3e, 0),
      -c.w * 0.25,
      c.h - 0.36,
      -c.d * 0.2,
    );
  } else if (c.kind === "timber") {
    g.userData.damageStage = damageStage;
    const along = c.w > c.d, length = Math.max(c.w, c.d);
    const colors = [c.color, 0x94613e, 0xa66f46];
    // Closely stacked beams stay opaque at shell height, even when chipped.
    for (let row = 0; row < 7; row++) {
      const chipped = damageStage > 0 && row >= 7 - damageStage * 2;
      const span = length - (chipped ? 0.35 + (row % 2) * 0.4 : 0.04);
      const beam = box(along ? span : 0.64, 0.38, along ? 0.64 : span,
        damageStage === 2 ? 0x795035 : colors[row % 3], 0);
      put(g, beam, along && chipped ? (row % 2 ? -0.16 : 0.16) : 0,
        0.2 + row * 0.39, !along && chipped ? (row % 2 ? -0.16 : 0.16) : 0);
    }
    for (const offset of [-length / 2 + 0.22, length / 2 - 0.22]) {
      put(g, box(along ? 0.3 : 0.9, c.h, along ? 0.9 : 0.3, 0x805336, 0),
        along ? offset : 0, c.h / 2, along ? 0 : offset);
      for (const y of [0.6, 2.15])
        put(g, box(along ? 0.32 : 0.92, 0.09, along ? 0.92 : 0.32, 0x49423a, 0),
          along ? offset : 0, y, along ? 0 : offset);
    }
  } else if (c.kind === "fence") {
    const along = c.w > c.d,
      length = Math.max(c.w, c.d);
    for (let offset = -length / 2 + 0.12; offset <= length / 2; offset += 0.55)
      put(
        g,
        box(along ? 0.24 : 0.18, c.h, along ? 0.18 : 0.24, c.color, 0),
        along ? offset : 0,
        c.h / 2,
        along ? 0 : offset,
      );
    for (const y of [0.45, 1.12])
      put(
        g,
        box(along ? length : 0.2, 0.18, along ? 0.2 : length, 0x8f603a, 0),
        0,
        y,
        0,
      );
  } else if (c.kind === "drum") {
    put(g, explosiveBarrel(), 0, 0.8, 0);
    for (const y of [0.22, 1.35])
      put(g, cylinder(0.63, 0.1, 0x574e3e), 0, y, 0);
    put(g, cylinder(0.15, 0.05, 0x343c31), 0.25, 1.63, 0);
  } else if (c.kind === "tower") {
    for (const side of [-1, 1]) {
      const x = side * TOWER_BASE.offset;
      towerFoundation(g, x);
      for (const z of [-TOWER_BASE.postZ, TOWER_BASE.postZ])
        put(g, towerPost(4.3), x, TOWER_BASE.height + 2.15, z);
      // Cross bracing terminates at the same posts that survive the collapse.
      for (const direction of [-1, 1]) {
        const brace = sidingBox(0.18, 4.35, 0.18, 0x96734c);
        brace.rotation.x = direction * Math.atan2(2 * TOWER_BASE.postZ, 3.8);
        put(g, brace, x, 2.85, 0);
      }
    }
    put(g, sidingBox(6, 0.35, 5, 0x887d59), 0, 5, 0);
    put(g, sidingBox(5.7, 2.15, 4.7, c.color), 0, 6.15, 0);
    for (const z of [-2.4, 2.4])
      put(g, box(4, 0.65, 0.08, 0x164e79), 0, 6.4, z);
    put(g, sidingGable(6.5, 1.2, 5.5, 0x197451), 0, 7.25, 0);
    put(g, shingleRoof(6.5, 1.2, 5.5, 0x197451), 0, 7.25, 0);
    for (const x of [2.2, 3.1])
      put(g, towerPost(4.9), x, 2.45, 2.15);
    for (let i = 0; i < 9; i++)
      put(g, sidingBox(0.9, 0.08, 0.18, 0xe2cc93), 2.65, 0.4 + i * 0.55, 2.15);
  } else if (c.kind === "rubble") {
    towerFoundation(g, 0);
    const rng = new Random(c.debrisSeed ?? Math.round(c.x * 73856093 + c.z * 19349663));
    const choose = (values: number[]) => values[Math.floor(rng.next() * values.length)];
    for (const z of [-TOWER_BASE.postZ, TOWER_BASE.postZ]) {
      // Cut posts keep their original position, section and grain direction.
      const height = choose([0.12, 0.2, 0.28, 0.34]);
      put(g, towerPost(height), 0, TOWER_BASE.height + height / 2, z);
      if (rng.next() < 0.7) {
        const splinter = sidingBox(0.09, 0.12, 0.16, 0xc5a073);
        splinter.rotation.z = rng.range(-0.4, 0.4);
        put(g, splinter, rng.range(-0.1, 0.1), TOWER_BASE.height + height - 0.01, z);
      }
    }
    // Discrete sizes reuse cached geometry; each foundation gets its own scatter.
    const count = choose([2, 3, 4]);
    for (let i = 0; i < count; i++) {
      const width = choose([0.16, 0.3, 0.55]);
      const length = choose([0.7, 1.1, 1.5]);
      const yaw = rng.range(-0.55, 0.55);
      const board = sidingBox(width, 0.09, length, choose([c.color, 0x887d59, 0x96734c]));
      board.rotation.y = yaw;
      // Keep the pile inside its foundation, preserving the opened center route.
      const roomX = Math.max(0, (TOWER_BASE.width - width * Math.cos(yaw) - length * Math.abs(Math.sin(yaw))) / 2 - 0.02);
      const roomZ = (TOWER_BASE.depth - length * Math.cos(yaw) - width * Math.abs(Math.sin(yaw))) / 2 - 0.02;
      put(g, board, rng.range(-roomX, roomX), TOWER_BASE.height + 0.045 + i * 0.055,
        rng.range(-roomZ, roomZ));
    }
  } else if (c.kind === "shed") {
    put(g, box(c.w, c.h, c.d, c.color), 0, c.h / 2, 0);
    const along = c.w > c.d;
    const length = along ? c.w : c.d;
    for (let i = -length / 2 + 0.2; i < length / 2; i += 0.52)
      put(
        g,
        box(
          along ? 0.035 : c.w + 0.04,
          c.h - 0.1,
          along ? c.d + 0.04 : 0.035,
          0x9c713e,
          0.005,
        ),
        along ? i : 0,
        c.h / 2,
        along ? 0 : i,
      );
    put(g, box(c.w + 0.14, 0.18, c.d + 0.14, 0x187fbe), 0, c.h, 0);
  } else {
    put(
      g,
      c.kind === "boundary" ? concreteWall(c.w, c.h, c.d)
        : box(c.w, c.h, c.d, c.color, 0.16),
      0,
      c.h / 2,
      0,
    );
    if (c.kind === "concrete" || c.kind === "wall") {
      const along = c.w > c.d;
      for (let i = 0; i < Math.floor((along ? c.w : c.d) / 1.1); i++) {
        const mark = box(
          along ? 0.55 : 0.035,
          0.22,
          along ? 0.035 : 0.55,
          0x499ac7,
          0.01,
        );
        put(
          g,
          mark,
          along ? -c.w / 2 + 0.6 + i * 1.1 : c.w / 2 + 0.02,
          c.h * 0.7,
          along ? c.d / 2 + 0.02 : -c.d / 2 + 0.6 + i * 1.1,
        );
      }
    }
  }
  return g;
}
const teamTextures = new Map<number, THREE.Texture>();
export function teamTexture(team: number) {
  let texture = teamTextures.get(team);
  if (!texture) {
    texture = new THREE.TextureLoader().load(
      `${import.meta.env.BASE_URL}textures/teams/${team === 0 ? "blue" : "red"}.png`,
    );
    teamTextures.set(team, texture);
  }
  return texture;
}

import * as THREE from "three";
import { batch } from "./batching";
import { Random } from "./data";
import type { Cover } from "./types";

type TreeDef = Pick<Cover, "x" | "z" | "w" | "d" | "h">;
export const TREE_FAMILIES = ["Pine", "Spruce", "Fir", "Oak", "Birch", "Aspen"] as const;
const textures = new Map<string, THREE.Texture>();
const materials = new Map<string, THREE.MeshStandardMaterial>();
function surface(kind: "bark" | "birch" | "rings" | "leaves" | "needles", color: number) {
  const key = `${kind}/${color}`;
  let mat = materials.get(key);
  if (!mat) {
    let map = textures.get(kind);
    if (!map) {
      map = new THREE.TextureLoader().load(
        `${import.meta.env?.BASE_URL ?? "/"}textures/trees/${kind}.${kind === "bark" ? "webp" : "png"}`,
      );
      map.colorSpace = THREE.SRGBColorSpace;
      map.wrapS = map.wrapT = THREE.RepeatWrapping;
      map.anisotropy = 4;
      textures.set(kind, map);
    }
    mat = new THREE.MeshStandardMaterial({
      map,
      color,
      roughness: 1,
      metalness: 0,
      bumpMap: map,
      bumpScale: kind === "bark" ? 0.055 : 0.018,
    });
    materials.set(key, mat);
  }
  return mat;
}
const stemGeometry = new THREE.CylinderGeometry(0.6, 1, 1, 8, 1, true);
const branchGeometry = new THREE.CylinderGeometry(0.6, 1, 1, 5, 1, true);
const rootGeometry = new THREE.CylinderGeometry(0.08, 1, 1, 5, 1, true);
const broadGeometry = new THREE.IcosahedronGeometry(1, 1);
const smallCrownGeometry = new THREE.IcosahedronGeometry(1, 0);
const boughGeometry = (() => {
  const geo = new THREE.ConeGeometry(1, 1, 10, 1).toNonIndexed();
  const p = geo.getAttribute("position");
  for (let i = 0; i < p.count; i++) {
    const angle = Math.atan2(p.getZ(i), p.getX(i));
    const bottom = 0.5 - p.getY(i);
    const scallop = 0.9 + Math.cos(angle * 5) * 0.1;
    p.setXYZ(
      i,
      p.getX(i) * scallop,
      p.getY(i) + bottom * Math.cos(angle * 5) * 0.1,
      p.getZ(i) * scallop,
    );
  }
  geo.computeVertexNormals();
  return geo;
})();
const up = new THREE.Vector3(0, 1, 0);
function mesh(
  parent: THREE.Group,
  geo: THREE.BufferGeometry,
  mat: THREE.MeshStandardMaterial,
  x: number,
  y: number,
  z: number,
  sx = 1,
  sy = 1,
  sz = sx,
) {
  const m = new THREE.Mesh(geo, mat);
  m.position.set(x, y, z);
  m.scale.set(sx, sy, sz);
  m.castShadow = m.receiveShadow = true;
  parent.add(m);
  return m;
}
function limb(
  parent: THREE.Group,
  mat: THREE.MeshStandardMaterial,
  from: THREE.Vector3,
  to: THREE.Vector3,
  radius: number,
  root = false,
): void {
  const delta = to.clone().sub(from);
  const m = mesh(
    parent,
    root ? rootGeometry : radius < 0.15 ? branchGeometry : stemGeometry,
    mat,
    (from.x + to.x) / 2,
    (from.y + to.y) / 2,
    (from.z + to.z) / 2,
    radius,
    delta.length(),
    radius,
  );
  m.quaternion.setFromUnitVectors(up, delta.normalize());
}

// A bounded set of reusable stump meshes: irregular flared bark and matching cut surface.
const stumpGeometries = new Map<
  number,
  { bark: THREE.BufferGeometry; cut: THREE.BufferGeometry }
>();
function stumpGeometry(variant: number) {
  let cached = stumpGeometries.get(variant);
  if (cached) {
    return cached;
  }
  const rng = new Random(variant * 17597 + 79);
  const sides = 10;
  const radii = Array.from({ length: sides }, () => rng.range(0.9, 1.1));
  const tops = Array.from({ length: sides }, () => rng.range(0.91, 1.09));
  const vertices: number[] = [];
  const uvs: number[] = [];
  const indices: number[] = [];
  for (let row = 0; row < 3; row++) {
    for (let i = 0; i <= sides; i++) {
      const j = i % sides;
      const a = (j / sides) * Math.PI * 2;
      const radius = radii[j] * [1.5, 1.12, 1][row];
      vertices.push(Math.sin(a) * radius, row === 2 ? tops[j] : row * 0.42, Math.cos(a) * radius);
      uvs.push(i / sides, row === 2 ? tops[j] * 0.55 : row * 0.23);
      if (row < 2 && i < sides) {
        const n = row * (sides + 1) + i;
        indices.push(n, n + 1, n + sides + 1, n + 1, n + sides + 2, n + sides + 1);
      }
    }
  }
  const bark = new THREE.BufferGeometry();
  bark.setAttribute("position", new THREE.Float32BufferAttribute(vertices, 3));
  bark.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
  bark.setIndex(indices);
  bark.computeVertexNormals();
  const capVertices: number[] = [];
  const capUV: number[] = [];
  for (let i = 0; i < sides; i++) {
    for (const j of [-1, i, (i + 1) % sides]) {
      const a = (j / sides) * Math.PI * 2;
      const x = j < 0 ? 0 : Math.sin(a) * radii[j];
      const z = j < 0 ? 0 : Math.cos(a) * radii[j];
      capVertices.push(x, j < 0 ? 0.91 : tops[j], z);
      capUV.push(0.5 + x / 2.4, 0.5 + z / 2.4);
    }
  }
  const cut = new THREE.BufferGeometry();
  cut.setAttribute("position", new THREE.Float32BufferAttribute(capVertices, 3));
  cut.setAttribute("uv", new THREE.Float32BufferAttribute(capUV, 2));
  cut.computeVertexNormals();
  cached = { bark, cut };
  stumpGeometries.set(variant, cached);
  return cached;
}

export function treeModel(c: TreeDef, detail: "full" | "background" = "full") {
  const seed = ((Math.round(c.x * 100) * 73856093) ^ (Math.round(c.z * 100) * 19349663)) >>> 0;
  const rng = new Random(seed);
  const family = Math.floor(rng.next() * TREE_FAMILIES.length);
  const conifer = family < 3;
  const twist = rng.range(0, Math.PI * 2);
  const height = c.h * rng.range(0.9, 1.07);
  const radius = Math.min(c.w, c.d) * (family === 3 ? 0.14 : family >= 4 ? 0.1 : 0.12);
  const stumpHeight = radius * rng.range(1.5, 1.9);
  const pale = family === 4 || family === 5;
  const barkMat = surface(
    pale ? "birch" : "bark",
    pale ? [0xe5ddc5, 0xc4c6a0][family - 4] : 0xd0b598,
  );
  const leafColors = [0x3a7847, 0x367267, 0x528746, 0x5c8c35, 0x80a64c, 0x9aae43];
  const shades = [0xb1c7a4, 0xd9e2c0, 0xffffff];
  const leafMats = shades.map((tint) =>
    surface(
      conifer ? "needles" : "leaves",
      new THREE.Color(leafColors[family]).multiply(new THREE.Color(tint)).getHex(),
    ),
  );
  const group = new THREE.Group();
  group.position.set(c.x, 0, c.z);
  group.userData.family = TREE_FAMILIES[family];
  group.userData.seed = seed;
  const crown = detail === "full" ? new THREE.Group() : group;
  crown.name = "trunk-and-crown";
  if (detail === "full") {
    const stump = new THREE.Group();
    stump.name = "rooted-stump";
    const geometry = stumpGeometry(seed % 24);
    mesh(stump, geometry.bark, barkMat, 0, 0, 0, radius, stumpHeight, radius).rotation.y = twist;
    const rootCount = 5 + (seed % 3);
    for (let i = 0; i < rootCount; i++) {
      const angle = twist + (i * Math.PI * 2) / rootCount + rng.range(-0.18, 0.18);
      const reach = radius * rng.range(2, 3);
      limb(
        stump,
        barkMat,
        new THREE.Vector3(
          Math.sin(angle) * radius * 0.5,
          stumpHeight * 0.5,
          Math.cos(angle) * radius * 0.5,
        ),
        new THREE.Vector3(Math.sin(angle) * reach, 0.035, Math.cos(angle) * reach),
        radius * rng.range(0.3, 0.5),
        true,
      );
    }
    batch(stump);
    const cut = mesh(
      stump,
      geometry.cut,
      surface("rings", 0xd9b77f),
      0,
      0,
      0,
      radius,
      stumpHeight,
      radius,
    );
    cut.rotation.y = twist;
    cut.name = "exposed-wood";
    cut.visible = false;
    group.add(stump, crown);
    group.userData.cutSurface = cut;
    group.userData.crown = crown;
    group.userData.stump = false;
  }
  const leanX = rng.range(-0.1, 0.1) * c.w;
  const leanZ = rng.range(-0.07, 0.07) * c.d;
  limb(
    crown,
    barkMat,
    new THREE.Vector3(0, detail === "full" ? stumpHeight * 0.88 : 0, 0),
    new THREE.Vector3(leanX, height * 0.78, leanZ),
    radius,
  );
  if (conifer) {
    const tiers = detail === "background" ? 4 : family === 1 ? 6 : 5;
    for (let i = 0; i < tiers; i++) {
      const t = i / (tiers - 1);
      const y = height * (0.32 + t * 0.58);
      const span = c.w * (family === 2 ? 0.42 : 0.48) * (1 - t * 0.78);
      const h = height * (family === 0 ? 0.29 : 0.26) * (1 - t * 0.25);
      const layer = mesh(
        crown,
        boughGeometry,
        leafMats[i % 3],
        leanX * t,
        y,
        leanZ * t,
        span,
        h,
        span * rng.range(0.87, 1.02),
      );
      layer.rotation.y = twist + i * 0.73;
      if (detail === "full" && i < tiers - 1) {
        for (let j = 0; j < 3; j++) {
          const angle = twist + i * 0.9 + (j * Math.PI * 2) / 3;
          const tip = new THREE.Vector3(
            Math.sin(angle) * span * 0.72,
            y - h * 0.1,
            Math.cos(angle) * span * 0.72,
          );
          limb(
            crown,
            barkMat,
            new THREE.Vector3(leanX * t, y - h * 0.3, leanZ * t),
            tip,
            radius * 0.19,
          );
          const tuft = mesh(
            crown,
            boughGeometry,
            leafMats[(i + j + 1) % 3],
            tip.x,
            tip.y,
            tip.z,
            span * 0.42,
            h * 0.48,
            span * 0.42,
          );
          tuft.rotation.set(Math.cos(angle) * 0.24, angle, -Math.sin(angle) * 0.24);
        }
      }
    }
  } else {
    const count = detail === "background" ? 6 : 7;
    for (let i = 0; i < count; i++) {
      const a = twist + i * 2.39996;
      const t = i / (count - 1);
      const spread = (1 - t * 0.65) * (family === 3 ? 0.27 : 0.2);
      const center = new THREE.Vector3(
        leanX + Math.sin(a) * c.w * spread,
        height * (0.48 + t * 0.37),
        leanZ + Math.cos(a) * c.d * spread,
      );
      if (detail === "full") {
        limb(
          crown,
          barkMat,
          new THREE.Vector3(leanX * 0.5, center.y - height * 0.2, leanZ * 0.5),
          center,
          radius * (0.32 - t * 0.16),
        );
      }
      const size = c.w * (family === 3 ? 0.3 : 0.25) * rng.range(0.84, 1.09);
      const leaves = mesh(
        crown,
        detail === "full" ? broadGeometry : smallCrownGeometry,
        leafMats[i % 3],
        center.x,
        center.y,
        center.z,
        size,
        height * (family === 3 ? 0.155 : 0.185),
        size * rng.range(0.8, 1.07),
      );
      leaves.rotation.set(rng.range(-0.3, 0.3), a, rng.range(-0.2, 0.2));
    }
  }
  if (detail === "full") {
    batch(crown);
  }
  return group;
}

export function setTreeDestroyed(tree: THREE.Group, destroyed: boolean): void {
  // Background trees omit the separately retained stump assembly.
  const crown = tree.userData.crown as THREE.Group | undefined;
  const cutSurface = tree.userData.cutSurface as THREE.Mesh | undefined;
  if (!crown || !cutSurface) {
    return;
  }
  crown.visible = !destroyed;
  cutSurface.visible = destroyed;
  tree.userData.stump = destroyed;
}

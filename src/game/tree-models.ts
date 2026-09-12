import * as THREE from "three";
import { batch } from "./batching";
import { Random } from "./data";
import type { Cover } from "./types";

type TreeDef = Pick<Cover, "x" | "z" | "w" | "d" | "h">;
export const TREE_FAMILIES = ["Pine", "Spruce", "Fir", "Oak", "Birch", "Aspen"] as const;
const textures = new Map<string, THREE.Texture>();
const materials = new Map<string, THREE.MeshStandardMaterial>();
function surface(kind: "bark" | "birch" | "rings" | "leaves" | "conifer-spray", color: number) {
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
      bumpMap: kind === "conifer-spray" ? null : map,
      bumpScale: kind === "bark" ? 0.055 : 0.018,
      ...(kind === "conifer-spray"
        ? { alphaTest: 0.35, alphaToCoverage: true, side: THREE.DoubleSide }
        : {}),
    });
    materials.set(key, mat);
  }
  return mat;
}
const stemGeometry = new THREE.CylinderGeometry(0.6, 1, 1, 8, 1, true);
const coniferStemGeometry = new THREE.CylinderGeometry(0.025, 1, 1, 7, 1, true);
const branchGeometry = new THREE.CylinderGeometry(0.6, 1, 1, 5, 1, true);
const rootGeometry = new THREE.CylinderGeometry(0.08, 1, 1, 5, 1, true);
const broadGeometry = new THREE.IcosahedronGeometry(1, 1);
const smallCrownGeometry = new THREE.IcosahedronGeometry(1, 0);
// Three intersecting needle cards retain volume from the overhead camera and at the horizon.
// A complete spray is six triangles; even distant trees get individual branching silhouettes.
const sprayGeometry = (() => {
  const vertices: number[] = [];
  const uv: number[] = [];
  const indices: number[] = [];
  for (let card = 0; card < 3; card++) {
    const angle = (card * Math.PI) / 3;
    for (const [x, z] of [
      [-0.5, 0],
      [0.5, 0],
      [-0.5, 1],
      [0.5, 1],
    ]) {
      vertices.push(x * Math.cos(angle), x * Math.sin(angle), z);
      uv.push(x + 0.5, z);
    }
    const n = card * 4;
    indices.push(n, n + 2, n + 1, n + 1, n + 2, n + 3);
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(vertices, 3));
  geometry.setAttribute("uv", new THREE.Float32BufferAttribute(uv, 2));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return geometry;
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
) {
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
  return m;
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
  const leafColors = [0x9eb783, 0x80a69a, 0xa3bd8e, 0x5c8c35, 0x80a64c, 0x9aae43];
  const shades = [0xb1c7a4, 0xd9e2c0, 0xffffff];
  const leafMats = shades.map((tint) =>
    surface(
      conifer ? "conifer-spray" : "leaves",
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
  const branches: THREE.Group[] = [];
  group.userData.branches = branches;
  const branchParent = (stage: number) => {
    if (detail === "background" || stage === 0) {
      return crown;
    }
    const branch = new THREE.Group();
    branch.name = "shedding-branch";
    branch.userData.dropStage = stage;
    crown.add(branch);
    branches.push(branch);
    return branch;
  };
  const leanX = rng.range(-0.1, 0.1) * c.w;
  const leanZ = rng.range(-0.07, 0.07) * c.d;
  const trunk = limb(
    crown,
    barkMat,
    new THREE.Vector3(0, detail === "full" ? stumpHeight * 0.88 : 0, 0),
    new THREE.Vector3(leanX, height * (conifer ? 0.98 : 0.78), leanZ),
    radius,
  );
  if (conifer) {
    trunk.geometry = coniferStemGeometry;
    const tiers = detail === "background" ? 6 : 7;
    const arms = detail === "background" ? 5 : 6;
    // Pines carry a looser, higher crown; spruce and fir retain their lower boughs.
    const base = family === 0 ? 0.36 : family === 1 ? 0.17 : 0.23;
    for (let i = 0; i < tiers; i++) {
      const t = i / (tiers - 1);
      const y = height * (base + t * (0.87 - base));
      const span = c.w * (family === 2 ? 0.44 : 0.5) * (1 - t * 0.76);
      for (let j = 0; j < arms; j++) {
        const angle = twist + i * 2.39996 + (j * Math.PI * 2) / arms + rng.range(-0.24, 0.24);
        const reach = span * rng.range(0.78, 1.16);
        const start = new THREE.Vector3(
          leanX * (base + t * (1 - base)),
          y + rng.range(-0.05, 0.05) * height,
          leanZ * (base + t * (1 - base)),
        );
        // An upright inner shoot fills the crown between whorls without solid foliage cones.
        if (j === arms - 1) {
          const shoot = mesh(
            crown,
            sprayGeometry,
            leafMats[i % 3],
            start.x,
            start.y - height * 0.06,
            start.z,
            span * 0.85,
            span * 0.85,
            height * (0.27 - t * 0.1),
          );
          shoot.rotation.x = -Math.PI / 2;
          shoot.rotateZ(angle);
          continue;
        }
        const parent = branchParent(
          (i === 1 && j === 0) || (i === 2 && j === 3)
            ? 1
            : (i === 0 && j === 2) || (i === 3 && j === 1)
              ? 2
              : 0,
        );
        const rise = reach * (family === 0 ? 0.24 : family === 1 ? -0.16 : 0.06);
        if (detail === "full" && i < tiers - 2) {
          limb(
            parent,
            barkMat,
            start,
            new THREE.Vector3(
              start.x + Math.sin(angle) * reach * 0.92,
              start.y + rise,
              start.z + Math.cos(angle) * reach * 0.92,
            ),
            radius * (0.17 - t * 0.1),
          );
        }
        const spray = mesh(
          parent,
          sprayGeometry,
          leafMats[(i + j) % 3],
          start.x,
          start.y,
          start.z,
          reach * (family === 0 ? 0.95 : 0.85),
          height * (0.25 - t * 0.12),
          reach * 1.1,
        );
        spray.rotation.set(-Math.atan2(rise, reach), angle, rng.range(-0.2, 0.2), "YXZ");
      }
    }
    const leader = mesh(
      crown,
      sprayGeometry,
      leafMats[2],
      leanX * 0.9,
      height * 0.82,
      leanZ * 0.9,
      c.w * 0.2,
      c.w * 0.2,
      height * 0.2,
    );
    leader.rotation.x = -Math.PI / 2;
    leader.rotateZ(twist);
  } else {
    const count = detail === "background" ? 6 : 7;
    for (let i = 0; i < count; i++) {
      const parent = branchParent(i < 2 ? 1 : i === 3 || i === 4 ? 2 : 0);
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
          parent,
          barkMat,
          new THREE.Vector3(leanX * 0.5, center.y - height * 0.2, leanZ * 0.5),
          center,
          radius * (0.32 - t * 0.16),
        );
      }
      const size = c.w * (family === 3 ? 0.3 : 0.25) * rng.range(0.84, 1.09);
      const leaves = mesh(
        parent,
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
    group.updateMatrixWorld(true);
    for (const branch of branches) {
      batch(branch);
      // Pivot each falling bough around its own center, not around the tree trunk.
      const center = new THREE.Box3().setFromObject(branch).getCenter(new THREE.Vector3());
      center.sub(group.position);
      for (const child of branch.children as THREE.Mesh[]) {
        child.geometry.translate(-center.x, -center.y, -center.z);
      }
      branch.position.copy(center);
    }
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

/** Shed two boughs after the first damage, then two more at 35% health. */
export function setTreeDamage(
  tree: THREE.Group,
  healthRatio: number,
  onDrop?: (branch: THREE.Group) => void,
): void {
  const stage = healthRatio >= 1 ? 0 : healthRatio > 0.35 ? 1 : 2;
  if (stage === (tree.userData.branchDamageStage ?? 0)) {
    return;
  }
  for (const branch of (tree.userData.branches ?? []) as THREE.Group[]) {
    const visible = branch.userData.dropStage > stage;
    if (branch.visible && !visible) {
      onDrop?.(branch);
    }
    branch.visible = visible;
  }
  tree.userData.branchDamageStage = stage;
}

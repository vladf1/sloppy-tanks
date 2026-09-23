import * as THREE from "three/webgpu";
import {
  abs,
  color,
  dot,
  float,
  max,
  mix,
  normalWorld,
  positionWorld,
  sin,
  smoothstep,
  texture as sampleTexture,
  vec2,
  vec3,
  vec4,
} from "three/tsl";
import { toCreasedNormals } from "three/addons/utils/BufferGeometryUtils.js";
import { Random } from "./math";
import { quarryRockShape } from "./quarry-rock-shape";
import { derivativeBump, quarryGrit } from "./quarry-grit";
import { quarrySoilAt } from "./quarry-terrain";

let stone: THREE.MeshStandardNodeMaterial | undefined;
const geometries = new Map<string, THREE.BufferGeometry>();

const LUMA = vec3(0.2126, 0.7152, 0.0722);
/** Linear mean luminance of sandstone.webp; dividing by it centres grain on 1. */
const STONE_MEAN = 0.4;

/** Height of the baked quarry floor: flat arena, 0.3 fall to the apron at -1.8. */
function floorHeight(x: THREE.Node<"float">, z: THREE.Node<"float">) {
  return max(abs(x), abs(z)).sub(60).max(0).mul(0.3).min(1.8).negate();
}

export function sandstoneMaterial(): THREE.MeshStandardNodeMaterial {
  if (!stone) {
    const texture = new THREE.TextureLoader().load(
      `${import.meta.env?.BASE_URL ?? "/"}textures/quarry/sandstone.webp`,
    );
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.wrapS = texture.wrapT = THREE.MirroredRepeatWrapping;
    texture.anisotropy = 4;
    stone = new THREE.MeshStandardNodeMaterial({
      map: texture,
      roughness: 0.95,
      vertexColors: true,
    });
    // Blend projections across rounded shoulders instead of exposing UV seams on
    // individual triangles. World-space sampling decorrelates reused rock
    // geometry so identical boulders never show the same patch of grain.
    const weights = normalWorld.abs().pow(6);
    const blend = weights.div(weights.x.add(weights.y).add(weights.z).max(0.0001));
    const p = positionWorld;
    const point = p.div(6.5);
    const grain = sampleTexture(texture, point.zy.add(vec2(0.31, 0.11)))
      .mul(blend.x)
      .add(sampleTexture(texture, point.xz.add(vec2(0.57, 0.43))).mul(blend.y))
      .add(sampleTexture(texture, point.xy.add(vec2(0.13, 0.79))).mul(blend.z)).rgb;
    // The photo is warm ochre; keep its relief and a little of its colour, and
    // let the strata below decide the hue.
    const relief = mix(vec3(dot(grain, LUMA)), grain, 0.35).div(STONE_MEAN);
    // Relief lighting from the same grain, with no separate bump-map reads.
    stone.normalNode = derivativeBump(dot(grain, LUMA), 0.6);
    // Undulating sediment: cream and rose beds of uneven thickness with thin
    // darker partings, continuous across every rock, wall and boulder.
    const warp = sin(p.x.mul(0.061).add(p.z.mul(0.047)))
      .mul(0.9)
      .add(sin(p.x.mul(0.19).sub(p.z.mul(0.23))).mul(0.35));
    const bed = p.y.add(warp);
    const broad = sin(bed.mul(1.3).add(sin(bed.mul(0.47)).mul(1.8)))
      .mul(0.5)
      .add(0.5);
    const parting = smoothstep(0.82, 1, sin(bed.mul(4.7).add(sin(bed.mul(1.9)).mul(2.1))));
    let layered = mix(color(0xdbc6a4), color(0xc99f7f), broad);
    // Broad tone shifts: grey-buff exposures and iron-stained patches.
    const tone = sin(p.x.mul(0.043).add(sin(p.z.mul(0.031)).mul(2)))
      .mul(sin(p.z.mul(0.057).add(p.y.mul(0.11)).sub(p.x.mul(0.02))))
      .mul(0.5)
      .add(0.5);
    layered = mix(layered, color(0xb8a58c), smoothstep(0.62, 0.9, tone).mul(0.55));
    layered = mix(layered, color(0xc78a5c), smoothstep(0.35, 0.08, tone).mul(0.35));
    // Weathering streaks run down steep faces, breaking the horizontal beds.
    const steep = float(1).sub(normalWorld.y.abs());
    const along = p.x.add(p.z);
    const streak = smoothstep(
      0.55,
      1,
      sin(along.mul(2.3).add(sin(along.mul(0.61)).mul(3))).mul(
        sin(along.mul(0.37).add(p.y.mul(0.35))),
      ),
    ).mul(steep);
    let rock = relief.mul(layered).mul(float(1).sub(parting.mul(0.15)).sub(streak.mul(0.14)));
    // Sand settles on ledges and caps in wind-shaped patches; feet are stained
    // by splashed soil and half-buried in drift. Both take the colour of the
    // baked ground beneath, so every rock meets the floor without a seam.
    const up = smoothstep(0.5, 0.92, normalWorld.y);
    const patch = sin(p.x.mul(0.37).add(sin(p.z.mul(0.29)).mul(1.7)))
      .mul(sin(p.z.mul(0.41).sub(p.x.mul(0.13))))
      .mul(0.5)
      .add(0.5);
    const ground = quarrySoilAt().rgb;
    const sand = ground.mul(dot(relief, LUMA).mul(0.3).add(0.75));
    rock = mix(rock, sand, up.mul(mix(0.2, 0.7, patch)));
    const foot = float(1).sub(smoothstep(0.02, 0.75, p.y.sub(floorHeight(p.x, p.z))));
    rock = mix(rock, ground.mul(0.9), foot.mul(0.6));
    stone.colorNode = vec4(rock, 1);
  }
  return stone;
}

/** Low-polygon ledges and fractured caps share their exact shape with collision. */
export function sandstoneRock(w: number, h: number, d: number, variant = 0): THREE.Mesh {
  const key = `${w}/${h}/${d}/${variant}`;
  let geometry = geometries.get(key);
  if (!geometry) {
    const rng = new Random(812 + variant);
    const shape = quarryRockShape(w, h, d, variant);
    const vertices = Array.from({ length: shape.positions.length / 3 }, (_, i) =>
      new THREE.Vector3().fromArray(shape.positions, i * 3),
    );
    const positions: number[] = [];
    const uvs: number[] = [];
    const colors: number[] = [];
    const triangle = (a: number, b: number, c: number, shade: number, warm: number) => {
      const normal = new THREE.Vector3()
        .subVectors(vertices[b], vertices[a])
        .cross(new THREE.Vector3().subVectors(vertices[c], vertices[a]))
        .normalize();
      // Dominant-axis projection prevents diagonal faces collapsing into streaks.
      // These UVs feed only the bump map; albedo uses world-space triplanar.
      const top = Math.abs(normal.y) > 0.65;
      const alongZ = Math.abs(normal.x) > Math.abs(normal.z);
      for (const index of [a, b, c]) {
        const p = vertices[index];
        positions.push(p.x, p.y, p.z);
        uvs.push((alongZ && !top ? p.z : p.x) / 5.5, (top ? p.z : p.y) / 5.5);
        const dust = top ? 1.07 : 0.88 + 0.12 * Math.min(1, p.y / h);
        colors.push(shade * dust * (1 + warm), shade * dust, shade * dust * (1 - warm));
      }
    };
    for (let i = 0; i < shape.indices.length; i += 3) {
      triangle(
        shape.indices[i],
        shape.indices[i + 1],
        shape.indices[i + 2],
        rng.range(0.9, 1.07),
        rng.range(-0.02, 0.035),
      );
    }
    geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
    geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
    geometry = toCreasedNormals(geometry, Math.PI / 5);
    geometries.set(key, geometry);
  }
  const mat = sandstoneMaterial();
  mat.vertexColors = true;
  const mesh = new THREE.Mesh(geometry, mat);
  mesh.castShadow = mesh.receiveShadow = true;
  return mesh;
}

export interface RubbleStone {
  x: number;
  y: number;
  z: number;
  /** Footprint and height, in metres. */
  w: number;
  h: number;
  d: number;
  rotY: number;
  /** Per-stone brightness; hue comes from the shared sandstone strata. */
  shade: number;
}

/** Push each corner of a unit stone in or out by a per-stone hash of that
 * corner, so shared corners stay welded and no two stones read as dice. */
export function roughenStone(vertex: THREE.Vector3, stone: number): THREE.Vector3 {
  const hash = Math.sin(
    stone * 12.9898 + vertex.x * 78.233 + vertex.y * 37.719 + vertex.z * 11.131,
  );
  const r = hash * 43758.5453 - Math.floor(hash * 43758.5453);
  return vertex.multiplyScalar(0.72 + r * 0.5);
}

const pebble = new THREE.IcosahedronGeometry(0.5, 0);
const pebbleMatrix = new THREE.Matrix4();
const pebbleRotation = new THREE.Euler();
const pebbleQuaternion = new THREE.Quaternion();
const pebbleScale = new THREE.Vector3();
const pebblePosition = new THREE.Vector3();

/** Hundreds of 20-triangle fragments merged into one sandstone mesh: loose
 * rubble and gravel where the full ledge rock would spend ten times the
 * triangles on stones a few pixels wide. Flat facets keep them angular. */
export function sandstoneRubble(stones: RubbleStone[]): THREE.Mesh {
  const template = pebble.index ? pebble.toNonIndexed() : pebble;
  const source = template.getAttribute("position");
  const perStone = source.count;
  const positions = new Float32Array(stones.length * perStone * 3);
  const uvs = new Float32Array(stones.length * perStone * 2);
  const colors = new Float32Array(stones.length * perStone * 3);
  const vertex = new THREE.Vector3();
  stones.forEach((stone, s) => {
    // A pebble-specific tilt: a few stones sit on edge, most lie flat.
    const jitter = (s * 7919) % 13;
    pebbleRotation.set((jitter - 6) * 0.05, stone.rotY, (((jitter * 5) % 13) - 6) * 0.04);
    pebbleMatrix.compose(
      pebblePosition.set(stone.x, stone.y, stone.z),
      pebbleQuaternion.setFromEuler(pebbleRotation),
      pebbleScale.set(stone.w, stone.h, stone.d),
    );
    for (let v = 0; v < perStone; v++) {
      roughenStone(vertex.fromBufferAttribute(source, v), s);
      // Squash the lower half so stones rest on, and sink into, the ground.
      if (vertex.y < 0) {
        vertex.y *= 0.35;
      }
      vertex.applyMatrix4(pebbleMatrix);
      const i = s * perStone + v;
      vertex.toArray(positions, i * 3);
      uvs[i * 2] = vertex.x / 5.5;
      uvs[i * 2 + 1] = (vertex.y + vertex.z) / 5.5;
      // Facet-level variation plus darker undersides where stones meet soil.
      const facet = 0.94 + ((Math.floor(v / 3) * 37 + s * 11) % 9) * 0.015;
      const under = vertex.y < stone.y ? 0.82 : 1;
      const shade = stone.shade * facet * under;
      colors[i * 3] = shade;
      colors[i * 3 + 1] = shade;
      colors[i * 3 + 2] = shade * 0.98;
    }
  });
  if (template !== pebble) {
    template.dispose();
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
  geometry.setAttribute("uv", new THREE.BufferAttribute(uvs, 2));
  geometry.setAttribute("color", new THREE.BufferAttribute(colors, 3));
  geometry.computeVertexNormals();
  const mesh = new THREE.Mesh(geometry, sandstoneMaterial());
  mesh.castShadow = mesh.receiveShadow = true;
  return mesh;
}

let dustMaterial: THREE.MeshStandardNodeMaterial | undefined;
/** Low, feathered sand apron: cosmetic sediment, never tall enough to imply cover. */
export function sandstoneFooting(w: number, d: number, variant: number): THREE.Mesh {
  if (!dustMaterial) {
    // Same drift colour and world-space grit as the soil, so the bank reads as
    // sand heaped against the rock rather than a painted ring.
    dustMaterial = new THREE.MeshStandardNodeMaterial({
      roughness: 1,
      vertexColors: true,
      transparent: true,
      depthWrite: false,
      polygonOffset: true,
      polygonOffsetFactor: -1,
      polygonOffsetUnits: -1,
    });
    dustMaterial.colorNode = vec4(quarrySoilAt().rgb.mul(quarryGrit(0.7).color).mul(1.05), 1);
  }
  const key = `footing/${w}/${d}/${variant}`;
  const cached = geometries.get(key);
  if (cached) {
    const mesh = new THREE.Mesh(cached, dustMaterial);
    mesh.receiveShadow = true;
    return mesh;
  }
  const shape = quarryRockShape(w, 1, d, variant).positions;
  const positions: number[] = [];
  const colors: number[] = [];
  const uvs: number[] = [];
  const indices: number[] = [];
  const rng = new Random(variant + 451);
  const sides = 16;
  const extensions = Array.from({ length: sides }, () => rng.range(0.5, 1.5));
  for (let ring = 0; ring < 3; ring++) {
    for (let side = 0; side < sides; side++) {
      const x = shape[side * 3];
      const z = shape[side * 3 + 2];
      const extension = ring === 0 ? -0.35 : extensions[side] * (ring === 1 ? 0.45 : 1);
      const px = x + Math.sign(x) * extension;
      const pz = z + Math.sign(z) * extension;
      positions.push(px, ring === 0 ? 0.18 : ring === 1 ? 0.055 : 0.018, pz);
      uvs.push(px / 8, pz / 8);
      const shade = ring === 0 ? 0.82 : 1;
      colors.push(shade, shade, shade, ring === 2 ? 0 : 1);
      if (ring < 2) {
        const a = ring * sides + side;
        const b = ring * sides + ((side + 1) % sides);
        indices.push(a, b, a + sides, b, b + sides, a + sides);
      }
    }
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 4));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  geometries.set(key, geometry);
  const mesh = new THREE.Mesh(geometry, dustMaterial);
  mesh.receiveShadow = true;
  return mesh;
}

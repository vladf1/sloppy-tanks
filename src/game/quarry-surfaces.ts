import * as THREE from "three";
import { toCreasedNormals } from "three/addons/utils/BufferGeometryUtils.js";
import { Random } from "./math";
import { quarryRockShape } from "./quarry-rock-shape";

let stone: THREE.MeshStandardMaterial | undefined;
const geometries = new Map<string, THREE.BufferGeometry>();

export function sandstoneMaterial(): THREE.MeshStandardMaterial {
  if (!stone) {
    const texture = new THREE.TextureLoader().load(
      `${import.meta.env?.BASE_URL ?? "/"}textures/quarry/sandstone.webp`,
    );
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.wrapS = texture.wrapT = THREE.MirroredRepeatWrapping;
    texture.anisotropy = 4;
    stone = new THREE.MeshStandardMaterial({
      map: texture,
      bumpMap: texture,
      bumpScale: 0.05,
      roughness: 0.97,
      vertexColors: true,
      color: 0xd4b28c,
    });
    // Blend projections across rounded shoulders instead of exposing UV seams on
    // individual triangles. World-space sampling decorrelates reused rock
    // geometry so identical boulders never show the same patch of grain.
    stone.onBeforeCompile = (shader) => {
      shader.vertexShader = shader.vertexShader
        .replace(
          "#include <common>",
          `#include <common>
        varying vec3 vStonePosition;
        varying vec3 vStoneNormal;
      `,
        )
        .replace(
          "#include <begin_vertex>",
          `#include <begin_vertex>
        vStonePosition = (modelMatrix * vec4(transformed, 1.0)).xyz;
        vStoneNormal = mat3(modelMatrix) * normal;
      `,
        );
      shader.fragmentShader = shader.fragmentShader
        .replace(
          "#include <common>",
          `#include <common>
        varying vec3 vStonePosition;
        varying vec3 vStoneNormal;
      `,
        )
        .replace(
          "#include <map_fragment>",
          `
        vec3 blend = pow(abs(normalize(vStoneNormal)), vec3(6.0));
        blend /= max(blend.x + blend.y + blend.z, 0.0001);
        vec3 stonePoint = vStonePosition / 5.5;
        vec4 stoneColor = texture2D(map, stonePoint.zy + vec2(0.31, 0.11)) * blend.x
          + texture2D(map, stonePoint.xz + vec2(0.57, 0.43)) * blend.y
          + texture2D(map, stonePoint.xy + vec2(0.13, 0.79)) * blend.z;
        diffuseColor *= stoneColor;
      `,
        );
    };
    stone.customProgramCacheKey = () => "quarry-triplanar-v2";
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

let dustMaterial: THREE.MeshStandardMaterial | undefined;
/** Low, feathered sand apron: cosmetic sediment, never tall enough to imply cover. */
export function sandstoneFooting(w: number, d: number, variant: number): THREE.Mesh {
  if (!dustMaterial) {
    const texture = new THREE.TextureLoader().load(
      `${import.meta.env?.BASE_URL ?? "/"}textures/ground/packed-dirt.webp`,
    );
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.wrapS = texture.wrapT = THREE.MirroredRepeatWrapping;
    dustMaterial = new THREE.MeshStandardMaterial({
      map: texture,
      color: 0x99a8b3,
      roughness: 1,
      vertexColors: true,
      transparent: true,
      depthWrite: false,
      polygonOffset: true,
      polygonOffsetFactor: -1,
      polygonOffsetUnits: -1,
    });
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
      const shade = ring === 0 ? 0.75 : 1;
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
